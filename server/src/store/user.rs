use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    domain::callsign,
    store::{StoreError, WriteTx, access::AdminAccess, is_unique_violation, new_id, now},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Admin,
    User,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::User => "user",
        }
    }

    pub(crate) fn parse(s: &str) -> Result<Role, StoreError> {
        match s {
            "admin" => Ok(Role::Admin),
            "user" => Ok(Role::User),
            _ => Err(StoreError::Invalid(format!("unknown user role {s:?}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub callsign: String,
    pub role: Role,
    pub disabled_at: Option<String>,
}

impl User {
    fn from_row(row: &SqliteRow) -> Result<User, StoreError> {
        Ok(User {
            id: row.try_get("id")?,
            callsign: row.try_get("callsign")?,
            role: Role::parse(row.try_get("role")?)?,
            disabled_at: row.try_get("disabled_at")?,
        })
    }
}

pub(crate) fn normalize_callsign(callsign: &str) -> Result<String, StoreError> {
    callsign::normalize(callsign).ok_or_else(|| StoreError::Invalid(format!("{callsign:?} is not a callsign")))
}

pub async fn create_initial_admin(tx: &mut WriteTx, callsign: &str) -> Result<User, StoreError> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&mut **tx)
        .await?;
    if count > 0 {
        return Err(StoreError::Conflict("the initial admin already exists".into()));
    }
    insert(tx, callsign, Role::Admin).await
}

pub async fn create(tx: &mut WriteTx, admin: &AdminAccess, callsign: &str, role: Role) -> Result<User, StoreError> {
    admin.revalidate(tx).await?;
    insert(tx, callsign, role).await
}

pub async fn find_by_callsign(conn: &mut SqliteConnection, callsign: &str) -> Result<Option<User>, StoreError> {
    let callsign = normalize_callsign(callsign)?;
    sqlx::query("SELECT id, callsign, role, disabled_at FROM users WHERE callsign = ?")
        .bind(callsign)
        .fetch_optional(conn)
        .await?
        .map(|row| User::from_row(&row))
        .transpose()
}

pub(crate) async fn find(conn: &mut SqliteConnection, id: &str) -> Result<Option<User>, StoreError> {
    sqlx::query("SELECT id, callsign, role, disabled_at FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .map(|row| User::from_row(&row))
        .transpose()
}

async fn insert(tx: &mut WriteTx, callsign: &str, role: Role) -> Result<User, StoreError> {
    let callsign = normalize_callsign(callsign)?;
    let id = new_id();
    let now = now();
    let inserted = sqlx::query("INSERT INTO users (id, callsign, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(&callsign)
        .bind(role.as_str())
        .bind(&now)
        .bind(&now)
        .execute(&mut **tx)
        .await;
    match inserted {
        Ok(_) => Ok(User {
            id,
            callsign,
            role,
            disabled_at: None,
        }),
        Err(e) if is_unique_violation(&e) => Err(StoreError::Conflict(format!("{callsign} already has a user"))),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testing::World;

    #[tokio::test]
    async fn creates_the_initial_admin_only_once() {
        let world = World::new().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let again = create_initial_admin(&mut tx, "JA1NEW").await;
        assert!(matches!(again, Err(StoreError::Conflict(_))), "{again:?}");
    }

    #[tokio::test]
    async fn refuses_a_second_user_with_the_same_callsign() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let again = create(&mut tx, &admin, "ja1own", Role::User).await;
        assert!(matches!(again, Err(StoreError::Conflict(_))), "{again:?}");
    }

    #[tokio::test]
    async fn finds_users_by_callsign() {
        let world = World::new().await;
        let mut conn = world.db.acquire().await.unwrap();
        assert_eq!(
            find_by_callsign(&mut conn, "ja1own").await.unwrap(),
            Some(world.owner.clone())
        );
        assert_eq!(find_by_callsign(&mut conn, "JA1NOB").await.unwrap(), None);
    }
}
