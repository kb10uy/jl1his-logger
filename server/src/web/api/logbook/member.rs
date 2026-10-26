use std::sync::Arc;

use axum::{
    extract::{FromRequestParts, Path},
    http::request::Parts,
};
use serde::Deserialize;

use crate::{
    store::access::{self, LogbookAccess},
    web::{AppState, api::auth::Caller, error::ApiError},
};

#[derive(Debug, Deserialize)]
struct LogbookPath {
    logbook_id: String,
}

#[derive(Debug, Deserialize)]
pub struct Item {
    pub id: String,
}

pub struct Member {
    pub caller: Caller,
    pub access: LogbookAccess,
}

impl FromRequestParts<Arc<AppState>> for Member {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Member, ApiError> {
        let caller = Caller::from_request_parts(parts, state).await?;
        let Path(path) = Path::<LogbookPath>::from_request_parts(parts, state)
            .await
            .map_err(|e| ApiError::new(e.status(), e.body_text()))?;
        let mut conn = state.db.acquire().await?;
        let access = access::logbook(&mut conn, &caller.principal, &path.logbook_id).await?;
        Ok(Member { caller, access })
    }
}
