use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;
use webauthn_rs::prelude::{
    CreationChallengeResponse, Passkey as Credential, PasskeyRegistration, PublicKeyCredential,
    RegisterPublicKeyCredential, WebauthnError,
};

use super::{Ceremony, Me, SignedIn, expired, session_cookie, user_agent};
use crate::{
    store::{
        StoreError, WriteTx,
        access::{self, Principal},
        account::{
            passkey::{self, NewPasskey, Passkey},
            registration, session,
        },
    },
    web::{
        AppState,
        error::ApiError,
        extract::{Json, Query},
    },
};

const DEFAULT_PASSKEY_NAME: &str = "Passkey";

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
pub struct SignIn {
    ceremony: String,
    credential: PublicKeyCredential,
}

#[derive(Deserialize)]
pub struct Register {
    ceremony: String,
    credential: RegisterPublicKeyCredential,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
pub struct LinkStart {
    token: String,
}

pub async fn login_start(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    let (challenge, auth) = state
        .webauthn
        .start_discoverable_authentication()
        .map_err(webauthn_failed)?;
    let ceremony = state.ceremonies.start(Ceremony::Authentication(auth));
    let mut options = serde_json::to_value(challenge).map_err(|e| ApiError::internal(e.to_string()))?;
    if let Some(options) = options.as_object_mut() {
        options.remove("mediation");
    }
    Ok(Json(json!({ "ceremony": ceremony, "options": options })))
}

pub async fn login_finish(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<SignIn>,
) -> Result<(CookieJar, Json<Me>), ApiError> {
    let Ceremony::Authentication(auth) = state.ceremonies.finish(&req.ceremony)? else {
        return Err(expired());
    };
    let (user_handle, credential_id) = state
        .webauthn
        .identify_discoverable_authentication(&req.credential)
        .map_err(|_| unknown_passkey())?;
    let mut tx = WriteTx::begin(&state.db).await?;
    let stored = passkey::find_by_credential(&mut tx, credential_id)
        .await?
        .ok_or_else(unknown_passkey)?;
    if Uuid::parse_str(&stored.user_id).ok() != Some(user_handle) {
        return Err(unknown_passkey());
    }
    let mut credential = decode(&stored.passkey)?;
    let result = state
        .webauthn
        .finish_discoverable_authentication(&req.credential, auth, &[(&credential).into()])
        .map_err(|e| {
            tracing::info!("passkey sign-in failed: {e}");
            ApiError::new(StatusCode::UNAUTHORIZED, "passkey sign-in failed").with("code", "passkey_failed".into())
        })?;
    credential.update_credential(&result);

    passkey::record_use(&mut tx, &stored.id, &encode(&credential)?).await?;
    let token = session::create(&mut tx, &stored.user_id, user_agent(&headers)).await?;
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
    let mut started = start_registration(
        &state,
        &found.user.id,
        &found.user.callsign,
        found.credential_ids,
        Some(req.token),
    )?;
    started["callsign"] = found.user.callsign.into();
    Ok(Json(started))
}

pub async fn link_finish(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<Register>,
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
    let token = session::create(&mut tx, &user.id, user_agent(&headers)).await?;
    tx.commit().await?;
    tracing::info!("{} registered a passkey through a link", user.callsign);
    Ok((jar.add(session_cookie(token)), Json(Me::from(&user))))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    user_id: Option<String>,
}

pub async fn list_passkeys(
    State(state): State<Arc<AppState>>,
    SignedIn(user): SignedIn,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<PasskeyInfo>>, ApiError> {
    let user_id = query.user_id.as_deref().unwrap_or(user.user_id());
    let mut conn = state.db.acquire().await?;
    let passkeys = passkey::list(&mut conn, &user, user_id).await?;
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
    Ok(Json(start_registration(
        &state,
        user.user_id(),
        user.callsign(),
        exclude,
        None,
    )?))
}

pub async fn add_finish(
    State(state): State<Arc<AppState>>,
    SignedIn(user): SignedIn,
    Json(req): Json<Register>,
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
    link: Option<String>,
) -> Result<Value, ApiError> {
    let handle = Uuid::parse_str(user_id).map_err(|e| ApiError::internal(format!("user ID {user_id:?}: {e}")))?;
    let (challenge, registration) = state
        .webauthn
        .start_passkey_registration(handle, callsign, callsign, Some(exclude))
        .map_err(webauthn_failed)?;
    let ceremony = state.ceremonies.start(Ceremony::Registration {
        user_id: user_id.to_owned(),
        link,
        state: registration,
    });
    Ok(json!({ "ceremony": ceremony, "options": require_resident_key(challenge)? }))
}

fn finish_registration(
    state: &AppState,
    req: &Register,
    registration: &PasskeyRegistration,
) -> Result<NewPasskey, ApiError> {
    let credential = state
        .webauthn
        .finish_passkey_registration(&req.credential, registration)
        .map_err(|e| {
            tracing::info!("passkey registration failed: {e}");
            ApiError::new(StatusCode::BAD_REQUEST, "passkey registration failed").with("code", "passkey_failed".into())
        })?;
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

fn decode(json: &str) -> Result<Credential, ApiError> {
    serde_json::from_str(json).map_err(|e| ApiError::internal(format!("stored passkey: {e}")))
}

fn encode(credential: &Credential) -> Result<String, ApiError> {
    serde_json::to_string(credential).map_err(|e| ApiError::internal(format!("passkey: {e}")))
}

fn unknown_passkey() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "this passkey is not registered here, or its user is disabled",
    )
    .with("code", "passkey_unknown".into())
}

fn link_failed(e: StoreError) -> ApiError {
    match e {
        StoreError::Unauthenticated => ApiError::new(
            StatusCode::UNAUTHORIZED,
            "the registration link is invalid, used or expired; ask an admin for a new one",
        )
        .with("code", "registration_link_invalid".into()),
        e => e.into(),
    }
}

fn webauthn_failed(e: WebauthnError) -> ApiError {
    ApiError::internal(format!("WebAuthn: {e}"))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, future::poll_fn, pin::pin, task::Poll};

    use axum::http::header;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use p256::ecdsa::{Signature, SigningKey, signature::Signer};
    use serde_cbor_2::Value as Cbor;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::{
        store::account::{
            registration::{self, DEFAULT_TTL},
            user,
        },
        web::testing::{Auth, Harness, ORIGIN},
    };

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

    struct Reply {
        status: StatusCode,
        cookie: Option<String>,
        body: Value,
    }

    async fn send(h: &Harness, method: &str, uri: &str, cookie: Option<&str>, body: Option<Value>) -> Reply {
        let mut headers = vec![("origin", ORIGIN)];
        headers.extend(cookie.map(|cookie| ("cookie", cookie)));
        let (status, headers, body) = h.send_with(method, uri, Auth::Anonymous, &headers, body).await;
        let cookie = headers.get(header::SET_COOKIE).map(|v| v.to_str().unwrap().to_owned());
        Reply { status, cookie, body }
    }

    async fn link(h: &Harness, user_id: &str) -> String {
        let admin = h.world.admin_access().await;
        let mut tx = WriteTx::begin(&h.world.db).await.unwrap();
        let link = registration::issue(&mut tx, &admin, user_id, DEFAULT_TTL)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        link
    }

    async fn register(h: &Harness, link: &str, authenticator: &Authenticator) -> Reply {
        let start = send(
            h,
            "POST",
            "/api/auth/register/start",
            None,
            Some(json!({ "token": link })),
        )
        .await;
        assert_eq!(start.status, StatusCode::OK, "{}", start.body);
        assert_eq!(
            start.body["callsign"],
            start.body["options"]["publicKey"]["user"]["name"]
        );
        let credential = authenticator.create(&start.body["options"]);
        send(
            h,
            "POST",
            "/api/auth/register/finish",
            None,
            Some(json!({ "ceremony": start.body["ceremony"], "credential": credential, "name": " Phone " })),
        )
        .await
    }

    async fn sign_in(h: &Harness, authenticator: &mut Authenticator, user_handle: &str) -> Reply {
        let start = send(h, "POST", "/api/auth/login/start", None, None).await;
        assert_eq!(start.status, StatusCode::OK, "{}", start.body);
        assert!(start.body["options"].get("mediation").is_none());
        let credential = authenticator.get(&start.body["options"], user_handle);
        send(
            h,
            "POST",
            "/api/auth/login/finish",
            None,
            Some(json!({ "ceremony": start.body["ceremony"], "credential": credential })),
        )
        .await
    }

    async fn signed_in_operator(h: &Harness) -> String {
        let registered = register(h, &link(h, &h.world.operator.id).await, &Authenticator::new(1)).await;
        assert_eq!(registered.status, StatusCode::OK, "{}", registered.body);
        session(&registered)
    }

    async fn add_passkey(h: &Harness, cookie: &str, seed: u8) -> Reply {
        let start = send(h, "POST", "/api/passkeys/register/start", Some(cookie), None).await;
        assert_eq!(start.status, StatusCode::OK, "{}", start.body);
        let credential = Authenticator::new(seed).create(&start.body["options"]);
        let finish = json!({ "ceremony": start.body["ceremony"], "credential": credential });
        send(h, "POST", "/api/passkeys/register/finish", Some(cookie), Some(finish)).await
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
        let link = link(&h, &operator.id).await;
        let mut authenticator = Authenticator::new(1);

        let registered = register(&h, &link, &authenticator).await;
        assert_eq!(registered.status, StatusCode::OK, "{}", registered.body);
        assert_eq!(
            registered.body,
            json!({ "id": operator.id, "callsign": "JA1OPR", "role": "user" })
        );
        let cookie = session(&registered);
        assert!(cookie.starts_with("__Host-session="));
        let me = send(&h, "GET", "/api/me", Some(&cookie), None).await;
        assert_eq!((me.status, &me.body["callsign"]), (StatusCode::OK, &json!("JA1OPR")));
        session(&me);

        let again = send(
            &h,
            "POST",
            "/api/auth/register/start",
            None,
            Some(json!({ "token": link })),
        )
        .await;
        assert_eq!(again.status, StatusCode::UNAUTHORIZED, "{}", again.body);

        let out = send(&h, "POST", "/api/auth/logout", Some(&cookie), None).await;
        assert_eq!(out.status, StatusCode::NO_CONTENT);
        assert!(out.cookie.unwrap().contains("Max-Age=0"));
        let me = send(&h, "GET", "/api/me", Some(&cookie), None).await;
        assert_eq!(me.status, StatusCode::UNAUTHORIZED);

        let signed_in = sign_in(&h, &mut authenticator, &handle(&operator.id)).await;
        assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.body);
        assert_eq!(signed_in.body["callsign"], "JA1OPR");
        let me = send(&h, "GET", "/api/me", Some(&session(&signed_in)), None).await;
        assert_eq!(me.status, StatusCode::OK);

        let mut conn = h.world.db.acquire().await.unwrap();
        let stored = passkey::find_by_credential(&mut conn, &[1; 16]).await.unwrap().unwrap();
        assert_eq!(stored.name, "Phone");
        assert!(stored.last_used_at.is_some());
        assert_eq!(decode(&stored.passkey).unwrap().cred_id().as_slice(), [1; 16]);
        assert!(stored.passkey.contains(r#""counter":1"#), "{}", stored.passkey);
    }

    #[tokio::test]
    async fn a_link_registers_once_even_when_started_twice() {
        let h = Harness::new().await;
        let link = link(&h, &h.world.operator.id).await;
        let mut finishes = Vec::new();
        for seed in [1, 2] {
            let start = send(
                &h,
                "POST",
                "/api/auth/register/start",
                None,
                Some(json!({ "token": link })),
            )
            .await;
            let credential = Authenticator::new(seed).create(&start.body["options"]);
            finishes.push(json!({ "ceremony": start.body["ceremony"], "credential": credential }));
        }
        let mut statuses = Vec::new();
        for finish in finishes {
            let reply = send(&h, "POST", "/api/auth/register/finish", None, Some(finish)).await;
            statuses.push(reply.status);
        }
        assert_eq!(statuses, [StatusCode::OK, StatusCode::UNAUTHORIZED]);
    }

    #[tokio::test]
    async fn refuses_a_repeated_counter_during_concurrent_sign_ins() {
        let h = Harness::new().await;
        let link = link(&h, &h.world.operator.id).await;
        let registered = register(&h, &link, &Authenticator::new(1)).await;
        assert_eq!(registered.status, StatusCode::OK, "{}", registered.body);

        let mut requests = Vec::new();
        for _ in 0..2 {
            let start = send(&h, "POST", "/api/auth/login/start", None, None).await;
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
                send(&h, "POST", "/api/auth/login/finish", None, Some(first)),
                send(&h, "POST", "/api/auth/login/finish", None, Some(second)),
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
        let registered = register(&h, &link(&h, &operator.id).await, &authenticator).await;
        assert_eq!(registered.status, StatusCode::OK, "{}", registered.body);

        let unknown = sign_in(&h, &mut Authenticator::new(9), &handle(&operator.id)).await;
        assert_eq!(unknown.status, StatusCode::UNAUTHORIZED, "{}", unknown.body);
        let mismatched = sign_in(&h, &mut authenticator, &handle(&h.world.owner.id)).await;
        assert_eq!(mismatched.status, StatusCode::UNAUTHORIZED, "{}", mismatched.body);

        let admin = h.world.admin_access().await;
        let mut tx = WriteTx::begin(&h.world.db).await.unwrap();
        user::set_disabled(&mut tx, &admin, &operator.id, true).await.unwrap();
        tx.commit().await.unwrap();
        let disabled = sign_in(&h, &mut authenticator, &handle(&operator.id)).await;
        assert_eq!(disabled.status, StatusCode::UNAUTHORIZED, "{}", disabled.body);
        let me = send(&h, "GET", "/api/me", Some(&session(&registered)), None).await;
        assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn adds_a_resident_passkey_through_its_own_route() {
        let h = Harness::new().await;
        let cookie = signed_in_operator(&h).await;

        let start = send(&h, "POST", "/api/passkeys/register/start", Some(&cookie), None).await;
        assert_eq!(start.status, StatusCode::OK, "{}", start.body);
        let options = &start.body["options"]["publicKey"];
        assert_eq!(options["excludeCredentials"][0]["id"], b64(&[1; 16]));
        let selection = &options["authenticatorSelection"];
        assert_eq!(selection["residentKey"], "required");
        assert_eq!(selection["requireResidentKey"], true);
        let credential = Authenticator::new(2).create(&start.body["options"]);
        let finish = json!({ "ceremony": start.body["ceremony"], "credential": credential });
        let wrong_route = send(&h, "POST", "/api/auth/register/finish", None, Some(finish)).await;
        assert_eq!(wrong_route.status, StatusCode::BAD_REQUEST, "{}", wrong_route.body);

        let added = add_passkey(&h, &cookie, 2).await;
        assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);
        assert_eq!(added.body["name"], DEFAULT_PASSKEY_NAME);
    }

    #[tokio::test]
    async fn lists_and_deletes_passkeys_but_keeps_the_last() {
        let h = Harness::new().await;
        let cookie = signed_in_operator(&h).await;
        let added = add_passkey(&h, &cookie, 2).await;

        let listed = send(&h, "GET", "/api/passkeys", Some(&cookie), None).await;
        let names: Vec<_> = listed.body.as_array().unwrap().iter().map(|p| &p["name"]).collect();
        assert_eq!(names, [&json!("Phone"), &json!("Passkey")]);
        assert!(listed.body[0].get("passkey").is_none());

        let second = format!("/api/passkeys/{}", added.body["id"].as_str().unwrap());
        let first = format!("/api/passkeys/{}", listed.body[0]["id"].as_str().unwrap());
        assert_eq!(
            send(&h, "DELETE", &first, None, None).await.status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(&h, "DELETE", &second, Some(&cookie), None).await.status,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            send(&h, "DELETE", &first, Some(&cookie), None).await.status,
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn admins_list_and_delete_the_last_passkey_of_another_user() {
        let h = Harness::new().await;
        signed_in_operator(&h).await;
        let others = format!("/api/passkeys?user_id={}", h.world.operator.id);
        let outsider = h.session(&h.world.outsider).await;
        let (status, _) = h.send("GET", &others, Auth::Session(&outsider), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let admin = h.session(&h.world.admin).await;
        let (status, body) = h.send("GET", &others, Auth::Session(&admin), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body.as_array().unwrap().len(), 1);
        let only = format!("/api/passkeys/{}", body[0]["id"].as_str().unwrap());
        let (status, _) = h.send("DELETE", &only, Auth::Session(&admin), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, body) = h.send("GET", &others, Auth::Session(&admin), None).await;
        assert_eq!(body, json!([]));
    }

    #[tokio::test]
    async fn refuses_bad_links_and_mismatched_ceremonies() {
        let h = Harness::new().await;
        let bad = send(
            &h,
            "POST",
            "/api/auth/register/start",
            None,
            Some(json!({ "token": "nothing" })),
        )
        .await;
        assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
        assert!(bad.body["error"].as_str().unwrap().contains("registration link"));

        let start = send(&h, "POST", "/api/auth/login/start", None, None).await;
        let credential = Authenticator::new(1).create(&json!({ "publicKey": { "challenge": "" } }));
        let wrong_kind = json!({ "ceremony": start.body["ceremony"], "credential": credential });
        for _ in 0..2 {
            let reply = send(&h, "POST", "/api/auth/register/finish", None, Some(wrong_kind.clone())).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{}", reply.body);
        }
        assert_eq!(
            send(&h, "GET", "/api/me", None, None).await.status,
            StatusCode::UNAUTHORIZED
        );
    }
}
