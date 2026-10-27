pub(super) mod passkey;
pub(super) mod session;
pub(super) mod token;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Serialize;
use webauthn_rs::prelude::{DiscoverableAuthentication, PasskeyRegistration};

use crate::{
    store::{
        StoreError, WriteTx,
        access::{self, AdminAccess, Principal, UserAccess},
        account::{
            session::{self as sessions, SESSION_TTL},
            token::{self as tokens, TokenKind},
            user::{self as users, Role, User},
        },
        hash_token, random_token,
    },
    web::{AppState, error::ApiError, extract::Json},
};

pub const SESSION_COOKIE: &str = "__Host-session";
const CEREMONY_TTL: Duration = Duration::from_secs(300);
const MAX_CEREMONIES: usize = 10_000;

pub(super) enum Ceremony {
    Authentication(DiscoverableAuthentication),
    Registration {
        user_id: String,
        link: Option<String>,
        state: PasskeyRegistration,
    },
}

struct Pending {
    started: Instant,
    order: u64,
    ceremony: Ceremony,
}

#[derive(Default)]
struct Table {
    next_order: u64,
    pending: HashMap<String, Pending>,
}

#[derive(Default)]
pub struct Ceremonies(Mutex<Table>);

impl Ceremonies {
    pub(super) fn start(&self, ceremony: Ceremony) -> String {
        let mut table = self.0.lock().expect("ceremony lock poisoned");
        table.pending.retain(|_, p| p.started.elapsed() < CEREMONY_TTL);
        if table.pending.len() >= MAX_CEREMONIES
            && let Some(oldest) = table
                .pending
                .iter()
                .min_by_key(|(_, p)| p.order)
                .map(|(id, _)| id.clone())
        {
            table.pending.remove(&oldest);
        }
        let id = random_token();
        let order = table.next_order;
        table.next_order += 1;
        table.pending.insert(
            id.clone(),
            Pending {
                started: Instant::now(),
                order,
                ceremony,
            },
        );
        id
    }

    pub(super) fn finish(&self, id: &str) -> Result<Ceremony, ApiError> {
        let pending = self
            .0
            .lock()
            .expect("ceremony lock poisoned")
            .pending
            .remove(id)
            .ok_or_else(expired)?;
        if pending.started.elapsed() >= CEREMONY_TTL {
            return Err(expired());
        }
        Ok(pending.ceremony)
    }
}

pub struct SignedIn(pub UserAccess);

impl FromRequestParts<Arc<AppState>> for SignedIn {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<SignedIn, ApiError> {
        let principal = sessions::authenticate(&state.db, &session_token(&parts.headers)?).await?;
        let mut conn = state.db.acquire().await?;
        Ok(SignedIn(access::user(&mut conn, &principal).await?))
    }
}

pub struct Admin(pub AdminAccess);

impl FromRequestParts<Arc<AppState>> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Admin, ApiError> {
        let principal = sessions::authenticate(&state.db, &session_token(&parts.headers)?).await?;
        let mut conn = state.db.acquire().await?;
        Ok(Admin(access::admin(&mut conn, &principal).await?))
    }
}

pub struct Caller {
    pub principal: Principal,
    pub scopes: Option<Vec<String>>,
    pub session_hash: Option<Vec<u8>>,
}

impl FromRequestParts<Arc<AppState>> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Caller, ApiError> {
        let Some(authorization) = parts.headers.get(header::AUTHORIZATION) else {
            let token = session_token(&parts.headers)?;
            return Ok(Caller {
                principal: sessions::authenticate(&state.db, &token).await?,
                scopes: None,
                session_hash: Some(hash_token(&token)),
            });
        };
        let plaintext = bearer(authorization).ok_or(StoreError::Unauthenticated)?;
        let (principal, token) = tokens::authenticate(&state.db, plaintext, TokenKind::Native).await?;
        Ok(Caller {
            principal,
            scopes: Some(token.logbook_ids),
            session_hash: None,
        })
    }
}

pub(super) fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok())
}

fn session_token(headers: &HeaderMap) -> Result<String, StoreError> {
    CookieJar::from_headers(headers)
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value().to_owned())
        .ok_or(StoreError::Unauthenticated)
}

pub(in crate::web) fn bearer(authorization: &HeaderValue) -> Option<&str> {
    authorization
        .to_str()
        .ok()
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
        .map(|(_, token)| token)
}

#[derive(Debug, Serialize)]
pub struct Me {
    id: String,
    callsign: String,
    role: Role,
}

impl From<&UserAccess> for Me {
    fn from(user: &UserAccess) -> Me {
        Me {
            id: user.user_id().to_owned(),
            callsign: user.callsign().to_owned(),
            role: user.role(),
        }
    }
}

impl From<&User> for Me {
    fn from(user: &User) -> Me {
        Me {
            id: user.id.clone(),
            callsign: user.callsign.clone(),
            role: user.role,
        }
    }
}

pub async fn me(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    jar: CookieJar,
) -> Result<(CookieJar, Json<Me>), ApiError> {
    let mut conn = state.db.acquire().await?;
    let user = users::of_principal(&mut conn, &caller.principal).await?;
    let jar = match caller.session_hash {
        Some(_) => {
            let token = jar
                .get(SESSION_COOKIE)
                .map(|c| c.value().to_owned())
                .unwrap_or_default();
            jar.add(session_cookie(token))
        }
        None => jar,
    };
    Ok((jar, Json(Me::from(&user))))
}

pub async fn logout(State(state): State<Arc<AppState>>, jar: CookieJar) -> Result<(CookieJar, StatusCode), ApiError> {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let mut tx = WriteTx::begin(&state.db).await?;
        sessions::delete(&mut tx, cookie.value()).await?;
        tx.commit().await?;
    }
    Ok((jar.remove(session_cookie(String::new())), StatusCode::NO_CONTENT))
}

pub(super) fn session_cookie(token: String) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .secure(true)
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(cookie::time::Duration::seconds(SESSION_TTL.num_seconds()))
        .build()
}

pub(super) fn expired() -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, "the passkey request expired; try again")
        .with("code", "passkey_expired".into())
}

#[cfg(test)]
mod tests {
    use url::Url;
    use webauthn_rs::WebauthnBuilder;

    use super::*;
    use crate::web::testing::ORIGIN;

    #[test]
    fn ceremonies_are_used_once_and_evict_the_oldest_when_full() {
        let ceremonies = Ceremonies::default();
        let webauthn = WebauthnBuilder::new("log.example.com", &Url::parse(ORIGIN).unwrap())
            .unwrap()
            .build()
            .unwrap();
        let start = || Ceremony::Authentication(webauthn.start_discoverable_authentication().unwrap().1);
        let pending = || ceremonies.0.lock().unwrap().pending.len();
        let id = ceremonies.start(start());
        assert!(matches!(ceremonies.finish(&id), Ok(Ceremony::Authentication(_))));
        assert!(ceremonies.finish(&id).is_err());

        let oldest = ceremonies.start(start());
        let second = ceremonies.start(start());
        for _ in 2..MAX_CEREMONIES {
            ceremonies.start(start());
        }
        assert_eq!(pending(), MAX_CEREMONIES);
        let newest = ceremonies.start(start());
        assert_eq!(pending(), MAX_CEREMONIES);
        assert!(ceremonies.finish(&oldest).is_err());
        assert!(ceremonies.finish(&second).is_ok());
        assert!(ceremonies.finish(&newest).is_ok());
    }
}
