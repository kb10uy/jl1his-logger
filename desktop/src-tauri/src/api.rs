use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use url::Url;

const USER_AGENT: &str = concat!("jl1his-logger-desktop/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logbook {
    pub id: String,
    pub name: String,
    pub role: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Me {
    pub callsign: String,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("the server refused the token")]
    Unauthorized,
    #[error("could not reach the server: {0}")]
    Unreachable(String),
    #[error("the server answered {status}: {message}")]
    Server { status: u16, message: String },
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

pub struct Api {
    client: Client,
    origin: Url,
    token: String,
}

impl Api {
    pub fn new(origin: Url, token: String) -> Api {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("the HTTP client always builds");
        Api { client, origin, token }
    }

    pub async fn logbooks(&self) -> Result<Vec<Logbook>, ApiError> {
        self.get("/api/logbooks").await
    }

    pub async fn me(&self) -> Result<Me, ApiError> {
        self.get("/api/me").await
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let url = self.origin.join(path).expect("a fixed path joins");
        let response = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| ApiError::Unreachable(e.to_string()))?;
        match response.status() {
            StatusCode::OK => response.json().await.map_err(|e| ApiError::Unreachable(e.to_string())),
            StatusCode::UNAUTHORIZED => Err(ApiError::Unauthorized),
            status => {
                let message = match response.json::<ErrorBody>().await {
                    Ok(body) => body.error,
                    Err(_) => status.canonical_reason().unwrap_or_default().to_owned(),
                };
                Err(ApiError::Server {
                    status: status.as_u16(),
                    message,
                })
            }
        }
    }
}
