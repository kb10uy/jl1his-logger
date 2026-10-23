use sqlx::SqliteConnection;

use crate::store::{StoreError, WriteTx, access::AdminAccess, now};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

pub async fn get(conn: &mut SqliteConnection, key: &str) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(conn)
        .await?)
}

pub async fn list(conn: &mut SqliteConnection, admin: &AdminAccess) -> Result<Vec<Setting>, StoreError> {
    admin.revalidate(conn).await?;
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT key, value, updated_at FROM settings ORDER BY key")
            .fetch_all(conn)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(key, value, updated_at)| Setting { key, value, updated_at })
        .collect())
}

pub async fn set(tx: &mut WriteTx, admin: &AdminAccess, key: &str, value: &str) -> Result<(), StoreError> {
    admin.revalidate(tx).await?;
    let key = key.trim();
    if key.is_empty() {
        return Err(StoreError::Invalid("a setting needs a key".into()));
    }
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(now())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testing::World;

    #[tokio::test]
    async fn sets_and_reads_instance_settings() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert_eq!(get(&mut tx, "motd").await.unwrap(), None);
        set(&mut tx, &admin, " motd ", "73").await.unwrap();
        set(&mut tx, &admin, "motd", "88").await.unwrap();
        assert_eq!(get(&mut tx, "motd").await.unwrap().as_deref(), Some("88"));
        set(&mut tx, &admin, "banner", "").await.unwrap();
        let listed: Vec<(String, String)> = list(&mut tx, &admin)
            .await
            .unwrap()
            .into_iter()
            .map(|s| (s.key, s.value))
            .collect();
        assert_eq!(listed, [("banner".into(), String::new()), ("motd".into(), "88".into())]);
        assert!(matches!(
            set(&mut tx, &admin, "", "x").await,
            Err(StoreError::Invalid(_))
        ));
    }
}
