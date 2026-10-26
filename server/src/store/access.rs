use std::{
    ops::Deref,
    sync::{Arc, Weak},
};

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection};

use crate::store::{StoreError, WriteTx, account::user::Role, new_id, now};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    User { user_id: String },
    Token { user_id: String, token_id: String },
}

impl Principal {
    pub fn user(user_id: &str) -> Principal {
        Principal::User {
            user_id: user_id.to_owned(),
        }
    }

    fn user_id(&self) -> &str {
        match self {
            Principal::User { user_id } | Principal::Token { user_id, .. } => user_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogbookRole {
    Owner,
    Operator,
}

impl LogbookRole {
    pub fn as_str(self) -> &'static str {
        match self {
            LogbookRole::Owner => "owner",
            LogbookRole::Operator => "operator",
        }
    }

    pub(crate) fn parse(s: &str) -> Result<LogbookRole, StoreError> {
        match s {
            "owner" => Ok(LogbookRole::Owner),
            "operator" => Ok(LogbookRole::Operator),
            _ => Err(StoreError::Invalid(format!("unknown Logbook role {s:?}").into())),
        }
    }
}

#[derive(Debug, Clone)]
enum Authorization {
    Member(Principal),
    Rescue { id: String, scope: Weak<()> },
}

#[derive(Debug, Clone)]
pub struct LogbookAccess {
    logbook_id: String,
    user_id: String,
    callsign: String,
    role: LogbookRole,
    authorization: Authorization,
}

impl LogbookAccess {
    pub fn logbook_id(&self) -> &str {
        &self.logbook_id
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn callsign(&self) -> &str {
        &self.callsign
    }

    pub fn role(&self) -> LogbookRole {
        self.role
    }

    pub fn owner(&self) -> Result<OwnerAccess<'_>, StoreError> {
        match self.role {
            LogbookRole::Owner => Ok(OwnerAccess(self)),
            LogbookRole::Operator => Err(StoreError::Forbidden("only an owner of the Logbook can do this")),
        }
    }

    pub(super) async fn revalidate(&self, conn: &mut SqliteConnection) -> Result<LogbookAccess, StoreError> {
        match &self.authorization {
            Authorization::Member(principal) => logbook(conn, principal, &self.logbook_id).await,
            Authorization::Rescue { id, scope } => {
                let recorded: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM rescue_operations WHERE id = ? AND logbook_id = ? AND actor_id = ?)",
                )
                .bind(id)
                .bind(&self.logbook_id)
                .bind(&self.user_id)
                .fetch_one(&mut *conn)
                .await?;
                if !recorded || scope.strong_count() == 0 {
                    return Err(StoreError::NotFound("Logbook"));
                }
                let admin = admin(conn, &Principal::user(&self.user_id)).await?;
                Ok(LogbookAccess {
                    callsign: admin.callsign,
                    ..self.clone()
                })
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OwnerAccess<'a>(&'a LogbookAccess);

impl OwnerAccess<'_> {
    pub(super) async fn revalidate(&self, conn: &mut SqliteConnection) -> Result<(), StoreError> {
        self.0.revalidate(conn).await?.owner()?;
        Ok(())
    }
}

impl Deref for OwnerAccess<'_> {
    type Target = LogbookAccess;

    fn deref(&self) -> &LogbookAccess {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminAccess {
    user_id: String,
    callsign: String,
}

impl AdminAccess {
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub(super) async fn revalidate(&self, conn: &mut SqliteConnection) -> Result<AdminAccess, StoreError> {
        admin(conn, &Principal::user(&self.user_id)).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAccess {
    user_id: String,
    callsign: String,
    role: Role,
}

impl UserAccess {
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn callsign(&self) -> &str {
        &self.callsign
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub(super) async fn revalidate(&self, conn: &mut SqliteConnection) -> Result<UserAccess, StoreError> {
        user(conn, &Principal::user(&self.user_id)).await
    }

    pub(super) fn may_manage(&self, user_id: &str) -> bool {
        self.user_id == user_id || self.role == Role::Admin
    }
}

pub async fn logbook(
    conn: &mut SqliteConnection,
    principal: &Principal,
    logbook_id: &str,
) -> Result<LogbookAccess, StoreError> {
    let row = sqlx::query(
        "SELECT u.callsign, m.role FROM users u JOIN memberships m ON m.user_id = u.id \
         WHERE u.id = ? AND u.disabled_at IS NULL AND m.logbook_id = ?",
    )
    .bind(principal.user_id())
    .bind(logbook_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(StoreError::NotFound("Logbook"))?;

    if let Principal::Token { user_id, token_id } = principal {
        let scoped: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM api_tokens t JOIN api_token_scopes s ON s.token_id = t.id \
             WHERE t.id = ? AND t.user_id = ? AND s.logbook_id = ?)",
        )
        .bind(token_id)
        .bind(user_id)
        .bind(logbook_id)
        .fetch_one(&mut *conn)
        .await?;
        if !scoped {
            return Err(StoreError::NotFound("Logbook"));
        }
    }

    Ok(LogbookAccess {
        logbook_id: logbook_id.to_owned(),
        user_id: principal.user_id().to_owned(),
        callsign: row.try_get("callsign")?,
        role: LogbookRole::parse(row.try_get("role")?)?,
        authorization: Authorization::Member(principal.clone()),
    })
}

pub async fn admin(conn: &mut SqliteConnection, principal: &Principal) -> Result<AdminAccess, StoreError> {
    let Principal::User { user_id } = principal else {
        return Err(StoreError::Forbidden("API tokens cannot administer the instance"));
    };
    let callsign: Option<String> =
        sqlx::query_scalar("SELECT callsign FROM users WHERE id = ? AND role = 'admin' AND disabled_at IS NULL")
            .bind(user_id)
            .fetch_optional(conn)
            .await?;
    match callsign {
        Some(callsign) => Ok(AdminAccess {
            user_id: user_id.clone(),
            callsign,
        }),
        None => Err(StoreError::Forbidden("only an admin can do this")),
    }
}

pub async fn user(conn: &mut SqliteConnection, principal: &Principal) -> Result<UserAccess, StoreError> {
    let Principal::User { user_id } = principal else {
        return Err(StoreError::Forbidden("API tokens cannot manage accounts"));
    };
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT callsign, role FROM users WHERE id = ? AND disabled_at IS NULL")
            .bind(user_id)
            .fetch_optional(conn)
            .await?;
    let (callsign, role) = row.ok_or(StoreError::Unauthenticated)?;
    Ok(UserAccess {
        user_id: user_id.clone(),
        callsign,
        role: Role::parse(&role)?,
    })
}

pub async fn rescue(
    tx: &mut WriteTx,
    admin: &AdminAccess,
    logbook_id: &str,
    action: &str,
    reason: &str,
) -> Result<LogbookAccess, StoreError> {
    let admin = admin.revalidate(tx).await?;
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(StoreError::Invalid("a rescue needs a reason".into()));
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM logbooks WHERE id = ?)")
        .bind(logbook_id)
        .fetch_one(&mut **tx)
        .await?;
    if !exists {
        return Err(StoreError::NotFound("Logbook"));
    }
    let id = new_id();
    sqlx::query(
        "INSERT INTO rescue_operations (id, logbook_id, actor_id, action, reason, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(logbook_id)
    .bind(&admin.user_id)
    .bind(action)
    .bind(reason)
    .bind(now())
    .execute(&mut **tx)
    .await?;
    Ok(LogbookAccess {
        logbook_id: logbook_id.to_owned(),
        user_id: admin.user_id.clone(),
        callsign: admin.callsign.clone(),
        role: LogbookRole::Owner,
        authorization: Authorization::Rescue {
            id,
            scope: Arc::downgrade(&tx.scope),
        },
    })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::store::{
        account::user::{self, Role},
        logbook,
        logbook::{membership, station},
        testing::{World, station_input},
    };

    async fn add_token(world: &World, user_id: &str, scopes: &[&str]) -> Principal {
        let token_id = new_id();
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, kind, name, token_hash, created_at) VALUES (?, ?, 'native', 't', ?, ?)",
        )
        .bind(&token_id)
        .bind(user_id)
        .bind(token_id.as_bytes())
        .bind(now())
        .execute(&mut *tx)
        .await
        .unwrap();
        for scope in scopes {
            sqlx::query("INSERT INTO api_token_scopes (token_id, logbook_id) VALUES (?, ?)")
                .bind(&token_id)
                .bind(scope)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
        Principal::Token {
            user_id: user_id.to_owned(),
            token_id,
        }
    }

    #[tokio::test]
    async fn grants_members_their_role() {
        let world = World::new().await;
        let owner = world.access(&world.owner).await;
        assert_eq!((owner.role(), owner.callsign()), (LogbookRole::Owner, "JA1OWN"));
        assert_eq!(owner.logbook_id(), world.logbook.id);
        assert!(owner.owner().is_ok());

        let operator = world.access(&world.operator).await;
        assert_eq!(operator.role(), LogbookRole::Operator);
        assert!(matches!(operator.owner(), Err(StoreError::Forbidden(_))));
    }

    #[tokio::test]
    async fn hides_a_logbook_from_non_members_including_admins() {
        let world = World::new().await;
        let mut conn = world.db.acquire().await.unwrap();
        for user in [&world.outsider, &world.admin] {
            let denied = logbook(&mut conn, &Principal::user(&user.id), &world.logbook.id).await;
            assert!(matches!(denied, Err(StoreError::NotFound("Logbook"))), "{denied:?}");
        }
        let missing = logbook(&mut conn, &Principal::user(&world.owner.id), "no-such-logbook").await;
        assert!(matches!(missing, Err(StoreError::NotFound("Logbook"))));
    }

    #[tokio::test]
    async fn refuses_disabled_users() {
        let world = World::new().await;
        let mut conn = world.db.acquire().await.unwrap();
        sqlx::query("UPDATE users SET disabled_at = ? WHERE id = ?")
            .bind(now())
            .bind(&world.operator.id)
            .execute(&mut *conn)
            .await
            .unwrap();
        let denied = logbook(&mut conn, &Principal::user(&world.operator.id), &world.logbook.id).await;
        assert!(matches!(denied, Err(StoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn limits_tokens_to_their_scopes() {
        let world = World::new().await;
        let scoped = add_token(&world, &world.operator.id, &[&world.logbook.id]).await;
        let unscoped = add_token(&world, &world.operator.id, &[]).await;
        let mut conn = world.db.acquire().await.unwrap();
        assert!(logbook(&mut conn, &scoped, &world.logbook.id).await.is_ok());
        assert!(matches!(
            logbook(&mut conn, &unscoped, &world.logbook.id).await,
            Err(StoreError::NotFound(_))
        ));

        let Principal::Token { token_id, .. } = scoped else {
            unreachable!()
        };
        let borrowed = Principal::Token {
            user_id: world.owner.id.clone(),
            token_id,
        };
        assert!(matches!(
            logbook(&mut conn, &borrowed, &world.logbook.id).await,
            Err(StoreError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn gives_admin_access_to_signed_in_admins_only() {
        let world = World::new().await;
        let admin_token = add_token(&world, &world.admin.id, &[]).await;
        let mut conn = world.db.acquire().await.unwrap();
        assert!(admin(&mut conn, &Principal::user(&world.admin.id)).await.is_ok());
        assert!(matches!(
            admin(&mut conn, &Principal::user(&world.owner.id)).await,
            Err(StoreError::Forbidden(_))
        ));
        assert!(matches!(
            admin(&mut conn, &admin_token).await,
            Err(StoreError::Forbidden(_))
        ));
    }

    #[tokio::test]
    async fn records_a_rescue_with_its_reason() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let blank = rescue(&mut tx, &admin, &world.logbook.id, "export", "  ").await;
        assert!(matches!(blank, Err(StoreError::Invalid(_))));
        let access = rescue(&mut tx, &admin, &world.logbook.id, "export", "owner lost their passkey")
            .await
            .unwrap();
        assert_eq!(
            (access.role(), access.user_id()),
            (LogbookRole::Owner, world.admin.id.as_str())
        );
        let recorded: (String, String, String) =
            sqlx::query_as("SELECT actor_id, action, reason FROM rescue_operations WHERE logbook_id = ?")
                .bind(&world.logbook.id)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(
            recorded,
            (
                world.admin.id.clone(),
                "export".into(),
                "owner lost their passkey".into()
            )
        );
    }

    #[rstest]
    #[case::rollback(false)]
    #[case::commit(true)]
    #[tokio::test]
    async fn limits_rescue_access_to_its_transaction(#[case] commit: bool) {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let access = rescue(
            &mut tx,
            &admin,
            &world.logbook.id,
            "create station",
            "owner lost access",
        )
        .await
        .unwrap();
        let input = station_input("Rescue");
        let station = station::create(&mut tx, &access.owner().unwrap(), &input)
            .await
            .unwrap();
        assert_eq!(station::get(&mut tx, &access, &station.id).await.unwrap(), station);

        let mut other = world.db.acquire().await.unwrap();
        assert!(matches!(
            logbook::get(&mut other, &access).await,
            Err(StoreError::NotFound("Logbook"))
        ));
        drop(other);
        if commit {
            tx.commit().await.unwrap();
        } else {
            drop(tx);
        }

        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            station::create(&mut tx, &access.owner().unwrap(), &input).await,
            Err(StoreError::NotFound("Logbook"))
        ));
        assert!(matches!(
            logbook::get(&mut tx, &access).await,
            Err(StoreError::NotFound("Logbook"))
        ));
        let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM rescue_operations")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let stations: i64 = sqlx::query_scalar("SELECT count(*) FROM stations")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!((audits, stations), (i64::from(commit), i64::from(commit)));
        tx.commit().await.unwrap();
    }

    #[rstest]
    #[case::removed(true)]
    #[case::demoted(false)]
    #[tokio::test]
    async fn rechecks_owner_permissions(#[case] remove: bool) {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let owner = access.owner().unwrap();
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        membership::set(&mut tx, &owner, &world.operator.id, LogbookRole::Owner)
            .await
            .unwrap();
        if remove {
            membership::remove(&mut tx, &owner, &world.owner.id).await.unwrap();
        } else {
            membership::set(&mut tx, &owner, &world.owner.id, LogbookRole::Operator)
                .await
                .unwrap();
        }
        let denied = station::create(&mut tx, &owner, &station_input("Denied")).await;
        assert!(matches!(
            denied,
            Err(StoreError::NotFound(_) | StoreError::Forbidden(_))
        ));
        assert!(
            membership::set(&mut tx, &owner, &world.outsider.id, LogbookRole::Owner)
                .await
                .is_err()
        );
        assert!(membership::remove(&mut tx, &owner, &world.operator.id).await.is_err());
        tx.commit().await.unwrap();

        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(
            station::create(&mut tx, &owner, &station_input("Denied"))
                .await
                .is_err()
        );
        let stations: i64 = sqlx::query_scalar("SELECT count(*) FROM stations")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(stations, 0);
    }

    #[rstest]
    #[case::revoked("DELETE FROM api_tokens WHERE id = ?")]
    #[case::unscoped("DELETE FROM api_token_scopes WHERE token_id = ?")]
    #[tokio::test]
    async fn rechecks_token_access(#[case] revoke: &'static str) {
        let world = World::new().await;
        let principal = add_token(&world, &world.owner.id, &[&world.logbook.id]).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let access = logbook(&mut tx, &principal, &world.logbook.id).await.unwrap();
        let Principal::Token { token_id, .. } = principal else {
            unreachable!()
        };
        sqlx::query(sqlx::AssertSqlSafe(revoke))
            .bind(token_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            station::list(&mut tx, &access).await,
            Err(StoreError::NotFound("Logbook"))
        ));
        assert!(matches!(
            station::create(&mut tx, &access.owner().unwrap(), &station_input("Denied")).await,
            Err(StoreError::NotFound("Logbook"))
        ));
    }

    #[rstest]
    #[case::disabled("UPDATE users SET disabled_at = '2026-10-22T00:00:00Z' WHERE id = ?")]
    #[case::demoted("UPDATE users SET role = 'user' WHERE id = ?")]
    #[tokio::test]
    async fn rechecks_admin_access(#[case] revoke: &'static str) {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(revoke))
            .bind(&world.admin.id)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            user::create(&mut tx, &admin, "JA1NEW", Role::User).await,
            Err(StoreError::Forbidden(_))
        ));
        assert!(matches!(
            logbook::create(&mut tx, &admin, "Denied", &world.owner.id).await,
            Err(StoreError::Forbidden(_))
        ));
        assert!(matches!(
            rescue(&mut tx, &admin, &world.logbook.id, "export", "support").await,
            Err(StoreError::Forbidden(_))
        ));
    }
}
