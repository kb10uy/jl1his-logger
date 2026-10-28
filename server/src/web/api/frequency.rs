use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    store::{
        WriteTx,
        account::frequency::{self, FavoriteFrequency, FrequencyInput},
    },
    web::{AppState, api::auth::Caller, error::ApiError, extract::Json},
};

#[derive(Debug, Serialize)]
pub struct FrequencyBody {
    id: String,
    freq: String,
    mode: String,
    submode: String,
    created_at: String,
}

impl From<FavoriteFrequency> for FrequencyBody {
    fn from(favorite: FavoriteFrequency) -> FrequencyBody {
        FrequencyBody {
            id: favorite.id,
            freq: favorite.freq,
            mode: favorite.mode,
            submode: favorite.submode,
            created_at: favorite.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FrequencyRequest {
    freq: String,
    mode: String,
    #[serde(default)]
    submode: String,
}

pub async fn list(State(state): State<Arc<AppState>>, caller: Caller) -> Result<Json<Vec<FrequencyBody>>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let favorites = frequency::list(&mut conn, &caller.principal).await?;
    Ok(Json(favorites.into_iter().map(FrequencyBody::from).collect()))
}

pub async fn add(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Json(req): Json<FrequencyRequest>,
) -> Result<Json<FrequencyBody>, ApiError> {
    let input = FrequencyInput {
        freq: req.freq,
        mode: req.mode,
        submode: req.submode,
    };
    let mut tx = WriteTx::begin(&state.db).await?;
    let added = frequency::add(&mut tx, &caller.principal, &input).await?;
    tx.commit().await?;
    Ok(Json(added.into()))
}

pub async fn remove(
    State(state): State<Arc<AppState>>,
    caller: Caller,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut tx = WriteTx::begin(&state.db).await?;
    frequency::remove(&mut tx, &caller.principal, &id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::web::testing::{Auth, Harness};

    #[tokio::test]
    async fn sessions_and_tokens_keep_the_users_own_favorites() {
        let h = Harness::new().await;
        let operator = h.session(&h.world.operator).await;
        let owner = h.session(&h.world.owner).await;
        let token = h.token(&h.world.operator, &[&h.world.logbook.id]).await;

        let (status, _) = h.send("GET", "/api/frequencies", Auth::Anonymous, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let body = json!({ "freq": "145.000", "mode": "FM" });
        let (status, added) = h
            .send("POST", "/api/frequencies", Auth::Session(&operator), Some(body))
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            (&added["freq"], &added["mode"], &added["submode"]),
            (&json!("145"), &json!("FM"), &json!(""))
        );

        let body = json!({ "freq": "7.1", "mode": "SSB", "submode": "LSB" });
        let (status, _) = h
            .send("POST", "/api/frequencies", Auth::Bearer(&token), Some(body))
            .await;
        assert_eq!(status, StatusCode::OK);
        let (_, listed) = h.send("GET", "/api/frequencies", Auth::Bearer(&token), None).await;
        assert_eq!(listed.as_array().unwrap().len(), 2);
        assert_eq!(listed[0]["freq"], "7.1");
        let (_, theirs) = h.send("GET", "/api/frequencies", Auth::Session(&owner), None).await;
        assert_eq!(theirs, json!([]));

        let body = json!({ "freq": "7.1", "mode": "VOICE" });
        let (status, refused) = h
            .send("POST", "/api/frequencies", Auth::Session(&operator), Some(body))
            .await;
        assert_eq!(
            (status, &refused["code"]),
            (StatusCode::BAD_REQUEST, &json!("favorite_frequency_invalid"))
        );

        let one = format!("/api/frequencies/{}", added["id"].as_str().unwrap());
        let (status, _) = h.send("DELETE", &one, Auth::Session(&owner), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = h.send("DELETE", &one, Auth::Bearer(&token), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, listed) = h.send("GET", "/api/frequencies", Auth::Session(&operator), None).await;
        assert_eq!(listed.as_array().unwrap().len(), 1);
    }
}
