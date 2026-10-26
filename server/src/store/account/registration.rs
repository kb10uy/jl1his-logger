use chrono::{TimeDelta, Utc};
use sqlx::SqliteConnection;

use crate::store::{
    StoreError, WriteTx,
    access::AdminAccess,
    account::passkey::{self, NewPasskey, Passkey},
    account::user::{self, Role, User},
    hash_token, random_token, timestamp,
};

pub const DEFAULT_TTL: TimeDelta = TimeDelta::hours(24);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub user: User,
    pub credential_ids: Vec<Vec<u8>>,
}

pub async fn issue(tx: &mut WriteTx, admin: &AdminAccess, user_id: &str, ttl: TimeDelta) -> Result<String, StoreError> {
    let admin = admin.revalidate(tx).await?;
    insert(tx, user_id, Some(admin.user_id()), ttl).await
}

pub async fn issue_for_setup(tx: &mut WriteTx, user_id: &str, ttl: TimeDelta) -> Result<String, StoreError> {
    let user = user::find(tx, user_id).await?.ok_or(StoreError::NotFound("user"))?;
    if user.role != Role::Admin || !passkey::of_user(tx, user_id).await?.is_empty() {
        return Err(StoreError::Conflict(
            format!("{} is not an admin waiting for a first passkey", user.callsign).into(),
        ));
    }
    insert(tx, user_id, None, ttl).await
}

pub async fn check(conn: &mut SqliteConnection, plaintext: &str) -> Result<Registration, StoreError> {
    let user_id: String = sqlx::query_scalar(
        "SELECT r.user_id FROM registration_tokens r JOIN users u ON u.id = r.user_id \
         WHERE r.token_hash = ? AND r.used_at IS NULL AND r.expires_at > ? AND u.disabled_at IS NULL",
    )
    .bind(hash_token(plaintext))
    .bind(timestamp(Utc::now()))
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(StoreError::Unauthenticated)?;
    let user = user::find(conn, &user_id).await?.ok_or(StoreError::Unauthenticated)?;
    let credential_ids = passkey::of_user(conn, &user_id)
        .await?
        .into_iter()
        .map(|p| p.credential_id)
        .collect();
    Ok(Registration { user, credential_ids })
}

pub async fn complete(tx: &mut WriteTx, plaintext: &str, new: &NewPasskey) -> Result<(User, Passkey), StoreError> {
    let now = timestamp(Utc::now());
    let user_id: String = sqlx::query_scalar(
        "UPDATE registration_tokens SET used_at = ? \
         WHERE token_hash = ? AND used_at IS NULL AND expires_at > ? \
         AND user_id IN (SELECT id FROM users WHERE disabled_at IS NULL) \
         RETURNING user_id",
    )
    .bind(&now)
    .bind(hash_token(plaintext))
    .bind(&now)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(StoreError::Unauthenticated)?;
    let user = user::find(tx, &user_id).await?.ok_or(StoreError::Unauthenticated)?;
    let passkey = passkey::insert(tx, &user_id, new).await?;
    Ok((user, passkey))
}

async fn insert(
    tx: &mut WriteTx,
    user_id: &str,
    created_by: Option<&str>,
    ttl: TimeDelta,
) -> Result<String, StoreError> {
    if ttl <= TimeDelta::zero() {
        return Err(StoreError::Invalid(
            "a registration link needs a positive lifetime".into(),
        ));
    }
    if !user::is_active(tx, user_id).await? {
        return Err(StoreError::NotFound("user"));
    }
    sqlx::query("DELETE FROM registration_tokens WHERE user_id = ? AND used_at IS NULL")
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    let now = Utc::now();
    let plaintext = random_token();
    sqlx::query(
        "INSERT INTO registration_tokens (token_hash, user_id, created_by, created_at, expires_at) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(hash_token(&plaintext))
    .bind(user_id)
    .bind(created_by)
    .bind(timestamp(now))
    .bind(timestamp(now + ttl))
    .execute(&mut **tx)
    .await?;
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{account::passkey::tests::new_passkey, testing::World};

    #[tokio::test]
    async fn registers_a_passkey_once_per_link() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let link = issue(&mut tx, &admin, &world.operator.id, DEFAULT_TTL).await.unwrap();
        let checked = check(&mut tx, &link).await.unwrap();
        assert_eq!(checked.user, world.operator);
        assert!(checked.credential_ids.is_empty());

        let (user, passkey) = complete(&mut tx, &link, &new_passkey(1)).await.unwrap();
        assert_eq!(
            (user.id.as_str(), passkey.user_id.as_str()),
            (world.operator.id.as_str(), world.operator.id.as_str())
        );
        assert!(matches!(check(&mut tx, &link).await, Err(StoreError::Unauthenticated)));
        assert!(matches!(
            complete(&mut tx, &link, &new_passkey(2)).await,
            Err(StoreError::Unauthenticated)
        ));

        let again = issue(&mut tx, &admin, &world.operator.id, DEFAULT_TTL).await.unwrap();
        assert_eq!(check(&mut tx, &again).await.unwrap().credential_ids, [vec![1]]);
    }

    #[tokio::test]
    async fn keeps_only_the_latest_unexpired_link() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let first = issue(&mut tx, &admin, &world.operator.id, DEFAULT_TTL).await.unwrap();
        let second = issue(&mut tx, &admin, &world.operator.id, DEFAULT_TTL).await.unwrap();
        assert!(matches!(check(&mut tx, &first).await, Err(StoreError::Unauthenticated)));
        assert!(check(&mut tx, &second).await.is_ok());

        sqlx::query("UPDATE registration_tokens SET expires_at = ?")
            .bind(timestamp(Utc::now() - TimeDelta::seconds(1)))
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(matches!(
            complete(&mut tx, &second, &new_passkey(1)).await,
            Err(StoreError::Unauthenticated)
        ));
        assert!(matches!(
            issue(&mut tx, &admin, &world.operator.id, TimeDelta::zero()).await,
            Err(StoreError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn issues_setup_links_only_for_an_admin_without_passkeys() {
        let world = World::new().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            issue_for_setup(&mut tx, &world.operator.id, DEFAULT_TTL).await,
            Err(StoreError::Conflict(_))
        ));
        let link = issue_for_setup(&mut tx, &world.admin.id, DEFAULT_TTL).await.unwrap();
        complete(&mut tx, &link, &new_passkey(1)).await.unwrap();
        assert!(matches!(
            issue_for_setup(&mut tx, &world.admin.id, DEFAULT_TTL).await,
            Err(StoreError::Conflict(_))
        ));
    }
}
