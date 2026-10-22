use std::ops::Deref;

use sqlx::{Row, SqliteConnection};

use crate::store::{StoreError, WriteTx, new_id, now};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            _ => Err(StoreError::Invalid(format!("unknown Logbook role {s:?}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogbookAccess {
    logbook_id: String,
    user_id: String,
    callsign: String,
    role: LogbookRole,
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
}

#[derive(Debug, Clone, Copy)]
pub struct OwnerAccess<'a>(&'a LogbookAccess);

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

pub async fn rescue(
    tx: &mut WriteTx,
    admin: &AdminAccess,
    logbook_id: &str,
    action: &str,
    reason: &str,
) -> Result<LogbookAccess, StoreError> {
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
    sqlx::query(
        "INSERT INTO rescue_operations (id, logbook_id, actor_id, action, reason, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testing::World;

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
}
