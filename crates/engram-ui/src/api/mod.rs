pub mod types;

use gloo_net::http::Request;
use serde::de::DeserializeOwned;
use serde::Serialize;

const DEFAULT_BASE_URL: &str = "http://localhost:3030";
const STORAGE_KEY: &str = "engram_api_url";

#[derive(Clone, Debug)]
pub struct ApiClient {
    pub base_url: String,
}

#[derive(Debug)]
pub enum ApiError {
    Network(String),
    Deserialize(String),
    Status(u16, String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Network(msg) => write!(f, "Network error: {msg}"),
            ApiError::Deserialize(msg) => write!(f, "Parse error: {msg}"),
            ApiError::Status(code, msg) => write!(f, "HTTP {code}: {msg}"),
        }
    }
}

impl ApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Load saved base URL from localStorage, or return default.
    pub fn load_base_url() -> String {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
    }

    /// Save base URL to localStorage.
    pub fn save_base_url(url: &str) {
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
        {
            let _ = storage.set_item(STORAGE_KEY, url);
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let resp = Request::get(&self.url(path))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.json().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }

    pub async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let resp = Request::post(&self.url(path))
            .json(body)
            .map_err(|e| ApiError::Deserialize(e.to_string()))?
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.json().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }

    pub async fn post_text<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<String, ApiError> {
        let resp = Request::post(&self.url(path))
            .json(body)
            .map_err(|e| ApiError::Deserialize(e.to_string()))?
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.text().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }

    pub async fn delete(&self, path: &str) -> Result<String, ApiError> {
        let resp = Request::delete(&self.url(path))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.text().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }

    pub async fn get_text(&self, path: &str) -> Result<String, ApiError> {
        let resp = Request::get(&self.url(path))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.text().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }
}
