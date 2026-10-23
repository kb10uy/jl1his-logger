use serde::Serialize;
use sqlx::{AssertSqlSafe, Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    domain::callsign,
    store::{StoreError, WriteTx, access::AdminAccess, is_unique_violation, new_id, now},
};

const SELECT: &str = "SELECT id, callsign, role, disabled_at FROM users";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
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
    sqlx::query(AssertSqlSafe(format!("{SELECT} WHERE callsign = ?")))
        .bind(callsign)
        .fetch_optional(conn)
        .await?
        .map(|row| User::from_row(&row))
        .transpose()
}

pub async fn list(conn: &mut SqliteConnection, admin: &AdminAccess) -> Result<Vec<User>, StoreError> {
    admin.revalidate(conn).await?;
    sqlx::query(AssertSqlSafe(format!("{SELECT} ORDER BY callsign")))
        .fetch_all(conn)
        .await?
        .iter()
        .map(User::from_row)
        .collect()
}

pub async fn set_disabled(
    tx: &mut WriteTx,
    admin: &AdminAccess,
    user_id: &str,
    disabled: bool,
) -> Result<User, StoreError> {
    admin.revalidate(tx).await?;
    let user = find(tx, user_id).await?.ok_or(StoreError::NotFound("user"))?;
    if user.disabled_at.is_some() == disabled {
        return Ok(user);
    }
    if disabled {
        if user.role == Role::Admin {
            keep_another_admin(tx, user_id).await?;
        }
        let sole: Option<String> = sqlx::query_scalar(
            "SELECT l.name FROM memberships m JOIN logbooks l ON l.id = m.logbook_id \
             WHERE m.user_id = ? AND m.role = 'owner' AND NOT EXISTS ( \
               SELECT 1 FROM memberships o JOIN users u ON u.id = o.user_id \
               WHERE o.logbook_id = m.logbook_id AND o.role = 'owner' AND o.user_id <> m.user_id \
               AND u.disabled_at IS NULL) \
             ORDER BY l.name LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(name) = sole {
            return Err(StoreError::Conflict(format!(
                "{} is the only active owner of {name}; set another owner first",
                user.callsign
            )));
        }
        for sql in [
            "DELETE FROM sessions WHERE user_id = ?",
            "DELETE FROM api_tokens WHERE user_id = ?",
            "DELETE FROM registration_tokens WHERE user_id = ? AND used_at IS NULL",
        ] {
            sqlx::query(sql).bind(user_id).execute(&mut **tx).await?;
        }
    }
    let now = now();
    sqlx::query("UPDATE users SET disabled_at = ?, updated_at = ? WHERE id = ?")
        .bind(disabled.then_some(&now))
        .bind(&now)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    find(tx, user_id).await?.ok_or(StoreError::NotFound("user"))
}

pub async fn set_role(tx: &mut WriteTx, admin: &AdminAccess, user_id: &str, role: Role) -> Result<User, StoreError> {
    admin.revalidate(tx).await?;
    let user = find(tx, user_id).await?.ok_or(StoreError::NotFound("user"))?;
    if user.role == role {
        return Ok(user);
    }
    if user.role == Role::Admin && user.disabled_at.is_none() {
        keep_another_admin(tx, user_id).await?;
    }
    sqlx::query("UPDATE users SET role = ?, updated_at = ? WHERE id = ?")
        .bind(role.as_str())
        .bind(now())
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    find(tx, user_id).await?.ok_or(StoreError::NotFound("user"))
}

async fn keep_another_admin(tx: &mut WriteTx, user_id: &str) -> Result<(), StoreError> {
    let others: i64 =
        sqlx::query_scalar("SELECT count(*) FROM users WHERE role = 'admin' AND disabled_at IS NULL AND id <> ?")
            .bind(user_id)
            .fetch_one(&mut **tx)
            .await?;
    match others {
        0 => Err(StoreError::Conflict(
            "the instance keeps at least one active admin".into(),
        )),
        _ => Ok(()),
    }
}

pub(crate) async fn find(conn: &mut SqliteConnection, id: &str) -> Result<Option<User>, StoreError> {
    sqlx::query(AssertSqlSafe(format!("{SELECT} WHERE id = ?")))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .map(|row| User::from_row(&row))
        .transpose()
}

pub(crate) async fn is_active(conn: &mut SqliteConnection, id: &str) -> Result<bool, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = ? AND disabled_at IS NULL)")
            .bind(id)
            .fetch_one(conn)
            .await?,
    )
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
    use sqlx::AssertSqlSafe;

    use super::*;
    use crate::store::{registration, session, testing::World, token};

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
    async fn keeps_one_active_admin() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        for refused in [
            set_disabled(&mut tx, &admin, &world.admin.id, true).await,
            set_role(&mut tx, &admin, &world.admin.id, Role::User).await,
        ] {
            assert!(matches!(refused, Err(StoreError::Conflict(_))), "{refused:?}");
        }
        set_role(&mut tx, &admin, &world.operator.id, Role::Admin)
            .await
            .unwrap();
        let demoted = set_role(&mut tx, &admin, &world.admin.id, Role::User).await.unwrap();
        assert_eq!(demoted.role, Role::User);
        assert!(matches!(admin.revalidate(&mut tx).await, Err(StoreError::Forbidden(_))));
    }

    #[tokio::test]
    async fn keeps_one_active_owner_of_each_logbook() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            set_disabled(&mut tx, &admin, &world.owner.id, true).await,
            Err(StoreError::Conflict(_))
        ));
        sqlx::query("UPDATE memberships SET role = 'owner' WHERE user_id = ?")
            .bind(&world.operator.id)
            .execute(&mut *tx)
            .await
            .unwrap();
        set_disabled(&mut tx, &admin, &world.operator.id, true).await.unwrap();
        assert!(matches!(
            set_disabled(&mut tx, &admin, &world.owner.id, true).await,
            Err(StoreError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn disables_and_enables_a_user_without_their_credentials() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let operator = world.user_access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        session::create(&mut tx, &world.operator.id, None).await.unwrap();
        let input = token::TokenInput {
            kind: token::TokenKind::Native,
            name: "hislog".into(),
            logbook_ids: vec![world.logbook.id.clone()],
            wl2_prefix: false,
        };
        token::issue(&mut tx, &operator, &input).await.unwrap();
        registration::issue(&mut tx, &admin, &world.operator.id, registration::DEFAULT_TTL)
            .await
            .unwrap();

        let disabled = set_disabled(&mut tx, &admin, &world.operator.id, true).await.unwrap();
        assert!(disabled.disabled_at.is_some());
        for table in ["sessions", "api_tokens", "registration_tokens"] {
            let count: i64 = sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            assert_eq!(count, 0, "{table}");
        }
        let enabled = set_disabled(&mut tx, &admin, &world.operator.id, false).await.unwrap();
        assert_eq!(enabled, world.operator);
        assert_eq!(list(&mut tx, &admin).await.unwrap().len(), 4);
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
