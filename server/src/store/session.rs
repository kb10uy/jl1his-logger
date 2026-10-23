use chrono::{DateTime, TimeDelta, Utc};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    db::Db,
    store::{
        StoreError, WriteTx,
        access::{Principal, UserAccess},
        hash_token, new_id, random_token, timestamp, user,
    },
};

pub const SESSION_TTL: TimeDelta = TimeDelta::days(30);
pub const SESSION_MAX: TimeDelta = TimeDelta::days(90);
const SESSION_REFRESH: TimeDelta = TimeDelta::days(1);
const MAX_USER_AGENT: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub user_agent: Option<String>,
    pub created_at: String,
    pub last_used_at: String,
    pub expires_at: String,
    pub(crate) token_hash: Vec<u8>,
}

impl Session {
    fn from_row(row: &SqliteRow) -> Result<Session, StoreError> {
        Ok(Session {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            user_agent: row.try_get("user_agent")?,
            created_at: row.try_get("created_at")?,
            last_used_at: row.try_get("last_used_at")?,
            expires_at: row.try_get("expires_at")?,
            token_hash: row.try_get("token_hash")?,
        })
    }
}

const SELECT: &str = "SELECT id, user_id, user_agent, created_at, last_used_at, expires_at, token_hash FROM sessions";

pub async fn create(tx: &mut WriteTx, user_id: &str, user_agent: Option<&str>) -> Result<String, StoreError> {
    if !user::is_active(tx, user_id).await? {
        return Err(StoreError::Unauthenticated);
    }
    let now = Utc::now();
    sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
        .bind(timestamp(now))
        .execute(&mut **tx)
        .await?;
    let plaintext = random_token();
    let user_agent = user_agent
        .map(|agent| agent.chars().take(MAX_USER_AGENT).collect::<String>())
        .filter(|agent| !agent.trim().is_empty());
    sqlx::query(
        "INSERT INTO sessions (id, token_hash, user_id, user_agent, created_at, last_used_at, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(hash_token(&plaintext))
    .bind(user_id)
    .bind(user_agent)
    .bind(timestamp(now))
    .bind(timestamp(now))
    .bind(timestamp(expiry(now, now)))
    .execute(&mut **tx)
    .await?;
    Ok(plaintext)
}

pub async fn authenticate(db: &Db, plaintext: &str) -> Result<Principal, StoreError> {
    let hash = hash_token(plaintext);
    let now = Utc::now();
    let (user_id, created_at, last_used_at): (String, String, String) = sqlx::query_as(
        "SELECT s.user_id, s.created_at, s.last_used_at FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token_hash = ? AND s.expires_at > ? AND u.disabled_at IS NULL",
    )
    .bind(&hash)
    .bind(timestamp(now))
    .fetch_optional(db)
    .await?
    .ok_or(StoreError::Unauthenticated)?;

    if last_used_at < timestamp(now - SESSION_REFRESH) {
        let created = DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| StoreError::Invalid(format!("session created_at {created_at:?}: {e}")))?
            .to_utc();
        let mut tx = WriteTx::begin(db).await?;
        sqlx::query("UPDATE sessions SET last_used_at = ?, expires_at = max(expires_at, ?) WHERE token_hash = ?")
            .bind(timestamp(now))
            .bind(timestamp(expiry(created, now)))
            .bind(&hash)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    Ok(Principal::user(&user_id))
}

fn expiry(created: DateTime<Utc>, now: DateTime<Utc>) -> DateTime<Utc> {
    (now + SESSION_TTL).min(created + SESSION_MAX)
}

pub async fn delete(tx: &mut WriteTx, plaintext: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(hash_token(plaintext))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn revalidate(conn: &mut SqliteConnection, hash: &[u8]) -> Result<(), StoreError> {
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token_hash = ? AND s.expires_at > ? AND u.disabled_at IS NULL)",
    )
    .bind(hash)
    .bind(timestamp(Utc::now()))
    .fetch_one(conn)
    .await?;
    match valid {
        true => Ok(()),
        false => Err(StoreError::Unauthenticated),
    }
}

pub async fn list(conn: &mut SqliteConnection, user: &UserAccess, user_id: &str) -> Result<Vec<Session>, StoreError> {
    let user = user.revalidate(conn).await?;
    if !user.may_manage(user_id) {
        return Err(StoreError::NotFound("user"));
    }
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "{SELECT} WHERE user_id = ? AND expires_at > ? ORDER BY last_used_at DESC, created_at DESC, id"
    )))
    .bind(user_id)
    .bind(timestamp(Utc::now()))
    .fetch_all(conn)
    .await?
    .iter()
    .map(Session::from_row)
    .collect()
}

pub async fn revoke(tx: &mut WriteTx, user: &UserAccess, id: &str) -> Result<(), StoreError> {
    let user = user.revalidate(tx).await?;
    let owner: Option<String> = sqlx::query_scalar("SELECT user_id FROM sessions WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
    match owner {
        Some(owner) if user.may_manage(&owner) => {}
        _ => return Err(StoreError::NotFound("session")),
    }
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::store::{now, testing::World};

    async fn signed_in(world: &World, user_agent: Option<&str>) -> String {
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let plaintext = create(&mut tx, &world.operator.id, user_agent).await.unwrap();
        tx.commit().await.unwrap();
        plaintext
    }

    async fn set(world: &World, sql: &'static str, value: String) {
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(value)
            .execute(&world.db)
            .await
            .unwrap();
    }

    async fn stored(world: &World) -> (String, String) {
        sqlx::query_as("SELECT last_used_at, expires_at FROM sessions")
            .fetch_one(&world.db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn signs_in_until_signed_out() {
        let world = World::new().await;
        let plaintext = signed_in(&world, Some("Mozilla/5.0 (Test)")).await;
        assert_eq!(
            authenticate(&world.db, &plaintext).await.unwrap(),
            Principal::user(&world.operator.id)
        );
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        delete(&mut tx, &plaintext).await.unwrap();
        tx.commit().await.unwrap();
        assert!(matches!(
            authenticate(&world.db, &plaintext).await,
            Err(StoreError::Unauthenticated)
        ));
    }

    #[rstest]
    #[case::valid(None, true)]
    #[case::deleted(Some("DELETE FROM sessions"), false)]
    #[case::expired(Some("UPDATE sessions SET expires_at = '2000-01-01T00:00:00Z'"), false)]
    #[case::disabled(Some("UPDATE users SET disabled_at = '2000-01-01T00:00:00Z'"), false)]
    #[tokio::test]
    async fn revalidates_without_renewing_the_session(#[case] revoke: Option<&str>, #[case] valid: bool) {
        let world = World::new().await;
        let plaintext = signed_in(&world, None).await;
        let expires_at = timestamp(Utc::now() + TimeDelta::days(3));
        set(&world, "UPDATE sessions SET expires_at = ?", expires_at.clone()).await;
        let mut conn = world.db.acquire().await.unwrap();
        if let Some(revoke) = revoke {
            sqlx::query(sqlx::AssertSqlSafe(revoke))
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        let result = super::revalidate(&mut conn, &hash_token(&plaintext)).await;
        if valid {
            result.unwrap();
            drop(conn);
            assert_eq!(stored(&world).await.1, expires_at);
        } else {
            assert!(matches!(result, Err(StoreError::Unauthenticated)));
        }
    }

    #[tokio::test]
    async fn extends_a_used_session_at_most_daily_and_never_past_ninety_days() {
        let world = World::new().await;
        let plaintext = signed_in(&world, None).await;
        let (created, expires) = stored(&world).await;
        authenticate(&world.db, &plaintext).await.unwrap();
        assert_eq!(stored(&world).await, (created, expires.clone()));

        let yesterday = timestamp(Utc::now() - TimeDelta::days(2));
        set(&world, "UPDATE sessions SET last_used_at = ?", yesterday.clone()).await;
        authenticate(&world.db, &plaintext).await.unwrap();
        let (used, renewed) = stored(&world).await;
        assert!(used > yesterday);
        assert!(renewed >= expires);

        let old = timestamp(Utc::now() - TimeDelta::days(80));
        set(&world, "UPDATE sessions SET created_at = ?", old).await;
        set(&world, "UPDATE sessions SET last_used_at = ?", yesterday).await;
        let soon = timestamp(Utc::now() + TimeDelta::days(5));
        set(&world, "UPDATE sessions SET expires_at = ?", soon.clone()).await;
        authenticate(&world.db, &plaintext).await.unwrap();
        let (_, capped) = stored(&world).await;
        assert!(capped > soon);
        assert!(capped < timestamp(Utc::now() + TimeDelta::days(11)), "{capped}");

        set(
            &world,
            "UPDATE sessions SET last_used_at = ?",
            timestamp(Utc::now() - TimeDelta::days(2)),
        )
        .await;
        authenticate(&world.db, &plaintext).await.unwrap();
        assert_eq!(stored(&world).await.1, capped);
    }

    #[tokio::test]
    async fn refuses_expired_sessions_and_disabled_users() {
        let world = World::new().await;
        let plaintext = signed_in(&world, None).await;
        set(
            &world,
            "UPDATE sessions SET expires_at = ?",
            timestamp(Utc::now() - TimeDelta::seconds(1)),
        )
        .await;
        assert!(matches!(
            authenticate(&world.db, &plaintext).await,
            Err(StoreError::Unauthenticated)
        ));

        sqlx::query("UPDATE users SET disabled_at = ? WHERE id = ?")
            .bind(now())
            .bind(&world.operator.id)
            .execute(&world.db)
            .await
            .unwrap();
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            create(&mut tx, &world.operator.id, None).await,
            Err(StoreError::Unauthenticated)
        ));
        let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(remaining, 1);
    }

    #[tokio::test]
    async fn lists_and_revokes_sessions_of_the_user_or_for_an_admin() {
        let world = World::new().await;
        let phone = signed_in(&world, Some(&"Phone ".repeat(100))).await;
        let laptop = signed_in(&world, Some("Laptop")).await;
        let operator = world.user_access(&world.operator).await;
        let outsider = world.user_access(&world.outsider).await;
        let admin = world.user_access(&world.admin).await;
        let mut conn = world.db.acquire().await.unwrap();

        let sessions = list(&mut conn, &operator, &world.operator.id).await.unwrap();
        assert_eq!(sessions.len(), 2);
        let agents: Vec<_> = sessions.iter().map(|s| s.user_agent.as_deref()).collect();
        assert!(agents.contains(&Some("Laptop")));
        let truncated = agents.iter().flatten().find(|a| a.starts_with("Phone")).unwrap();
        assert_eq!(truncated.chars().count(), MAX_USER_AGENT);
        assert!(sessions.iter().any(|s| s.token_hash == hash_token(&phone)));
        assert!(matches!(
            list(&mut conn, &outsider, &world.operator.id).await,
            Err(StoreError::NotFound(_))
        ));
        drop(conn);

        let (laptop_id, phone_id) = {
            let by_hash = |hash: Vec<u8>| sessions.iter().find(|s| s.token_hash == hash).unwrap().id.clone();
            (by_hash(hash_token(&laptop)), by_hash(hash_token(&phone)))
        };
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            revoke(&mut tx, &outsider, &laptop_id).await,
            Err(StoreError::NotFound(_))
        ));
        revoke(&mut tx, &operator, &laptop_id).await.unwrap();
        revoke(&mut tx, &admin, &phone_id).await.unwrap();
        assert!(matches!(
            revoke(&mut tx, &operator, &phone_id).await,
            Err(StoreError::NotFound(_))
        ));
        tx.commit().await.unwrap();
        assert!(authenticate(&world.db, &laptop).await.is_err());
        assert!(authenticate(&world.db, &phone).await.is_err());
    }
}
