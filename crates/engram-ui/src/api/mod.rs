pub mod types;

use gloo_net::http::{Request, RequestBuilder};
use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::JsValue;

const FALLBACK_BASE_URL: &str = "http://localhost:3030";
const AUTH_STORAGE_KEY: &str = "engram_auth";

#[derive(Clone, Debug)]
pub struct ApiClient {
    pub base_url: String,
}

#[derive(Debug)]
pub enum ApiError {
    Network(String),
    Deserialize(String),
    Status(u16, String),
    Unauthorized,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Network(msg) => write!(f, "Network error: {msg}"),
            ApiError::Deserialize(msg) => write!(f, "Parse error: {msg}"),
            ApiError::Status(code, msg) => write!(f, "HTTP {code}: {msg}"),
            ApiError::Unauthorized => write!(f, "Unauthorized"),
        }
    }
}

impl ApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Derive API base URL from the browser's current origin.
    /// The frontend is always served by the same engram binary as the API,
    /// so window.location.origin is always correct -- even through tunnels/proxies.
    pub fn load_base_url() -> String {
        if let Some(origin) = web_sys::window()
            .and_then(|w| w.location().origin().ok())
        {
            if !origin.is_empty() && origin != "null" {
                return origin;
            }
        }
        FALLBACK_BASE_URL.to_string()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Get the current auth token from session storage.
    /// Public so SSE endpoints can pass it as a query parameter.
    pub fn auth_token() -> Option<String> {
        web_sys::window()
            .and_then(|w| w.session_storage().ok().flatten())
            .and_then(|s| s.get_item(AUTH_STORAGE_KEY).ok().flatten())
            .and_then(|json| {
                serde_json::from_str::<serde_json::Value>(&json)
                    .ok()
                    .and_then(|v| v.get("token")?.as_str().map(|s| s.to_string()))
            })
    }

    /// Inject auth header onto a RequestBuilder
    fn with_auth(builder: RequestBuilder) -> RequestBuilder {
        if let Some(token) = Self::auth_token() {
            builder.header("Authorization", &format!("Bearer {token}"))
        } else {
            builder
        }
    }

    fn handle_unauthorized() {
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.session_storage().ok().flatten())
        {
            let _ = storage.remove_item(AUTH_STORAGE_KEY);
        }
        if let Some(w) = web_sys::window() {
            let _ = w.location().reload();
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let resp = Self::with_auth(Request::get(&self.url(path)))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if resp.status() == 401 {
            Self::handle_unauthorized();
            return Err(ApiError::Unauthorized);
        }

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
        let req = Self::with_auth(Request::post(&self.url(path)))
            .json(body)
            .map_err(|e| ApiError::Deserialize(e.to_string()))?;

        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if resp.status() == 401 {
            Self::handle_unauthorized();
            return Err(ApiError::Unauthorized);
        }

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
        let req = Self::with_auth(Request::post(&self.url(path)))
            .json(body)
            .map_err(|e| ApiError::Deserialize(e.to_string()))?;

        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if resp.status() == 401 {
            Self::handle_unauthorized();
            return Err(ApiError::Unauthorized);
        }

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.text().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }

    pub async fn put<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let req = Self::with_auth(Request::put(&self.url(path)))
            .json(body)
            .map_err(|e| ApiError::Deserialize(e.to_string()))?;

        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if resp.status() == 401 {
            Self::handle_unauthorized();
            return Err(ApiError::Unauthorized);
        }

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.json().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }

    pub async fn put_text<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<String, ApiError> {
        let req = Self::with_auth(Request::put(&self.url(path)))
            .json(body)
            .map_err(|e| ApiError::Deserialize(e.to_string()))?;

        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if resp.status() == 401 {
            Self::handle_unauthorized();
            return Err(ApiError::Unauthorized);
        }

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.text().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }

    pub async fn patch<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let req = Self::with_auth(Request::patch(&self.url(path)))
            .json(body)
            .map_err(|e| ApiError::Deserialize(e.to_string()))?;

        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if resp.status() == 401 {
            Self::handle_unauthorized();
            return Err(ApiError::Unauthorized);
        }

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.json().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }

    pub async fn delete(&self, path: &str) -> Result<String, ApiError> {
        let resp = Self::with_auth(Request::delete(&self.url(path)))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if resp.status() == 401 {
            Self::handle_unauthorized();
            return Err(ApiError::Unauthorized);
        }

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.text().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }

    pub async fn delete_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let resp = Self::with_auth(Request::delete(&self.url(path)))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if resp.status() == 401 {
            Self::handle_unauthorized();
            return Err(ApiError::Unauthorized);
        }

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.json().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }

    pub async fn get_text(&self, path: &str) -> Result<String, ApiError> {
        let resp = Self::with_auth(Request::get(&self.url(path)))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if resp.status() == 401 {
            Self::handle_unauthorized();
            return Err(ApiError::Unauthorized);
        }

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.text().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }

    /// POST with FormData body (multipart file upload).
    /// Do NOT set Content-Type — the browser sets multipart boundary automatically.
    pub async fn post_formdata(&self, path: &str, form_data: web_sys::FormData) -> Result<String, ApiError> {
        let builder = Self::with_auth(Request::post(&self.url(path)));
        let resp = builder
            .body(JsValue::from(form_data))
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if resp.status() == 401 {
            Self::handle_unauthorized();
            return Err(ApiError::Unauthorized);
        }

        if !resp.ok() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status, body));
        }

        resp.text().await.map_err(|e| ApiError::Deserialize(e.to_string()))
    }
}
