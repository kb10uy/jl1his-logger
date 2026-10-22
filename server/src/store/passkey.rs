use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use crate::store::{StoreError, WriteTx, access::UserAccess, is_unique_violation, new_id, now};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPasskey {
    pub credential_id: Vec<u8>,
    pub passkey: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passkey {
    pub id: String,
    pub user_id: String,
    pub credential_id: Vec<u8>,
    pub passkey: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

impl Passkey {
    fn from_row(row: &SqliteRow) -> Result<Passkey, StoreError> {
        Ok(Passkey {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            credential_id: row.try_get("credential_id")?,
            passkey: row.try_get("passkey")?,
            name: row.try_get("name")?,
            created_at: row.try_get("created_at")?,
            last_used_at: row.try_get("last_used_at")?,
        })
    }
}

pub async fn add(tx: &mut WriteTx, user: &UserAccess, new: &NewPasskey) -> Result<Passkey, StoreError> {
    let user = user.revalidate(tx).await?;
    insert(tx, user.user_id(), new).await
}

pub async fn list(conn: &mut SqliteConnection, user: &UserAccess, user_id: &str) -> Result<Vec<Passkey>, StoreError> {
    let user = user.revalidate(conn).await?;
    if !user.may_manage(user_id) {
        return Err(StoreError::NotFound("user"));
    }
    of_user(conn, user_id).await
}

pub async fn delete(tx: &mut WriteTx, user: &UserAccess, id: &str) -> Result<(), StoreError> {
    let user = user.revalidate(tx).await?;
    let owner: Option<String> = sqlx::query_scalar("SELECT user_id FROM passkeys WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
    let owner = match owner {
        Some(owner) if user.may_manage(&owner) => owner,
        _ => return Err(StoreError::NotFound("passkey")),
    };
    if owner == user.user_id() {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM passkeys WHERE user_id = ?")
            .bind(&owner)
            .fetch_one(&mut **tx)
            .await?;
        if count <= 1 {
            return Err(StoreError::Conflict("your last passkey cannot be deleted".into()));
        }
    }
    sqlx::query("DELETE FROM passkeys WHERE id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn find_by_credential(conn: &mut SqliteConnection, credential_id: &[u8]) -> Result<Passkey, StoreError> {
    let row = sqlx::query(
        "SELECT p.id, p.user_id, p.credential_id, p.passkey, p.name, p.created_at, p.last_used_at \
         FROM passkeys p JOIN users u ON u.id = p.user_id \
         WHERE p.credential_id = ? AND u.disabled_at IS NULL",
    )
    .bind(credential_id)
    .fetch_optional(conn)
    .await?
    .ok_or(StoreError::Unauthenticated)?;
    Passkey::from_row(&row)
}

pub async fn record_use(tx: &mut WriteTx, id: &str, passkey: &str) -> Result<(), StoreError> {
    check_json(passkey)?;
    let updated = sqlx::query("UPDATE passkeys SET passkey = ?, last_used_at = ? WHERE id = ?")
        .bind(passkey)
        .bind(now())
        .bind(id)
        .execute(&mut **tx)
        .await?;
    match updated.rows_affected() {
        0 => Err(StoreError::NotFound("passkey")),
        _ => Ok(()),
    }
}

pub(crate) async fn insert(tx: &mut WriteTx, user_id: &str, new: &NewPasskey) -> Result<Passkey, StoreError> {
    let name = new.name.trim();
    if name.is_empty() {
        return Err(StoreError::Invalid("a passkey needs a name".into()));
    }
    if new.credential_id.is_empty() {
        return Err(StoreError::Invalid("a passkey needs a credential ID".into()));
    }
    check_json(&new.passkey)?;
    let id = new_id();
    let inserted = sqlx::query(
        "INSERT INTO passkeys (id, user_id, credential_id, passkey, name, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&new.credential_id)
    .bind(&new.passkey)
    .bind(name)
    .bind(now())
    .execute(&mut **tx)
    .await;
    match inserted {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            return Err(StoreError::Conflict("the passkey is already registered".into()));
        }
        Err(e) => return Err(e.into()),
    }
    let row = sqlx::query(
        "SELECT id, user_id, credential_id, passkey, name, created_at, last_used_at FROM passkeys WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&mut **tx)
    .await?;
    Passkey::from_row(&row)
}

pub(crate) async fn of_user(conn: &mut SqliteConnection, user_id: &str) -> Result<Vec<Passkey>, StoreError> {
    sqlx::query(
        "SELECT id, user_id, credential_id, passkey, name, created_at, last_used_at FROM passkeys          WHERE user_id = ? ORDER BY created_at, id",
    )
        .bind(user_id)
        .fetch_all(conn)
        .await?
        .iter()
        .map(Passkey::from_row)
        .collect()
}

fn check_json(passkey: &str) -> Result<(), StoreError> {
    serde_json::from_str::<serde_json::Value>(passkey)
        .map(|_| ())
        .map_err(|e| StoreError::Invalid(format!("a passkey is stored as JSON: {e}")))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::store::testing::World;

    pub(crate) fn new_passkey(credential_id: u8) -> NewPasskey {
        NewPasskey {
            credential_id: vec![credential_id],
            passkey: format!(r#"{{"cred":{credential_id}}}"#),
            name: " Phone ".into(),
        }
    }

    #[tokio::test]
    async fn adds_passkeys_and_finds_them_by_credential() {
        let world = World::new().await;
        let user = world.user_access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let added = add(&mut tx, &user, &new_passkey(1)).await.unwrap();
        assert_eq!(
            (added.name.as_str(), added.user_id.as_str()),
            ("Phone", world.operator.id.as_str())
        );
        assert!(matches!(
            add(&mut tx, &user, &new_passkey(1)).await,
            Err(StoreError::Conflict(_))
        ));
        let bad = NewPasskey {
            passkey: "not json".into(),
            ..new_passkey(2)
        };
        assert!(matches!(add(&mut tx, &user, &bad).await, Err(StoreError::Invalid(_))));

        assert_eq!(find_by_credential(&mut tx, &[1]).await.unwrap(), added);
        record_use(&mut tx, &added.id, r#"{"cred":1,"counter":2}"#)
            .await
            .unwrap();
        let used = find_by_credential(&mut tx, &[1]).await.unwrap();
        assert_eq!(used.passkey, r#"{"cred":1,"counter":2}"#);
        assert!(used.last_used_at.is_some());
        assert!(matches!(
            find_by_credential(&mut tx, &[9]).await,
            Err(StoreError::Unauthenticated)
        ));
    }

    #[tokio::test]
    async fn keeps_the_last_passkey_of_its_own_user() {
        let world = World::new().await;
        let user = world.user_access(&world.operator).await;
        let admin = world.user_access(&world.admin).await;
        let outsider = world.user_access(&world.outsider).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let first = add(&mut tx, &user, &new_passkey(1)).await.unwrap();
        let second = add(&mut tx, &user, &new_passkey(2)).await.unwrap();

        assert!(matches!(
            delete(&mut tx, &outsider, &first.id).await,
            Err(StoreError::NotFound(_))
        ));
        assert!(matches!(
            list(&mut tx, &outsider, &world.operator.id).await,
            Err(StoreError::NotFound(_))
        ));
        delete(&mut tx, &user, &first.id).await.unwrap();
        assert!(matches!(
            delete(&mut tx, &user, &second.id).await,
            Err(StoreError::Conflict(_))
        ));
        delete(&mut tx, &admin, &second.id).await.unwrap();
        assert!(list(&mut tx, &user, &world.operator.id).await.unwrap().is_empty());
    }
}
