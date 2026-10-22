use chrono::{TimeDelta, Utc};
use sqlx::SqliteConnection;

use crate::{
    db::Db,
    store::{StoreError, WriteTx, access::Principal, hash_token, random_token, timestamp},
};

pub const SESSION_TTL: TimeDelta = TimeDelta::days(30);
const SESSION_REFRESH: TimeDelta = TimeDelta::days(1);

pub async fn create(tx: &mut WriteTx, user_id: &str) -> Result<String, StoreError> {
    let active: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = ? AND disabled_at IS NULL)")
        .bind(user_id)
        .fetch_one(&mut **tx)
        .await?;
    if !active {
        return Err(StoreError::Unauthenticated);
    }
    let now = Utc::now();
    sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
        .bind(timestamp(now))
        .execute(&mut **tx)
        .await?;
    let plaintext = random_token();
    sqlx::query("INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)")
        .bind(hash_token(&plaintext))
        .bind(user_id)
        .bind(timestamp(now))
        .bind(timestamp(now + SESSION_TTL))
        .execute(&mut **tx)
        .await?;
    Ok(plaintext)
}

pub async fn authenticate(db: &Db, plaintext: &str) -> Result<Principal, StoreError> {
    let hash = hash_token(plaintext);
    let now = Utc::now();
    let (user_id, expires_at): (String, String) = sqlx::query_as(
        "SELECT s.user_id, s.expires_at FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token_hash = ? AND s.expires_at > ? AND u.disabled_at IS NULL",
    )
    .bind(&hash)
    .bind(timestamp(now))
    .fetch_optional(db)
    .await?
    .ok_or(StoreError::Unauthenticated)?;

    if expires_at < timestamp(now + SESSION_TTL - SESSION_REFRESH) {
        let mut tx = WriteTx::begin(db).await?;
        sqlx::query("UPDATE sessions SET expires_at = ? WHERE token_hash = ?")
            .bind(timestamp(now + SESSION_TTL))
            .bind(&hash)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    Ok(Principal::user(&user_id))
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

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::store::{now, testing::World};

    async fn signed_in(world: &World) -> String {
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let plaintext = create(&mut tx, &world.operator.id).await.unwrap();
        tx.commit().await.unwrap();
        plaintext
    }

    async fn set_expiry(world: &World, expires_at: String) {
        sqlx::query("UPDATE sessions SET expires_at = ?")
            .bind(expires_at)
            .execute(&world.db)
            .await
            .unwrap();
    }

    async fn expiry(world: &World) -> String {
        sqlx::query_scalar("SELECT expires_at FROM sessions")
            .fetch_one(&world.db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn signs_in_until_signed_out() {
        let world = World::new().await;
        let plaintext = signed_in(&world).await;
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
        let plaintext = signed_in(&world).await;
        let expires_at = timestamp(Utc::now() + TimeDelta::days(3));
        set_expiry(&world, expires_at.clone()).await;
        let mut conn = world.db.acquire().await.unwrap();
        if let Some(revoke) = revoke {
            sqlx::query(sqlx::AssertSqlSafe(revoke))
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        let result = revalidate(&mut conn, &hash_token(&plaintext)).await;
        if valid {
            result.unwrap();
            drop(conn);
            assert_eq!(expiry(&world).await, expires_at);
        } else {
            assert!(matches!(result, Err(StoreError::Unauthenticated)));
        }
    }

    #[tokio::test]
    async fn extends_a_used_session_at_most_daily() {
        let world = World::new().await;
        let plaintext = signed_in(&world).await;

        let recent = timestamp(Utc::now() + SESSION_TTL - TimeDelta::hours(1));
        set_expiry(&world, recent.clone()).await;
        authenticate(&world.db, &plaintext).await.unwrap();
        assert_eq!(expiry(&world).await, recent);

        let old = timestamp(Utc::now() + TimeDelta::days(3));
        set_expiry(&world, old.clone()).await;
        authenticate(&world.db, &plaintext).await.unwrap();
        assert!(expiry(&world).await > timestamp(Utc::now() + SESSION_TTL - TimeDelta::hours(1)));
    }

    #[tokio::test]
    async fn refuses_expired_sessions_and_disabled_users() {
        let world = World::new().await;
        let plaintext = signed_in(&world).await;
        set_expiry(&world, timestamp(Utc::now() - TimeDelta::seconds(1))).await;
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
            create(&mut tx, &world.operator.id).await,
            Err(StoreError::Unauthenticated)
        ));
        let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(remaining, 1);
    }
}
