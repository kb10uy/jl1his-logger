use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::{FromRequestParts, Path, State},
    http::{StatusCode, request::Parts},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;
use webauthn_rs::prelude::{
    CreationChallengeResponse, DiscoverableAuthentication, Passkey as Credential, PasskeyRegistration,
    PublicKeyCredential, RegisterPublicKeyCredential, WebauthnError,
};

use crate::{
    store::{
        StoreError, WriteTx,
        access::{self, Principal, UserAccess},
        passkey::{self, NewPasskey, Passkey},
        random_token, registration,
        session::{self, SESSION_TTL},
        user::User,
    },
    web::{AppState, error::ApiError},
};

pub const SESSION_COOKIE: &str = "__Host-session";
const CEREMONY_TTL: Duration = Duration::from_secs(300);
const MAX_CEREMONIES: usize = 1000;
const DEFAULT_PASSKEY_NAME: &str = "Passkey";

enum Ceremony {
    Authentication(DiscoverableAuthentication),
    Registration {
        user_id: String,
        link: Option<String>,
        state: PasskeyRegistration,
    },
}

#[derive(Default)]
pub struct Ceremonies(Mutex<HashMap<String, (Instant, Ceremony)>>);

impl Ceremonies {
    fn start(&self, ceremony: Ceremony) -> Result<String, ApiError> {
        let mut map = self.0.lock().expect("ceremony lock poisoned");
        map.retain(|_, (started, _)| started.elapsed() < CEREMONY_TTL);
        if map.len() >= MAX_CEREMONIES {
            return Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "too many passkey requests in flight; try again later",
            ));
        }
        let id = random_token();
        map.insert(id.clone(), (Instant::now(), ceremony));
        Ok(id)
    }

    fn finish(&self, id: &str) -> Result<Ceremony, ApiError> {
        let (started, ceremony) = self
            .0
            .lock()
            .expect("ceremony lock poisoned")
            .remove(id)
            .ok_or_else(expired)?;
        if started.elapsed() >= CEREMONY_TTL {
            return Err(expired());
        }
        Ok(ceremony)
    }
}

pub struct SignedIn(pub UserAccess);

impl FromRequestParts<Arc<AppState>> for SignedIn {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<SignedIn, ApiError> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar.get(SESSION_COOKIE).ok_or(StoreError::Unauthenticated)?.value();
        let principal = session::authenticate(&state.db, token).await?;
        let mut conn = state.db.acquire().await?;
        Ok(SignedIn(access::user(&mut conn, &principal).await?))
    }
}

#[derive(Debug, Serialize)]
pub struct Me {
    id: String,
    callsign: String,
    role: &'static str,
}

impl From<&UserAccess> for Me {
    fn from(user: &UserAccess) -> Me {
        Me {
            id: user.user_id().to_owned(),
            callsign: user.callsign().to_owned(),
            role: user.role().as_str(),
        }
    }
}

impl From<&User> for Me {
    fn from(user: &User) -> Me {
        Me {
            id: user.id.clone(),
            callsign: user.callsign.clone(),
            role: user.role.as_str(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PasskeyInfo {
    id: String,
    name: String,
    created_at: String,
    last_used_at: Option<String>,
}

impl From<Passkey> for PasskeyInfo {
    fn from(passkey: Passkey) -> PasskeyInfo {
        PasskeyInfo {
            id: passkey.id,
            name: passkey.name,
            created_at: passkey.created_at,
            last_used_at: passkey.last_used_at,
        }
    }
}

#[derive(Deserialize)]
pub struct Finish<C> {
    ceremony: String,
    credential: C,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
pub struct LinkStart {
    token: String,
}

pub async fn me(SignedIn(user): SignedIn, jar: CookieJar) -> (CookieJar, Json<Me>) {
    let token = jar
        .get(SESSION_COOKIE)
        .map(|c| c.value().to_owned())
        .unwrap_or_default();
    (jar.add(session_cookie(token)), Json(Me::from(&user)))
}

pub async fn logout(State(state): State<Arc<AppState>>, jar: CookieJar) -> Result<(CookieJar, StatusCode), ApiError> {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let mut tx = WriteTx::begin(&state.db).await?;
        session::delete(&mut tx, cookie.value()).await?;
        tx.commit().await?;
    }
    Ok((jar.remove(session_cookie(String::new())), StatusCode::NO_CONTENT))
}

pub async fn login_start(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    let (challenge, auth) = state
        .webauthn
        .start_discoverable_authentication()
        .map_err(webauthn_failed)?;
    let ceremony = state.ceremonies.start(Ceremony::Authentication(auth))?;
    let mut options = serde_json::to_value(challenge).map_err(|e| ApiError::internal(e.to_string()))?;
    if let Some(options) = options.as_object_mut() {
        options.remove("mediation");
    }
    Ok(Json(json!({ "ceremony": ceremony, "options": options })))
}

pub async fn login_finish(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(req): Json<Finish<PublicKeyCredential>>,
) -> Result<(CookieJar, Json<Me>), ApiError> {
    let Ceremony::Authentication(auth) = state.ceremonies.finish(&req.ceremony)? else {
        return Err(expired());
    };
    let (user_handle, credential_id) = state
        .webauthn
        .identify_discoverable_authentication(&req.credential)
        .map_err(|_| unknown_passkey())?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let stored = match passkey::find_by_credential(&mut tx, credential_id).await {
        Ok(stored) => stored,
        Err(StoreError::Unauthenticated) => return Err(unknown_passkey()),
        Err(e) => return Err(e.into()),
    };
    if Uuid::parse_str(&stored.user_id).ok() != Some(user_handle) {
        return Err(unknown_passkey());
    }
    let mut credential = decode(&stored.passkey)?;
    let result = state
        .webauthn
        .finish_discoverable_authentication(&req.credential, auth, &[(&credential).into()])
        .map_err(|e| {
            tracing::info!("passkey sign-in failed: {e}");
            ApiError::new(StatusCode::UNAUTHORIZED, format!("passkey sign-in failed: {e}"))
        })?;
    credential.update_credential(&result);

    passkey::record_use(&mut tx, &stored.id, &encode(&credential)?).await?;
    let token = session::create(&mut tx, &stored.user_id).await?;
    let user = access::user(&mut tx, &Principal::user(&stored.user_id)).await?;
    tx.commit().await?;
    tracing::info!("{} signed in", user.callsign());
    Ok((jar.add(session_cookie(token)), Json(Me::from(&user))))
}

pub async fn link_start(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LinkStart>,
) -> Result<Json<Value>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let found = registration::check(&mut conn, &req.token).await.map_err(link_failed)?;
    drop(conn);
    let (challenge, registration) =
        start_registration(&state, &found.user.id, &found.user.callsign, found.credential_ids)?;
    let ceremony = state.ceremonies.start(Ceremony::Registration {
        user_id: found.user.id,
        link: Some(req.token),
        state: registration,
    })?;
    Ok(Json(json!({
        "ceremony": ceremony,
        "options": require_resident_key(challenge)?,
        "callsign": found.user.callsign,
    })))
}

pub async fn link_finish(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(req): Json<Finish<RegisterPublicKeyCredential>>,
) -> Result<(CookieJar, Json<Me>), ApiError> {
    let Ceremony::Registration {
        user_id,
        link: Some(link),
        state: registration,
    } = state.ceremonies.finish(&req.ceremony)?
    else {
        return Err(expired());
    };
    let new = finish_registration(&state, &req, &registration)?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let (user, _) = registration::complete(&mut tx, &link, &new)
        .await
        .map_err(link_failed)?;
    if user.id != user_id {
        return Err(expired());
    }
    let token = session::create(&mut tx, &user.id).await?;
    tx.commit().await?;
    tracing::info!("{} registered a passkey through a link", user.callsign);
    Ok((jar.add(session_cookie(token)), Json(Me::from(&user))))
}

pub async fn list_passkeys(
    State(state): State<Arc<AppState>>,
    SignedIn(user): SignedIn,
) -> Result<Json<Vec<PasskeyInfo>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let passkeys = passkey::list(&mut conn, &user, user.user_id()).await?;
    Ok(Json(passkeys.into_iter().map(PasskeyInfo::from).collect()))
}

pub async fn add_start(State(state): State<Arc<AppState>>, SignedIn(user): SignedIn) -> Result<Json<Value>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let exclude = passkey::list(&mut conn, &user, user.user_id())
        .await?
        .into_iter()
        .map(|p| p.credential_id)
        .collect();
    drop(conn);
    let (challenge, registration) = start_registration(&state, user.user_id(), user.callsign(), exclude)?;
    let ceremony = state.ceremonies.start(Ceremony::Registration {
        user_id: user.user_id().to_owned(),
        link: None,
        state: registration,
    })?;
    Ok(Json(
        json!({ "ceremony": ceremony, "options": require_resident_key(challenge)? }),
    ))
}

pub async fn add_finish(
    State(state): State<Arc<AppState>>,
    SignedIn(user): SignedIn,
    Json(req): Json<Finish<RegisterPublicKeyCredential>>,
) -> Result<(StatusCode, Json<PasskeyInfo>), ApiError> {
    let Ceremony::Registration {
        user_id,
        link: None,
        state: registration,
    } = state.ceremonies.finish(&req.ceremony)?
    else {
        return Err(expired());
    };
    if user_id != user.user_id() {
        return Err(expired());
    }
    let new = finish_registration(&state, &req, &registration)?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let added = passkey::add(&mut tx, &user, &new).await?;
    tx.commit().await?;
    tracing::info!("{} added a passkey", user.callsign());
    Ok((StatusCode::CREATED, Json(added.into())))
}

pub async fn delete_passkey(
    State(state): State<Arc<AppState>>,
    SignedIn(user): SignedIn,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    passkey::delete(&mut tx, &user, &id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

fn start_registration(
    state: &AppState,
    user_id: &str,
    callsign: &str,
    exclude: Vec<Vec<u8>>,
) -> Result<(CreationChallengeResponse, PasskeyRegistration), ApiError> {
    let handle = Uuid::parse_str(user_id).map_err(|e| ApiError::internal(format!("user ID {user_id:?}: {e}")))?;
    state
        .webauthn
        .start_passkey_registration(handle, callsign, callsign, Some(exclude))
        .map_err(webauthn_failed)
}

fn finish_registration(
    state: &AppState,
    req: &Finish<RegisterPublicKeyCredential>,
    registration: &PasskeyRegistration,
) -> Result<NewPasskey, ApiError> {
    let credential = state
        .webauthn
        .finish_passkey_registration(&req.credential, registration)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("passkey registration failed: {e}")))?;
    let name = match req.name.trim() {
        "" => DEFAULT_PASSKEY_NAME,
        name => name,
    };
    Ok(NewPasskey {
        credential_id: credential.cred_id().to_vec(),
        passkey: encode(&credential)?,
        name: name.to_owned(),
    })
}

fn require_resident_key(challenge: CreationChallengeResponse) -> Result<Value, ApiError> {
    let mut options = serde_json::to_value(challenge).map_err(|e| ApiError::internal(e.to_string()))?;
    let selection = options
        .pointer_mut("/publicKey/authenticatorSelection")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| ApiError::internal("registration options without authenticatorSelection"))?;
    selection.insert("residentKey".into(), "required".into());
    selection.insert("requireResidentKey".into(), true.into());
    Ok(options)
}

fn session_cookie(token: String) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .secure(true)
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(cookie::time::Duration::seconds(SESSION_TTL.num_seconds()))
        .build()
}

fn decode(json: &str) -> Result<Credential, ApiError> {
    serde_json::from_str(json).map_err(|e| ApiError::internal(format!("stored passkey: {e}")))
}

fn encode(credential: &Credential) -> Result<String, ApiError> {
    serde_json::to_string(credential).map_err(|e| ApiError::internal(format!("passkey: {e}")))
}

fn expired() -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, "the passkey request expired; try again")
}

fn unknown_passkey() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "this passkey is not registered here, or its user is disabled",
    )
}

fn link_failed(e: StoreError) -> ApiError {
    match e {
        StoreError::Unauthenticated => ApiError::new(
            StatusCode::UNAUTHORIZED,
            "the registration link is invalid, used or expired; ask an admin for a new one",
        ),
        e => e.into(),
    }
}

fn webauthn_failed(e: WebauthnError) -> ApiError {
    ApiError::internal(format!("WebAuthn: {e}"))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, future::poll_fn, pin::pin, task::Poll};

    use axum::{Router, body::Body, http::header, response::IntoResponse};
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use http_body_util::BodyExt;
    use p256::ecdsa::{Signature, SigningKey, signature::Signer};
    use serde_cbor_2::Value as Cbor;
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;
    use url::Url;
    use webauthn_rs::WebauthnBuilder;

    use super::*;
    use crate::{
        config::Config,
        store::{
            registration::{self, DEFAULT_TTL},
            testing::World,
            user,
        },
        web::router,
    };

    const ORIGIN: &str = "https://log.example.com";

    struct Authenticator {
        key: SigningKey,
        credential_id: Vec<u8>,
        counter: u32,
    }

    impl Authenticator {
        fn new(seed: u8) -> Authenticator {
            Authenticator {
                key: SigningKey::from_slice(&[seed; 32]).unwrap(),
                credential_id: vec![seed; 16],
                counter: 0,
            }
        }

        fn create(&self, options: &Value) -> Value {
            let client_data = client_data("webauthn.create", options);
            let point = self.key.verifying_key().to_encoded_point(false);
            let public_key = Cbor::Map(BTreeMap::from([
                (Cbor::Integer(1), Cbor::Integer(2)),
                (Cbor::Integer(3), Cbor::Integer(-7)),
                (Cbor::Integer(-1), Cbor::Integer(1)),
                (Cbor::Integer(-2), Cbor::Bytes(point.x().unwrap().to_vec())),
                (Cbor::Integer(-3), Cbor::Bytes(point.y().unwrap().to_vec())),
            ]));
            let mut auth_data = self.auth_data(0x45);
            auth_data.extend([0; 16]);
            auth_data.extend((self.credential_id.len() as u16).to_be_bytes());
            auth_data.extend(&self.credential_id);
            auth_data.extend(serde_cbor_2::to_vec(&public_key).unwrap());
            let attestation = Cbor::Map(BTreeMap::from([
                (Cbor::Text("fmt".into()), Cbor::Text("none".into())),
                (Cbor::Text("attStmt".into()), Cbor::Map(BTreeMap::new())),
                (Cbor::Text("authData".into()), Cbor::Bytes(auth_data)),
            ]));
            json!({
                "id": b64(&self.credential_id),
                "rawId": b64(&self.credential_id),
                "type": "public-key",
                "response": {
                    "attestationObject": b64(&serde_cbor_2::to_vec(&attestation).unwrap()),
                    "clientDataJSON": b64(client_data.as_bytes()),
                },
                "extensions": {},
            })
        }

        fn get(&mut self, options: &Value, user_handle: &str) -> Value {
            self.counter += 1;
            let client_data = client_data("webauthn.get", options);
            let auth_data = self.auth_data(0x05);
            let mut signed = auth_data.clone();
            signed.extend(Sha256::digest(client_data.as_bytes()));
            let signature: Signature = self.key.sign(&signed);
            json!({
                "id": b64(&self.credential_id),
                "rawId": b64(&self.credential_id),
                "type": "public-key",
                "response": {
                    "authenticatorData": b64(&auth_data),
                    "clientDataJSON": b64(client_data.as_bytes()),
                    "signature": b64(&signature.to_der().to_bytes()),
                    "userHandle": user_handle,
                },
                "extensions": {},
            })
        }

        fn auth_data(&self, flags: u8) -> Vec<u8> {
            let mut data = Sha256::digest(b"log.example.com").to_vec();
            data.push(flags);
            data.extend(self.counter.to_be_bytes());
            data
        }
    }

    fn b64(bytes: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(bytes)
    }

    fn client_data(kind: &str, options: &Value) -> String {
        json!({
            "type": kind,
            "challenge": options["publicKey"]["challenge"],
            "origin": ORIGIN,
            "crossOrigin": false,
        })
        .to_string()
    }

    struct Harness {
        world: World,
        app: Router,
    }

    struct Reply {
        status: StatusCode,
        cookie: Option<String>,
        body: Value,
    }

    impl Harness {
        async fn new() -> Harness {
            let world = World::new().await;
            let state = AppState::new(world.db.clone(), Config::default(), Url::parse(ORIGIN).unwrap()).unwrap();
            Harness {
                app: router(Arc::new(state), None),
                world,
            }
        }

        async fn link(&self, user_id: &str) -> String {
            let admin = self.world.admin_access().await;
            let mut tx = WriteTx::begin(&self.world.db).await.unwrap();
            let link = registration::issue(&mut tx, &admin, user_id, DEFAULT_TTL)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            link
        }

        async fn send(&self, method: &str, uri: &str, cookie: Option<&str>, body: Option<Value>) -> Reply {
            let mut req = axum::http::Request::builder()
                .method(method)
                .uri(uri)
                .header(header::ORIGIN, ORIGIN);
            if let Some(cookie) = cookie {
                req = req.header(header::COOKIE, cookie);
            }
            let body = match body {
                Some(body) => {
                    req = req.header(header::CONTENT_TYPE, "application/json");
                    Body::from(body.to_string())
                }
                None => Body::empty(),
            };
            let response = self.app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
            let status = response.status();
            let cookie = response
                .headers()
                .get(header::SET_COOKIE)
                .map(|v| v.to_str().unwrap().to_owned());
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let body = match bytes.is_empty() {
                true => Value::Null,
                false => serde_json::from_slice(&bytes).unwrap(),
            };
            Reply { status, cookie, body }
        }

        async fn register(&self, link: &str, authenticator: &Authenticator) -> Reply {
            let start = self
                .send("POST", "/api/auth/register/start", None, Some(json!({ "token": link })))
                .await;
            assert_eq!(start.status, StatusCode::OK, "{}", start.body);
            assert_eq!(
                start.body["callsign"],
                start.body["options"]["publicKey"]["user"]["name"]
            );
            let credential = authenticator.create(&start.body["options"]);
            self.send(
                "POST",
                "/api/auth/register/finish",
                None,
                Some(json!({ "ceremony": start.body["ceremony"], "credential": credential, "name": " Phone " })),
            )
            .await
        }

        async fn sign_in(&self, authenticator: &mut Authenticator, user_handle: &str) -> Reply {
            let start = self.send("POST", "/api/auth/login/start", None, None).await;
            assert_eq!(start.status, StatusCode::OK, "{}", start.body);
            assert!(start.body["options"].get("mediation").is_none());
            let credential = authenticator.get(&start.body["options"], user_handle);
            self.send(
                "POST",
                "/api/auth/login/finish",
                None,
                Some(json!({ "ceremony": start.body["ceremony"], "credential": credential })),
            )
            .await
        }
    }

    fn session(reply: &Reply) -> String {
        let cookie = reply.cookie.as_deref().expect("a session cookie");
        for attribute in ["Secure", "HttpOnly", "SameSite=Lax", "Path=/", "Max-Age=2592000"] {
            assert!(cookie.contains(attribute), "{cookie}");
        }
        cookie.split(';').next().unwrap().to_owned()
    }

    fn handle(user_id: &str) -> String {
        b64(Uuid::parse_str(user_id).unwrap().as_bytes())
    }

    #[tokio::test]
    async fn registers_through_a_link_and_signs_in_again() {
        let h = Harness::new().await;
        let operator = &h.world.operator;
        let link = h.link(&operator.id).await;
        let mut authenticator = Authenticator::new(1);

        let registered = h.register(&link, &authenticator).await;
        assert_eq!(registered.status, StatusCode::OK, "{}", registered.body);
        assert_eq!(
            registered.body,
            json!({ "id": operator.id, "callsign": "JA1OPR", "role": "user" })
        );
        let cookie = session(&registered);
        assert!(cookie.starts_with("__Host-session="));
        let me = h.send("GET", "/api/me", Some(&cookie), None).await;
        assert_eq!((me.status, &me.body["callsign"]), (StatusCode::OK, &json!("JA1OPR")));
        session(&me);

        let again = h
            .send("POST", "/api/auth/register/start", None, Some(json!({ "token": link })))
            .await;
        assert_eq!(again.status, StatusCode::UNAUTHORIZED, "{}", again.body);

        let out = h.send("POST", "/api/auth/logout", Some(&cookie), None).await;
        assert_eq!(out.status, StatusCode::NO_CONTENT);
        assert!(out.cookie.unwrap().contains("Max-Age=0"));
        let me = h.send("GET", "/api/me", Some(&cookie), None).await;
        assert_eq!(me.status, StatusCode::UNAUTHORIZED);

        let signed_in = h.sign_in(&mut authenticator, &handle(&operator.id)).await;
        assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.body);
        assert_eq!(signed_in.body["callsign"], "JA1OPR");
        let me = h.send("GET", "/api/me", Some(&session(&signed_in)), None).await;
        assert_eq!(me.status, StatusCode::OK);

        let mut conn = h.world.db.acquire().await.unwrap();
        let stored = passkey::find_by_credential(&mut conn, &[1; 16]).await.unwrap();
        assert_eq!(stored.name, "Phone");
        assert!(stored.last_used_at.is_some());
        assert_eq!(decode(&stored.passkey).unwrap().cred_id().as_slice(), [1; 16]);
        assert!(stored.passkey.contains(r#""counter":1"#), "{}", stored.passkey);
    }

    #[tokio::test]
    async fn a_link_registers_once_even_when_started_twice() {
        let h = Harness::new().await;
        let link = h.link(&h.world.operator.id).await;
        let mut finishes = Vec::new();
        for seed in [1, 2] {
            let start = h
                .send("POST", "/api/auth/register/start", None, Some(json!({ "token": link })))
                .await;
            let credential = Authenticator::new(seed).create(&start.body["options"]);
            finishes.push(json!({ "ceremony": start.body["ceremony"], "credential": credential }));
        }
        let mut statuses = Vec::new();
        for finish in finishes {
            let reply = h.send("POST", "/api/auth/register/finish", None, Some(finish)).await;
            statuses.push(reply.status);
        }
        assert_eq!(statuses, [StatusCode::OK, StatusCode::UNAUTHORIZED]);
    }

    #[tokio::test]
    async fn refuses_a_repeated_counter_during_concurrent_sign_ins() {
        let h = Harness::new().await;
        let link = h.link(&h.world.operator.id).await;
        let registered = h.register(&link, &Authenticator::new(1)).await;
        assert_eq!(registered.status, StatusCode::OK, "{}", registered.body);

        let mut requests = Vec::new();
        for _ in 0..2 {
            let start = h.send("POST", "/api/auth/login/start", None, None).await;
            assert_eq!(start.status, StatusCode::OK, "{}", start.body);
            let credential = Authenticator::new(1).get(&start.body["options"], &handle(&h.world.operator.id));
            requests.push(json!({ "ceremony": start.body["ceremony"], "credential": credential }));
        }
        let second = requests.pop().unwrap();
        let first = requests.pop().unwrap();
        let mut held_connections = Vec::new();
        for _ in 0..h.world.db.options().get_max_connections() {
            held_connections.push(h.world.db.acquire().await.unwrap());
        }
        let mut finishes = pin!(async {
            tokio::join!(
                h.send("POST", "/api/auth/login/finish", None, Some(first)),
                h.send("POST", "/api/auth/login/finish", None, Some(second)),
            )
        });
        poll_fn(|cx| {
            assert!(finishes.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        drop(held_connections.pop());
        let (first, second) = finishes.await;
        drop(held_connections);
        let mut statuses = [first.status, second.status];
        statuses.sort();
        assert_eq!(statuses, [StatusCode::OK, StatusCode::UNAUTHORIZED]);

        let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
            .fetch_one(&h.world.db)
            .await
            .unwrap();
        assert_eq!(sessions, 2);
    }

    #[tokio::test]
    async fn refuses_unknown_mismatched_and_disabled_passkeys() {
        let h = Harness::new().await;
        let operator = &h.world.operator;
        let mut authenticator = Authenticator::new(1);
        let registered = h.register(&h.link(&operator.id).await, &authenticator).await;
        assert_eq!(registered.status, StatusCode::OK, "{}", registered.body);

        let unknown = h.sign_in(&mut Authenticator::new(9), &handle(&operator.id)).await;
        assert_eq!(unknown.status, StatusCode::UNAUTHORIZED, "{}", unknown.body);
        let mismatched = h.sign_in(&mut authenticator, &handle(&h.world.owner.id)).await;
        assert_eq!(mismatched.status, StatusCode::UNAUTHORIZED, "{}", mismatched.body);

        let admin = h.world.admin_access().await;
        let mut tx = WriteTx::begin(&h.world.db).await.unwrap();
        user::set_disabled(&mut tx, &admin, &operator.id, true).await.unwrap();
        tx.commit().await.unwrap();
        let disabled = h.sign_in(&mut authenticator, &handle(&operator.id)).await;
        assert_eq!(disabled.status, StatusCode::UNAUTHORIZED, "{}", disabled.body);
        let me = h.send("GET", "/api/me", Some(&session(&registered)), None).await;
        assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn manages_passkeys_of_the_signed_in_user() {
        let h = Harness::new().await;
        let registered = h
            .register(&h.link(&h.world.operator.id).await, &Authenticator::new(1))
            .await;
        let cookie = session(&registered);

        let start = h
            .send("POST", "/api/passkeys/register/start", Some(&cookie), None)
            .await;
        assert_eq!(start.status, StatusCode::OK, "{}", start.body);
        let options = &start.body["options"]["publicKey"];
        assert_eq!(options["excludeCredentials"][0]["id"], b64(&[1; 16]));
        let selection = &options["authenticatorSelection"];
        assert_eq!(selection["residentKey"], "required");
        assert_eq!(selection["requireResidentKey"], true);
        let credential = Authenticator::new(2).create(&start.body["options"]);
        let finish = json!({ "ceremony": start.body["ceremony"], "credential": credential });
        let wrong_route = h.send("POST", "/api/auth/register/finish", None, Some(finish)).await;
        assert_eq!(wrong_route.status, StatusCode::BAD_REQUEST, "{}", wrong_route.body);

        let start = h
            .send("POST", "/api/passkeys/register/start", Some(&cookie), None)
            .await;
        let credential = Authenticator::new(2).create(&start.body["options"]);
        let finish = json!({ "ceremony": start.body["ceremony"], "credential": credential });
        let added = h
            .send("POST", "/api/passkeys/register/finish", Some(&cookie), Some(finish))
            .await;
        assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);
        assert_eq!(added.body["name"], DEFAULT_PASSKEY_NAME);

        let listed = h.send("GET", "/api/passkeys", Some(&cookie), None).await;
        let names: Vec<_> = listed.body.as_array().unwrap().iter().map(|p| &p["name"]).collect();
        assert_eq!(names, [&json!("Phone"), &json!("Passkey")]);
        assert!(listed.body[0].get("passkey").is_none());

        let second = format!("/api/passkeys/{}", added.body["id"].as_str().unwrap());
        let first = format!("/api/passkeys/{}", listed.body[0]["id"].as_str().unwrap());
        assert_eq!(
            h.send("DELETE", &first, None, None).await.status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            h.send("DELETE", &second, Some(&cookie), None).await.status,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            h.send("DELETE", &first, Some(&cookie), None).await.status,
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn refuses_bad_links_and_mismatched_ceremonies() {
        let h = Harness::new().await;
        let bad = h
            .send(
                "POST",
                "/api/auth/register/start",
                None,
                Some(json!({ "token": "nothing" })),
            )
            .await;
        assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
        assert!(bad.body["error"].as_str().unwrap().contains("registration link"));

        let start = h.send("POST", "/api/auth/login/start", None, None).await;
        let credential = Authenticator::new(1).create(&json!({ "publicKey": { "challenge": "" } }));
        let wrong_kind = json!({ "ceremony": start.body["ceremony"], "credential": credential });
        for _ in 0..2 {
            let reply = h
                .send("POST", "/api/auth/register/finish", None, Some(wrong_kind.clone()))
                .await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{}", reply.body);
        }
        assert_eq!(
            h.send("GET", "/api/me", None, None).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn ceremonies_are_used_once_and_bounded() {
        let ceremonies = Ceremonies::default();
        let webauthn = WebauthnBuilder::new("log.example.com", &Url::parse(ORIGIN).unwrap())
            .unwrap()
            .build()
            .unwrap();
        let start = || Ceremony::Authentication(webauthn.start_discoverable_authentication().unwrap().1);
        let id = ceremonies.start(start()).unwrap();
        assert!(matches!(ceremonies.finish(&id), Ok(Ceremony::Authentication(_))));
        assert!(ceremonies.finish(&id).is_err());
        for _ in 0..MAX_CEREMONIES {
            ceremonies.start(start()).unwrap();
        }
        let full = ceremonies.start(start()).unwrap_err().into_response();
        assert_eq!(full.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
