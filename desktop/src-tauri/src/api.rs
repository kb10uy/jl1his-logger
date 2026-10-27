use std::time::Duration;

use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;
use url::Url;

const USER_AGENT: &str = concat!("jl1his-logger-desktop/", env!("CARGO_PKG_VERSION"));
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AreaField {
    pub name: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logbook {
    pub id: String,
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub area_field: Option<AreaField>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Me {
    pub callsign: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

impl From<Method> for reqwest::Method {
    fn from(method: Method) -> reqwest::Method {
        match method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Delete => reqwest::Method::DELETE,
        }
    }
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("the server refused the token")]
    Unauthorized,
    #[error("could not reach the server: {0}")]
    Unreachable(String),
    #[error("the server answered {status}: {message}")]
    Server { status: u16, message: String, body: Value },
    #[error("{0} is not a path of the native API")]
    NotApiPath(String),
}

#[derive(Clone)]
pub struct Api {
    client: Client,
    origin: Url,
    token: String,
}

pub fn is_api_path(path: &str) -> bool {
    path.starts_with("/api/") && !path.contains(['\\', '#'])
}

impl Api {
    pub fn new(origin: Url, token: String) -> Api {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(45))
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
        let response = self.send(Method::Get, path, None, Some(REQUEST_TIMEOUT)).await?;
        response.json().await.map_err(|e| ApiError::Unreachable(e.to_string()))
    }

    pub async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value, ApiError> {
        let response = self.send(method, path, body, Some(REQUEST_TIMEOUT)).await?;
        if response.status() == StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        response.json().await.map_err(|e| ApiError::Unreachable(e.to_string()))
    }

    pub async fn events(&self, logbook_id: &str, after: Option<u64>) -> Result<Response, ApiError> {
        let logbook: String = url::form_urlencoded::byte_serialize(logbook_id.as_bytes()).collect();
        let query = after.map(|after| format!("?after={after}")).unwrap_or_default();
        self.send(
            Method::Get,
            &format!("/api/logbooks/{logbook}/events{query}"),
            None,
            None,
        )
        .await
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        timeout: Option<Duration>,
    ) -> Result<Response, ApiError> {
        if !is_api_path(path) {
            return Err(ApiError::NotApiPath(path.to_owned()));
        }
        let url = self
            .origin
            .join(path)
            .map_err(|_| ApiError::NotApiPath(path.to_owned()))?;
        let mut request = self.client.request(method.into(), url).bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(body);
        }
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let response = request.send().await.map_err(|e| ApiError::Unreachable(e.to_string()))?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if status == StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }
        let body = response.json::<Value>().await.unwrap_or(Value::Null);
        let message = match body.get("error").and_then(Value::as_str) {
            Some(error) => error.to_owned(),
            None => status.canonical_reason().unwrap_or_default().to_owned(),
        };
        Err(ApiError::Server {
            status: status.as_u16(),
            message,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("/api/logbooks/abc/qsos?q=call%3AJA1*", true)]
    #[case("/api/me", true)]
    #[case("/api", false)]
    #[case("//evil.example/api/", false)]
    #[case("https://evil.example/api/", false)]
    #[case("/wl2/api/v2/qso", false)]
    #[case("/api/\\evil", false)]
    #[case("/api/logbooks#x", false)]
    fn accepts_only_native_api_paths(#[case] path: &str, #[case] expected: bool) {
        assert_eq!(is_api_path(path), expected);
    }

    #[test]
    fn reads_a_logbook_without_an_area_field() {
        let logbook: Logbook = serde_json::from_str(r#"{"id":"a","name":"Main","role":"owner"}"#).unwrap();
        assert!(logbook.area_field.is_none());
    }
}
