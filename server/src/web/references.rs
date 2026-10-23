use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use sha2::{Digest, Sha256};

use crate::{
    config::ReferenceConfig,
    db::Db,
    domain::reference::{self, Cty, DxccCodes, Entity, ReferenceError, Subdivisions},
    store::{
        StoreError,
        reference::{ReferenceKind, current},
    },
};

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
    pub async fn cty(&self, db: &Db, paths: &ReferenceConfig) -> Result<Option<Arc<Cty>>, LoadError> {
        load(db, &self.cty, ReferenceKind::Cty, &paths.cty, reference::parse_cty).await
    }

    pub async fn dxcc(&self, db: &Db, paths: &ReferenceConfig) -> Result<Option<Arc<DxccCodes>>, LoadError> {
        load(db, &self.dxcc, ReferenceKind::Dxcc, &paths.dxcc, reference::parse_dxcc).await
    }

    pub async fn entity(&self, db: &Db, paths: &ReferenceConfig, call: &str) -> Result<Option<Entity>, LoadError> {
        let Some(cty) = self.cty(db, paths).await? else {
            return Ok(None);
        };
        let codes = self.dxcc(db, paths).await?;
        Ok(reference::entity(&cty, codes.as_deref(), call))
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
