use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use axum::{
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
        reference::{self, Cty, DxccCodes, Entity, ReferenceError, Subdivisions},
    },
    store::{
        StoreError,
        reference::{self as versions, ReferenceKind, ReferenceVersion, current},
    },
    web::{AppState, auth::SignedIn, error::ApiError, extract::Json},
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
    dxcc: Slot<DxccCodes>,
    subdivisions: Slot<Subdivisions>,
}

impl References {
    async fn cty(&self, db: &Db, paths: &ReferenceConfig) -> Result<Option<Arc<Cty>>, LoadError> {
        load(db, &self.cty, ReferenceKind::Cty, &paths.cty, reference::parse_cty).await
    }

    async fn dxcc(&self, db: &Db, paths: &ReferenceConfig) -> Result<Option<Arc<DxccCodes>>, LoadError> {
        load(db, &self.dxcc, ReferenceKind::Dxcc, &paths.dxcc, reference::parse_dxcc).await
    }

    pub async fn entity(&self, db: &Db, paths: &ReferenceConfig, call: &str) -> Result<Option<Entity>, LoadError> {
        let Some(cty) = self.cty(db, paths).await? else {
            return Ok(None);
        };
        let codes = self.dxcc(db, paths).await?;
        Ok(reference::entity(&cty, codes.as_deref(), call))
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
            reference::parse_subdivisions,
        )
        .await
    }
}

async fn load<T>(
    db: &Db,
    slot: &Slot<T>,
    kind: ReferenceKind,
    path: &Path,
    parse: fn(&str) -> Result<T, ReferenceError>,
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
            "the file is not version {} as installed; install it again with the CLI",
            installed.version
        )));
    }
    let text = String::from_utf8(bytes).map_err(|e| failed(&e))?;
    let data = Arc::new(parse(&text).map_err(|e| failed(&e))?);
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
    use serde_json::json;

    use crate::{
        store::reference::ReferenceKind,
        web::testing::{Auth, Harness},
    };

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
