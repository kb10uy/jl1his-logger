use std::{fs, path::Path, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header, request::Builder},
};
use http_body_util::BodyExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tower::ServiceExt;
use url::Url;

use crate::{
    config::{Config, ReferenceConfig},
    store::{
        WriteTx,
        reference::{self, ReferenceKind},
        session,
        station::{self, Station, tests::input as station_input},
        testing::World,
        token::{self, TokenInput, TokenKind},
        user::User,
    },
    web::{AppState, router},
};

pub const ORIGIN: &str = "https://log.example.com";

pub enum Auth<'a> {
    Anonymous,
    Session(&'a str),
    Bearer(&'a str),
}

pub struct Harness {
    pub world: World,
    pub state: Arc<AppState>,
    pub app: Router,
    pub station: Station,
}

impl Harness {
    pub async fn new() -> Harness {
        Harness::with_config(Config::default()).await
    }

    pub async fn with_config(config: Config) -> Harness {
        let world = World::new().await;
        let state = Arc::new(AppState::new(world.db.clone(), config, Url::parse(ORIGIN).unwrap()).unwrap());
        let owner = world.access(&world.owner).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let station = station::create(&mut tx, &owner.owner().unwrap(), &station_input("Home"))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        Harness {
            app: router(state.clone(), None),
            state,
            world,
            station,
        }
    }

    pub async fn with_references() -> (Harness, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            reference: ReferenceConfig {
                cty: dir.path().join("cty.dat"),
                dxcc: dir.path().join("dxcc.toml"),
                subdivisions: dir.path().join("subdivisions.toml"),
                parks: dir.path().join("parks.toml"),
            },
            ..Config::default()
        };
        (Harness::with_config(config).await, dir)
    }

    pub fn reference_path(&self, kind: ReferenceKind) -> &Path {
        let paths = &self.state.config.reference;
        match kind {
            ReferenceKind::Cty => &paths.cty,
            ReferenceKind::Dxcc => &paths.dxcc,
            ReferenceKind::Subdivisions => &paths.subdivisions,
            ReferenceKind::Parks => &paths.parks,
        }
    }

    pub async fn install_reference(&self, kind: ReferenceKind, text: &str, version: &str) {
        fs::write(self.reference_path(kind), text).unwrap();
        self.record_reference(kind, version, text).await;
    }

    pub async fn record_reference(&self, kind: ReferenceKind, version: &str, text: &str) {
        let admin = self.world.admin_access().await;
        let mut tx = WriteTx::begin(&self.world.db).await.unwrap();
        let checksum = Sha256::digest(text);
        reference::set(&mut tx, &admin, kind, version, Some(checksum.as_slice()))
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    pub async fn session(&self, user: &User) -> String {
        let mut tx = WriteTx::begin(&self.world.db).await.unwrap();
        let token = session::create(&mut tx, &user.id).await.unwrap();
        tx.commit().await.unwrap();
        format!("__Host-session={token}")
    }

    pub async fn token(&self, user: &User, kind: TokenKind, logbook_ids: &[&str]) -> String {
        let access = self.world.user_access(user).await;
        let mut tx = WriteTx::begin(&self.world.db).await.unwrap();
        let issued = token::issue(
            &mut tx,
            &access,
            &TokenInput {
                kind,
                name: "test".into(),
                logbook_ids: logbook_ids.iter().map(|id| id.to_string()).collect(),
                wl2_prefix: false,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        issued.plaintext
    }

    pub async fn send(&self, method: &str, uri: &str, auth: Auth<'_>, body: Option<Value>) -> (StatusCode, Value) {
        let (status, _, body) = self.send_with(method, uri, auth, &[], body).await;
        (status, body)
    }

    pub async fn send_with(
        &self,
        method: &str,
        uri: &str,
        auth: Auth<'_>,
        headers: &[(&str, &str)],
        body: Option<Value>,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut req = authorize(Request::builder().method(method).uri(uri), auth);
        for (name, value) in headers {
            req = req.header(*name, *value);
        }
        let body = match body {
            Some(body) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                Body::from(body.to_string())
            }
            None => Body::empty(),
        };
        self.exchange(req.body(body).unwrap()).await
    }

    pub async fn send_text(&self, uri: &str, auth: Auth<'_>, text: &str) -> (StatusCode, Value) {
        let req = authorize(Request::post(uri), auth).header(header::CONTENT_TYPE, "text/plain");
        let (status, _, body) = self.exchange(req.body(Body::from(text.to_owned())).unwrap()).await;
        (status, body)
    }

    async fn exchange(&self, req: Request<Body>) -> (StatusCode, HeaderMap, Value) {
        let response = self.app.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = match bytes.is_empty() {
            true => Value::Null,
            false => {
                serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()))
            }
        };
        (status, headers, body)
    }
}

pub fn authorize(req: Builder, auth: Auth<'_>) -> Builder {
    match auth {
        Auth::Anonymous => req,
        Auth::Session(cookie) => req.header(header::ORIGIN, ORIGIN).header(header::COOKIE, cookie),
        Auth::Bearer(token) => req.header(header::AUTHORIZATION, format!("Bearer {token}")),
    }
}
