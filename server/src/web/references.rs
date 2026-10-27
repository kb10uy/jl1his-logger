use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use axum::{
    body::Bytes,
    extract::{Path as UrlPath, State},
    http::StatusCode,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    config::ReferenceConfig,
    db::Db,
    domain::{
        qso::Qso,
        reference::{self, Cty, Entity, ReferenceError, Subdivisions},
    },
    store::{
        StoreError,
        reference::{self as versions, ReferenceKind, ReferenceVersion, current},
    },
    web::{
        AppState,
        api::auth::{Admin, SignedIn},
        error::ApiError,
        extract::Json,
    },
};

#[derive(Debug, Serialize)]
pub struct VersionBody {
    kind: ReferenceKind,
    version: String,
    updated_at: String,
}

impl From<ReferenceVersion> for VersionBody {
    fn from(version: ReferenceVersion) -> VersionBody {
        VersionBody {
            kind: version.kind,
            version: version.version,
            updated_at: version.updated_at,
        }
    }
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    SignedIn(user): SignedIn,
) -> Result<Json<Vec<VersionBody>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let installed = versions::list(&mut conn, &user).await?;
    Ok(Json(installed.into_iter().map(VersionBody::from).collect()))
}

pub async fn install_cty(
    State(state): State<Arc<AppState>>,
    Admin(admin): Admin,
    body: Bytes,
) -> Result<Json<VersionBody>, ApiError> {
    let installed = versions::install(
        &state.db,
        &admin,
        ReferenceKind::Cty,
        &state.config.reference.cty,
        &body,
        None,
    )
    .await?;
    Ok(Json(installed.version.into()))
}

#[derive(Debug, Serialize)]
pub struct SubdivisionBody {
    code: String,
    name: String,
}

pub async fn subdivisions(
    State(state): State<Arc<AppState>>,
    SignedIn(_): SignedIn,
    UrlPath(dxcc): UrlPath<String>,
) -> Result<Json<Vec<SubdivisionBody>>, ApiError> {
    let dxcc: u32 = dxcc
        .parse()
        .map_err(|_| ApiError::bad_request(format!("{dxcc:?} is not a DXCC entity code")))?;
    let subdivisions = state
        .references
        .subdivisions(&state.db, &state.config.reference)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no subdivisions are installed"))?;
    Ok(Json(
        subdivisions
            .get(&dxcc)
            .into_iter()
            .flatten()
            .map(|(code, name)| SubdivisionBody {
                code: code.clone(),
                name: name.clone(),
            })
            .collect(),
    ))
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    File(String),
}

struct Loaded<T> {
    version: String,
    checksum: Option<Vec<u8>>,
    data: Arc<T>,
}

type Slot<T> = Mutex<Option<Loaded<T>>>;

#[derive(Default)]
pub struct References {
    cty: Slot<Cty>,
    subdivisions: Slot<Subdivisions>,
}

impl References {
    async fn cty(&self, db: &Db, paths: &ReferenceConfig) -> Result<Option<Arc<Cty>>, LoadError> {
        load(
            db,
            &self.cty,
            ReferenceKind::Cty,
            &paths.cty,
            reference::parse_cty_archive,
        )
        .await
    }

    pub async fn entity(&self, db: &Db, paths: &ReferenceConfig, call: &str) -> Result<Option<Entity>, LoadError> {
        let Some(cty) = self.cty(db, paths).await? else {
            return Ok(None);
        };
        Ok(reference::entity(&cty, call))
    }

    pub async fn entity_of(&self, db: &Db, paths: &ReferenceConfig, qso: &Qso) -> Option<Entity> {
        let call = qso.get("CALL")?;
        match self.entity(db, paths, call).await {
            Ok(entity) => entity,
            Err(e) => {
                tracing::warn!("recording {call} without entity fields: {e}");
                None
            }
        }
    }

    pub async fn subdivisions(&self, db: &Db, paths: &ReferenceConfig) -> Result<Option<Arc<Subdivisions>>, LoadError> {
        load(
            db,
            &self.subdivisions,
            ReferenceKind::Subdivisions,
            &paths.subdivisions,
            |bytes| reference::parse_subdivisions(reference::text(bytes)?),
        )
        .await
    }
}

async fn load<T>(
    db: &Db,
    slot: &Slot<T>,
    kind: ReferenceKind,
    path: &Path,
    parse: fn(&[u8]) -> Result<T, ReferenceError>,
) -> Result<Option<Arc<T>>, LoadError> {
    let mut conn = db.acquire().await.map_err(StoreError::from)?;
    let Some(installed) = current(&mut conn, kind).await? else {
        return Ok(None);
    };
    drop(conn);
    if let Some(loaded) = slot.lock().expect("reference lock poisoned").as_ref()
        && loaded.version == installed.version
        && loaded.checksum == installed.checksum
    {
        return Ok(Some(loaded.data.clone()));
    }

    let failed = |e: &dyn std::fmt::Display| LoadError::File(format!("{} {}: {e}", kind.as_str(), path.display()));
    let bytes = tokio::fs::read(path).await.map_err(|e| failed(&e))?;
    if installed
        .checksum
        .as_deref()
        .is_some_and(|checksum| checksum != Sha256::digest(&bytes).as_slice())
    {
        return Err(failed(&format!(
            "the file is not version {} as installed; install it again",
            installed.version
        )));
    }
    let data = Arc::new(parse(&bytes).map_err(|e| failed(&e))?);
    *slot.lock().expect("reference lock poisoned") = Some(Loaded {
        version: installed.version,
        checksum: installed.checksum,
        data: data.clone(),
    });
    Ok(Some(data))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use rstest::rstest;
    use serde_json::{Value, json};

    use crate::{
        domain::reference::{
            MAX_ARCHIVE_BYTES,
            tests::{CTY, CTY_CSV, cty_archive},
        },
        store::{account::token::TokenKind, reference::ReferenceKind},
        web::testing::{Auth, Harness, ORIGIN},
    };

    const CTY_URI: &str = "/api/reference/cty";

    async fn lookup(h: &Harness, call: &str) -> Value {
        let session = h.session(&h.world.operator).await;
        let uri = h.path(&format!("/lookup?call={call}"));
        let (status, body) = h.send("GET", &uri, Auth::Session(&session), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["entity"].clone()
    }

    #[tokio::test]
    async fn installs_a_cty_archive_uploaded_by_an_admin() {
        let (h, dir) = Harness::with_references().await;
        let admin = h.session(&h.world.admin).await;
        let archive = cty_archive(CTY, CTY_CSV);
        let (status, body) = h.put_bytes(CTY_URI, Auth::Session(&admin), &[], &archive).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!((&body["kind"], &body["version"]), (&json!("cty"), &json!("20260915")));
        assert_eq!(std::fs::read(dir.path().join("cty.zip")).unwrap(), archive);
        assert_eq!(lookup(&h, "JA1XYZ").await["dxcc"], 339);

        let renamed = cty_archive(
            &CTY.replace("Japan:", "Nippon:").replace("20260915", "20261001"),
            CTY_CSV,
        );
        let (status, body) = h.put_bytes(CTY_URI, Auth::Session(&admin), &[], &renamed).await;
        assert_eq!((status, &body["version"]), (StatusCode::OK, &json!("20261001")));
        assert_eq!(lookup(&h, "JA1XYZ").await["name"], "Nippon");
        let (_, versions) = h.send("GET", "/api/reference", Auth::Session(&admin), None).await;
        assert_eq!(versions[0]["version"], "20261001");
    }

    #[tokio::test]
    async fn refuses_a_malformed_archive_and_keeps_the_installed_one() {
        let (h, dir) = Harness::with_references().await;
        let admin = h.session(&h.world.admin).await;
        let archive = cty_archive(CTY, CTY_CSV);
        h.put_bytes(CTY_URI, Auth::Session(&admin), &[], &archive).await;

        for (upload, reason) in [
            (CTY.as_bytes().to_vec(), "the file is not a zip archive"),
            (
                cty_archive(CTY, "JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,JA;\n"),
                "cty.csv has no row for JD/o of cty.dat",
            ),
            (Vec::new(), "the file is not a zip archive"),
        ] {
            let (status, body) = h.put_bytes(CTY_URI, Auth::Session(&admin), &[], &upload).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(
                body,
                json!({ "status": "error", "error": reason, "code": "reference_invalid" })
            );
        }
        assert_eq!(std::fs::read(dir.path().join("cty.zip")).unwrap(), archive);
        assert_eq!(lookup(&h, "JA1XYZ").await["name"], "Japan");
    }

    #[tokio::test]
    async fn refuses_an_archive_over_the_body_limit() {
        let (h, dir) = Harness::with_references().await;
        let admin = h.session(&h.world.admin).await;
        let mut archive = cty_archive(CTY, CTY_CSV);
        archive.resize(MAX_ARCHIVE_BYTES + 1, 0);
        let (status, _) = h.put_bytes(CTY_URI, Auth::Session(&admin), &[], &archive).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert!(!dir.path().join("cty.zip").exists());
    }

    #[rstest]
    #[case::operator("operator", StatusCode::FORBIDDEN)]
    #[case::owner("owner", StatusCode::FORBIDDEN)]
    #[case::anonymous("anonymous", StatusCode::UNAUTHORIZED)]
    #[case::admin_token("token", StatusCode::UNAUTHORIZED)]
    #[case::foreign_origin("foreign", StatusCode::FORBIDDEN)]
    #[tokio::test]
    async fn installs_only_for_an_admin_session_from_the_public_origin(
        #[case] who: &str,
        #[case] expected: StatusCode,
    ) {
        let (h, dir) = Harness::with_references().await;
        let operator = h.session(&h.world.operator).await;
        let owner = h.session(&h.world.owner).await;
        let admin = h.session(&h.world.admin).await;
        sqlx::query("INSERT INTO memberships (logbook_id, user_id, role, created_at) VALUES (?, ?, 'operator', 'now')")
            .bind(&h.world.logbook.id)
            .bind(&h.world.admin.id)
            .execute(&h.world.db)
            .await
            .unwrap();
        let token = h.token(&h.world.admin, TokenKind::Native, &[&h.world.logbook.id]).await;
        let (auth, headers): (Auth<'_>, Vec<(&str, &str)>) = match who {
            "operator" => (Auth::Session(&operator), vec![]),
            "owner" => (Auth::Session(&owner), vec![]),
            "anonymous" => (Auth::Anonymous, vec![]),
            "token" => (Auth::Bearer(&token), vec![]),
            "foreign" => (
                Auth::Anonymous,
                vec![("origin", "https://evil.example.com"), ("cookie", &admin)],
            ),
            _ => unreachable!(),
        };
        let (status, body) = h.put_bytes(CTY_URI, auth, &headers, &cty_archive(CTY, CTY_CSV)).await;
        assert_eq!(status, expected, "{body}");
        assert!(!dir.path().join("cty.zip").exists());
        let (status, _) = h
            .put_bytes(CTY_URI, Auth::Anonymous, &[("origin", ORIGIN), ("cookie", &admin)], &[])
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn hides_file_errors_from_the_client() {
        let (h, dir) = Harness::with_references().await;
        std::fs::create_dir(dir.path().join("cty.zip")).unwrap();
        let admin = h.session(&h.world.admin).await;
        let (status, body) = h
            .put_bytes(CTY_URI, Auth::Session(&admin), &[], &cty_archive(CTY, CTY_CSV))
            .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, json!({ "status": "error", "error": "internal server error" }));
        let (_, versions) = h.send("GET", "/api/reference", Auth::Session(&admin), None).await;
        assert_eq!(versions, json!([]));
    }

    #[tokio::test]
    async fn serves_the_subdivisions_of_an_entity() {
        let (h, _dir) = Harness::with_references().await;
        let session = h.session(&h.world.operator).await;
        let uri = "/api/reference/subdivisions/339";
        let (status, _) = h.send("GET", uri, Auth::Session(&session), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        h.install_reference(
            ReferenceKind::Subdivisions,
            "[339]
13 = \"Tokyo\"
01 = \"Hokkaido\"
[291]
CA = \"California\"
",
            "2026-10-23",
        )
        .await;
        let (status, body) = h.send("GET", uri, Auth::Session(&session), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body,
            json!([{ "code": "01", "name": "Hokkaido" }, { "code": "13", "name": "Tokyo" }])
        );

        let (status, body) = h
            .send("GET", "/api/reference/subdivisions/1", Auth::Session(&session), None)
            .await;
        assert_eq!((status, body), (StatusCode::OK, json!([])));
        let (status, _) = h
            .send(
                "GET",
                "/api/reference/subdivisions/Japan",
                Auth::Session(&session),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = h.send("GET", uri, Auth::Anonymous, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
