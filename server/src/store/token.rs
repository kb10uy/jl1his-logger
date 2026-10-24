use std::collections::BTreeSet;

use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    db::Db,
    store::{
        Fault, StoreError, WriteTx,
        access::{self, Principal, UserAccess},
        hash_token, name as store_name, new_id, now, random_token, timestamp,
    },
};

const WL2_PREFIX: &str = "wl2_";
const TOUCH_INTERVAL: TimeDelta = TimeDelta::days(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Native,
    Wl2,
}

impl TokenKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TokenKind::Native => "native",
            TokenKind::Wl2 => "wl2",
        }
    }

    fn parse(s: &str) -> Result<TokenKind, StoreError> {
        match s {
            "native" => Ok(TokenKind::Native),
            "wl2" => Ok(TokenKind::Wl2),
            _ => Err(StoreError::Invalid(format!("unknown token kind {s:?}").into())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInput {
    pub kind: TokenKind,
    pub name: String,
    pub logbook_ids: Vec<String>,
    pub wl2_prefix: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiToken {
    pub id: String,
    pub user_id: String,
    pub kind: TokenKind,
    pub name: String,
    pub logbook_ids: Vec<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

impl ApiToken {
    fn from_row(row: &SqliteRow) -> Result<ApiToken, StoreError> {
        let scopes: String = row.try_get("scopes")?;
        Ok(ApiToken {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            kind: TokenKind::parse(row.try_get("kind")?)?,
            name: row.try_get("name")?,
            logbook_ids: serde_json::from_str(&scopes)
                .map_err(|e| StoreError::Invalid(format!("token scopes are not a list: {e}").into()))?,
            created_at: row.try_get("created_at")?,
            last_used_at: row.try_get("last_used_at")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issued {
    pub token: ApiToken,
    pub plaintext: String,
}

const SELECT: &str = "SELECT t.id, t.user_id, t.kind, t.name, t.created_at, t.last_used_at, \
     (SELECT json_group_array(logbook_id) FROM \
      (SELECT logbook_id FROM api_token_scopes WHERE token_id = t.id ORDER BY logbook_id)) AS scopes \
     FROM api_tokens t";

pub async fn issue(tx: &mut WriteTx, user: &UserAccess, input: &TokenInput) -> Result<Issued, StoreError> {
    let user = user.revalidate(tx).await?;
    let name = store_name("a token", &input.name)?;
    let scopes: BTreeSet<&str> = input.logbook_ids.iter().map(String::as_str).collect();
    match (input.kind, scopes.len()) {
        (_, 0) => {
            return Err(StoreError::Invalid(Fault::coded(
                "token_needs_logbook",
                "a token needs a Logbook",
            )));
        }
        (TokenKind::Wl2, 2..) => {
            return Err(StoreError::Invalid(Fault::coded(
                "wl2_token_one_logbook",
                "a wl2 token is fixed to one Logbook",
            )));
        }
        _ => {}
    }
    if input.wl2_prefix && input.kind != TokenKind::Wl2 {
        return Err(StoreError::Invalid(Fault::coded(
            "wl2_prefix_only",
            "only wl2 tokens take the wl2_ prefix",
        )));
    }
    let principal = Principal::user(user.user_id());
    for logbook_id in &scopes {
        access::logbook(tx, &principal, logbook_id).await?;
    }

    let prefix = if input.wl2_prefix { WL2_PREFIX } else { "" };
    let plaintext = format!("{prefix}{}", random_token());
    let id = new_id();
    sqlx::query("INSERT INTO api_tokens (id, user_id, kind, name, token_hash, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(user.user_id())
        .bind(input.kind.as_str())
        .bind(name)
        .bind(hash_token(&plaintext))
        .bind(now())
        .execute(&mut **tx)
        .await?;
    for logbook_id in &scopes {
        sqlx::query("INSERT INTO api_token_scopes (token_id, logbook_id) VALUES (?, ?)")
            .bind(&id)
            .bind(logbook_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(Issued {
        token: find(tx, &id).await?.ok_or(StoreError::NotFound("API token"))?,
        plaintext,
    })
}

pub async fn list(conn: &mut SqliteConnection, user: &UserAccess, user_id: &str) -> Result<Vec<ApiToken>, StoreError> {
    let user = user.revalidate(conn).await?;
    if !user.may_manage(user_id) {
        return Err(StoreError::NotFound("user"));
    }
    sqlx::query(AssertSqlSafe(format!(
        "{SELECT} WHERE t.user_id = ? ORDER BY t.created_at, t.id"
    )))
    .bind(user_id)
    .fetch_all(conn)
    .await?
    .iter()
    .map(ApiToken::from_row)
    .collect()
}

pub async fn revoke(tx: &mut WriteTx, user: &UserAccess, token_id: &str) -> Result<(), StoreError> {
    let user = user.revalidate(tx).await?;
    match find(tx, token_id).await? {
        Some(token) if user.may_manage(&token.user_id) => {}
        _ => return Err(StoreError::NotFound("API token")),
    }
    sqlx::query("DELETE FROM api_tokens WHERE id = ?")
        .bind(token_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn authenticate(db: &Db, plaintext: &str, kind: TokenKind) -> Result<(Principal, ApiToken), StoreError> {
    let mut token = {
        let mut conn = db.acquire().await?;
        let row = sqlx::query(AssertSqlSafe(format!(
            "{SELECT} JOIN users u ON u.id = t.user_id \
             WHERE t.token_hash = ? AND t.kind = ? AND u.disabled_at IS NULL"
        )))
        .bind(hash_token(plaintext.trim()))
        .bind(kind.as_str())
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(StoreError::Unauthenticated)?;
        ApiToken::from_row(&row)?
    };

    let now = Utc::now();
    let stale = timestamp(now - TOUCH_INTERVAL);
    if token.last_used_at.as_ref().is_none_or(|used| *used < stale) {
        let used = timestamp(now);
        let mut tx = WriteTx::begin(db).await?;
        sqlx::query(
            "UPDATE api_tokens SET last_used_at = ? WHERE id = ? AND (last_used_at IS NULL OR last_used_at < ?)",
        )
        .bind(&used)
        .bind(&token.id)
        .bind(&stale)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        token.last_used_at = Some(used);
    }

    let principal = Principal::Token {
        user_id: token.user_id.clone(),
        token_id: token.id.clone(),
    };
    Ok((principal, token))
}

async fn find(conn: &mut SqliteConnection, id: &str) -> Result<Option<ApiToken>, StoreError> {
    sqlx::query(AssertSqlSafe(format!("{SELECT} WHERE t.id = ?")))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .map(|row| ApiToken::from_row(&row))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{logbook, testing::World};

    fn input(kind: TokenKind, logbook_ids: &[&str]) -> TokenInput {
        TokenInput {
            kind,
            name: " hislog ".into(),
            logbook_ids: logbook_ids.iter().map(|id| id.to_string()).collect(),
            wl2_prefix: false,
        }
    }

    async fn issue_one(world: &World, input: &TokenInput) -> Issued {
        let user = world.user_access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let issued = issue(&mut tx, &user, input).await.unwrap();
        tx.commit().await.unwrap();
        issued
    }

    #[tokio::test]
    async fn issues_a_token_that_authenticates_as_its_user() {
        let world = World::new().await;
        let issued = issue_one(&world, &input(TokenKind::Native, &[&world.logbook.id])).await;
        assert_eq!(issued.token.name, "hislog");
        assert_eq!(issued.token.logbook_ids, std::slice::from_ref(&world.logbook.id));
        assert_eq!(issued.plaintext.len(), 64);

        let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM api_tokens WHERE id = ?")
            .bind(&issued.token.id)
            .fetch_one(&world.db)
            .await
            .unwrap();
        assert_ne!(stored, issued.plaintext.as_bytes());

        let (principal, token) = authenticate(&world.db, &issued.plaintext, TokenKind::Native)
            .await
            .unwrap();
        assert!(token.last_used_at.is_some());
        let mut conn = world.db.acquire().await.unwrap();
        let access = access::logbook(&mut conn, &principal, &world.logbook.id).await.unwrap();
        assert_eq!(access.user_id(), world.operator.id);
    }

    #[tokio::test]
    async fn opens_only_the_api_of_its_kind() {
        let world = World::new().await;
        let mut wl2 = input(TokenKind::Wl2, &[&world.logbook.id]);
        wl2.wl2_prefix = true;
        let issued = issue_one(&world, &wl2).await;
        assert!(issued.plaintext.starts_with("wl2_"));
        assert!(authenticate(&world.db, &issued.plaintext, TokenKind::Wl2).await.is_ok());
        assert!(matches!(
            authenticate(&world.db, &issued.plaintext, TokenKind::Native).await,
            Err(StoreError::Unauthenticated)
        ));
        assert!(matches!(
            authenticate(&world.db, "wl2_unknown", TokenKind::Wl2).await,
            Err(StoreError::Unauthenticated)
        ));
    }

    #[tokio::test]
    async fn refuses_scopes_outside_the_users_memberships() {
        let world = World::new().await;
        let other = {
            let admin = world.admin_access().await;
            let mut tx = WriteTx::begin(&world.db).await.unwrap();
            let other = logbook::create(&mut tx, &admin, "Other", &world.owner.id)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            other
        };
        let user = world.user_access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let refused = [
            input(TokenKind::Native, &[&world.logbook.id, &other.id]),
            input(TokenKind::Native, &[]),
            input(TokenKind::Wl2, &[&world.logbook.id, &other.id]),
            TokenInput {
                wl2_prefix: true,
                ..input(TokenKind::Native, &[&world.logbook.id])
            },
            TokenInput {
                name: " ".into(),
                ..input(TokenKind::Native, &[&world.logbook.id])
            },
        ];
        for input in &refused {
            let result = issue(&mut tx, &user, input).await;
            assert!(
                matches!(result, Err(StoreError::NotFound(_) | StoreError::Invalid(_))),
                "{input:?}: {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn lets_the_user_or_an_admin_list_and_revoke() {
        let world = World::new().await;
        let issued = issue_one(&world, &input(TokenKind::Native, &[&world.logbook.id])).await;
        let owner = world.user_access(&world.owner).await;
        let admin = world.user_access(&world.admin).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();

        assert!(matches!(
            list(&mut tx, &owner, &world.operator.id).await,
            Err(StoreError::NotFound(_))
        ));
        assert!(matches!(
            revoke(&mut tx, &owner, &issued.token.id).await,
            Err(StoreError::NotFound(_))
        ));
        assert_eq!(
            list(&mut tx, &admin, &world.operator.id).await.unwrap(),
            std::slice::from_ref(&issued.token)
        );
        revoke(&mut tx, &admin, &issued.token.id).await.unwrap();
        assert!(list(&mut tx, &admin, &world.operator.id).await.unwrap().is_empty());
        tx.commit().await.unwrap();

        assert!(matches!(
            authenticate(&world.db, &issued.plaintext, TokenKind::Native).await,
            Err(StoreError::Unauthenticated)
        ));
    }

    #[tokio::test]
    async fn touches_last_used_at_once_a_day() {
        let world = World::new().await;
        let issued = issue_one(&world, &input(TokenKind::Native, &[&world.logbook.id])).await;
        let old = timestamp(Utc::now() - TimeDelta::hours(1));
        sqlx::query("UPDATE api_tokens SET last_used_at = ?")
            .bind(&old)
            .execute(&world.db)
            .await
            .unwrap();
        let (_, token) = authenticate(&world.db, &issued.plaintext, TokenKind::Native)
            .await
            .unwrap();
        assert_eq!(token.last_used_at, Some(old));

        let older = timestamp(Utc::now() - TimeDelta::hours(25));
        sqlx::query("UPDATE api_tokens SET last_used_at = ?")
            .bind(&older)
            .execute(&world.db)
            .await
            .unwrap();
        authenticate(&world.db, &issued.plaintext, TokenKind::Native)
            .await
            .unwrap();
        let stored: String = sqlx::query_scalar("SELECT last_used_at FROM api_tokens")
            .fetch_one(&world.db)
            .await
            .unwrap();
        assert!(stored > older);
    }
}
