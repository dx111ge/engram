/// Multi-user authentication and authorization.
///
/// Users are stored in `.brain.users` (JSON, Argon2id password hashes).
/// Sessions are in-memory bearer tokens, lost on server restart.
/// The admin's password also serves as the encryption key for `.brain.secrets`.

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aes_gcm::aead::rand_core::{OsRng, RngCore};

use crate::state::AppState;

// ── Types ──

/// User role — determines what actions are permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Analyst,
    Reader,
}

impl Role {
    pub fn can_write(&self) -> bool {
        matches!(self, Role::Admin | Role::Analyst)
    }

    pub fn can_delete(&self) -> bool {
        matches!(self, Role::Admin | Role::Analyst)
    }

    pub fn can_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Admin => write!(f, "admin"),
            Role::Analyst => write!(f, "analyst"),
            Role::Reader => write!(f, "reader"),
        }
    }
}

/// A persistent API key with a label and creation time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub key_hash: String,
    pub label: String,
    pub created_at: u64,
}

/// A user record stored in `.brain.users`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub username: String,
    pub password_hash: String,
    pub salt: String,
    pub role: Role,
    pub trust_level: f32,
    pub enabled: bool,
    pub created_at: u64,
    #[serde(default)]
    pub api_keys: Vec<ApiKey>,
}

/// User info extracted from a valid session, injected into request extensions.
#[derive(Debug, Clone, Serialize)]
pub struct UserInfo {
    pub username: String,
    pub role: Role,
    pub trust_level: f32,
}

/// An active session.
#[derive(Debug, Clone)]
pub struct Session {
    pub username: String,
    pub role: Role,
    pub trust_level: f32,
    pub created_at: Instant,
    pub expires_at: Instant,
}

/// Default session duration: 24 hours.
const SESSION_DURATION: Duration = Duration::from_secs(24 * 60 * 60);

// ── UserStore ──

/// Persistent user store backed by a `.brain.users` JSON sidecar.
pub struct UserStore {
    users: HashMap<String, UserRecord>,
    path: PathBuf,
}

impl UserStore {
    /// Create an empty store (no file backing yet).
    pub fn empty() -> Self {
        Self {
            users: HashMap::new(),
            path: PathBuf::new(),
        }
    }

    /// Load users from a JSON file. Returns empty store if file doesn't exist.
    pub fn load(path: &Path) -> Self {
        let users = if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(contents) => {
                    serde_json::from_str::<Vec<UserRecord>>(&contents)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|u| (u.username.clone(), u))
                        .collect()
                }
                Err(_) => HashMap::new(),
            }
        } else {
            HashMap::new()
        };
        Self {
            users,
            path: path.to_path_buf(),
        }
    }

    /// Save users to the JSON file.
    pub fn save(&self) -> std::io::Result<()> {
        let records: Vec<&UserRecord> = self.users.values().collect();
        let json = serde_json::to_string_pretty(&records)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&self.path, json)
    }

    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }

    pub fn len(&self) -> usize {
        self.users.len()
    }

    pub fn get(&self, username: &str) -> Option<&UserRecord> {
        self.users.get(username)
    }

    pub fn list(&self) -> Vec<&UserRecord> {
        self.users.values().collect()
    }

    /// Create a new user with a hashed password. Returns error if username exists.
    pub fn create_user(
        &mut self,
        username: &str,
        password: &str,
        role: Role,
        trust_level: f32,
    ) -> Result<(), String> {
        if self.users.contains_key(username) {
            return Err(format!("user '{}' already exists", username));
        }
        if username.is_empty() {
            return Err("username cannot be empty".to_string());
        }
        if password.len() < 8 {
            return Err("password must be at least 8 characters".to_string());
        }

        let (hash, salt) = hash_password(password)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.users.insert(
            username.to_string(),
            UserRecord {
                username: username.to_string(),
                password_hash: hash,
                salt,
                role,
                trust_level: trust_level.clamp(0.0, 1.0),
                enabled: true,
                created_at: now,
                api_keys: Vec::new(),
            },
        );
        Ok(())
    }

    /// Verify a password against the stored hash. Returns the user record if valid.
    pub fn verify_password(&self, username: &str, password: &str) -> Option<&UserRecord> {
        let user = self.users.get(username)?;
        if !user.enabled {
            return None;
        }
        if verify_password(password, &user.password_hash, &user.salt) {
            Some(user)
        } else {
            None
        }
    }

    /// Update a user's role, trust_level, or enabled status.
    pub fn update_user(
        &mut self,
        username: &str,
        role: Option<Role>,
        trust_level: Option<f32>,
        enabled: Option<bool>,
    ) -> Result<(), String> {
        let user = self
            .users
            .get_mut(username)
            .ok_or_else(|| format!("user '{}' not found", username))?;
        if let Some(r) = role {
            user.role = r;
        }
        if let Some(t) = trust_level {
            user.trust_level = t.clamp(0.0, 1.0);
        }
        if let Some(e) = enabled {
            user.enabled = e;
        }
        Ok(())
    }

    /// Delete a user. Returns true if removed.
    pub fn delete_user(&mut self, username: &str) -> bool {
        self.users.remove(username).is_some()
    }

    /// Change a user's password.
    pub fn change_password(&mut self, username: &str, new_password: &str) -> Result<(), String> {
        if new_password.len() < 8 {
            return Err("password must be at least 8 characters".to_string());
        }
        let user = self
            .users
            .get_mut(username)
            .ok_or_else(|| format!("user '{}' not found", username))?;
        let (hash, salt) = hash_password(new_password)?;
        user.password_hash = hash;
        user.salt = salt;
        Ok(())
    }

    /// Generate a new API key for a user. Returns the raw key (only shown once).
    pub fn create_api_key(&mut self, username: &str, label: &str) -> Result<String, String> {
        let user = self
            .users
            .get_mut(username)
            .ok_or_else(|| format!("user '{}' not found", username))?;
        let raw_key = generate_api_key();
        let key_hash = hash_api_key(&raw_key);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        user.api_keys.push(ApiKey {
            key_hash,
            label: label.to_string(),
            created_at: now,
        });
        Ok(raw_key)
    }

    /// Revoke an API key by its hash prefix (first 16 chars).
    pub fn revoke_api_key(&mut self, username: &str, key_id: &str) -> Result<bool, String> {
        let user = self
            .users
            .get_mut(username)
            .ok_or_else(|| format!("user '{}' not found", username))?;
        let before = user.api_keys.len();
        user.api_keys.retain(|k| !k.key_hash.starts_with(key_id));
        Ok(user.api_keys.len() < before)
    }

    /// Validate an API key across all users. Returns user info if valid.
    pub fn validate_api_key(&self, raw_key: &str) -> Option<&UserRecord> {
        let key_hash = hash_api_key(raw_key);
        for user in self.users.values() {
            if !user.enabled {
                continue;
            }
            for ak in &user.api_keys {
                if ak.key_hash == key_hash {
                    return Some(user);
                }
            }
        }
        None
    }
}

// ── Password hashing ──

fn hash_password(password: &str) -> Result<(String, String), String> {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    let salt_hex = hex_encode(&salt);

    let mut hash = [0u8; 32];
    let argon2 = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(65536, 3, 1, Some(32))
            .map_err(|e| format!("argon2 params: {e}"))?,
    );
    argon2
        .hash_password_into(password.as_bytes(), &salt, &mut hash)
        .map_err(|e| format!("argon2 hash: {e}"))?;

    Ok((hex_encode(&hash), salt_hex))
}

fn verify_password(password: &str, stored_hash: &str, stored_salt: &str) -> bool {
    let salt = match hex_decode(stored_salt) {
        Some(s) => s,
        None => return false,
    };

    let mut hash = [0u8; 32];
    let argon2 = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        match argon2::Params::new(65536, 3, 1, Some(32)) {
            Ok(p) => p,
            Err(_) => return false,
        },
    );
    if argon2
        .hash_password_into(password.as_bytes(), &salt, &mut hash)
        .is_err()
    {
        return false;
    }

    hex_encode(&hash) == stored_hash
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

// ── API Key helpers ──

/// Generate a random API key: `egk_` + 48 random hex chars.
fn generate_api_key() -> String {
    let mut bytes = [0u8; 24];
    OsRng.fill_bytes(&mut bytes);
    format!("egk_{}", hex_encode(&bytes))
}

/// Hash an API key for storage (SHA-256 via simple hash — we use argon2 for passwords
/// but API keys are high-entropy so a fast hash is fine for storage).
fn hash_api_key(raw_key: &str) -> String {
    // Simple hash: repeated xor-fold with salt prefix. We don't need argon2 here
    // because API keys have 192 bits of entropy. Use a basic HMAC-like construction.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    raw_key.hash(&mut h);
    let a = h.finish();
    let mut h2 = DefaultHasher::new();
    format!("{}{}", raw_key, a).hash(&mut h2);
    let b = h2.finish();
    format!("{:016x}{:016x}", a, b)
}

// ── Session management ──

/// Generate a random 256-bit session token as hex string.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex_encode(&bytes)
}

/// Create a session and return the token.
pub fn create_session(
    sessions: &Arc<RwLock<HashMap<String, Session>>>,
    user: &UserRecord,
) -> Result<String, String> {
    let token = generate_token();
    let now = Instant::now();
    let session = Session {
        username: user.username.clone(),
        role: user.role,
        trust_level: user.trust_level,
        created_at: now,
        expires_at: now + SESSION_DURATION,
    };
    let mut map = sessions.write().map_err(|_| "session lock poisoned")?;
    map.insert(token.clone(), session);
    Ok(token)
}

/// Validate a token and return the session info.
pub fn validate_session(
    sessions: &Arc<RwLock<HashMap<String, Session>>>,
    token: &str,
) -> Option<UserInfo> {
    let map = sessions.read().ok()?;
    let session = map.get(token)?;
    if Instant::now() > session.expires_at {
        return None;
    }
    Some(UserInfo {
        username: session.username.clone(),
        role: session.role,
        trust_level: session.trust_level,
    })
}

/// Remove expired sessions. Called periodically.
pub fn cleanup_sessions(sessions: &Arc<RwLock<HashMap<String, Session>>>) {
    if let Ok(mut map) = sessions.write() {
        let now = Instant::now();
        map.retain(|_, s| s.expires_at > now);
    }
}

/// Invalidate all sessions for a given username.
pub fn invalidate_user_sessions(
    sessions: &Arc<RwLock<HashMap<String, Session>>>,
    username: &str,
) {
    if let Ok(mut map) = sessions.write() {
        map.retain(|_, s| s.username != username);
    }
}

// ── Middleware ──

/// Paths that don't require authentication.
fn is_public_path(path: &str, method: &axum::http::Method) -> bool {
    matches!(
        (method, path),
        (_, "/health")
            | (&axum::http::Method::GET, "/auth/status")
            | (&axum::http::Method::POST, "/auth/setup")
            | (&axum::http::Method::POST, "/auth/login")
    )
}

/// Returns true if the path looks like a static file request (frontend assets).
fn is_static_file(path: &str) -> bool {
    // API endpoints all start with a known prefix
    // Static files: /, /index.html, /js/*, /css/*, /favicon.ico, etc.
    let api_prefixes = [
        "/store", "/relate", "/batch", "/query", "/similar", "/search",
        "/ask", "/tell", "/node/", "/learn/", "/rules", "/export/", "/import/",
        "/quantize", "/mesh/", "/ingest", "/sources", "/actions/",
        "/events/", "/proxy/", "/assessments", "/secrets", "/config",
        "/reindex", "/health", "/stats", "/compute", "/explain/",
        "/tools", "/auth/", "/reason/", "/enrich/",
    ];
    !api_prefixes.iter().any(|prefix| path.starts_with(prefix))
}

/// Axum middleware that enforces authentication via bearer tokens.
pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let mut request = request;
    let path = request.uri().path().to_string();
    let method = request.method().clone();

    // Always allow static files through (frontend needs to load to show login)
    if is_static_file(&path) {
        return next.run(request).await;
    }

    // Always allow public auth endpoints
    if is_public_path(&path, &method) {
        return next.run(request).await;
    }

    // If no users exist (fresh install), allow everything (setup mode)
    let is_setup_mode = state.user_store.read()
        .map(|store| store.is_empty())
        .unwrap_or(false);
    if is_setup_mode {
        return next.run(request).await;
    }

    // Extract bearer token or X-Api-Key header
    let token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| {
            request
                .headers()
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        });

    let token = match token {
        Some(t) => t,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    // Check if this is an API key (starts with egk_)
    let user_info = if token.starts_with("egk_") {
        let info = state.user_store.read().ok().and_then(|store| {
            store.validate_api_key(&token).map(|u| UserInfo {
                username: u.username.clone(),
                role: u.role,
                trust_level: u.trust_level,
            })
        });
        match info {
            Some(i) => i,
            None => return StatusCode::UNAUTHORIZED.into_response(),
        }
    } else {
        // Validate session token
        match validate_session(&state.sessions, &token) {
            Some(info) => info,
            None => {
                if let Ok(mut sessions) = state.sessions.write() {
                    sessions.remove(&token);
                }
                return StatusCode::UNAUTHORIZED.into_response();
            }
        }
    };

    // Path-based role enforcement
    let needs_admin = path.starts_with("/config")
        || path.starts_with("/secrets")
        || path.starts_with("/reindex")
        || path.starts_with("/quantize")
        || path.starts_with("/auth/users");

    let needs_write = method == axum::http::Method::POST
        || method == axum::http::Method::PUT
        || method == axum::http::Method::PATCH
        || method == axum::http::Method::DELETE;

    // Admin-only endpoints
    if needs_admin && !user_info.role.can_admin() {
        return StatusCode::FORBIDDEN.into_response();
    }

    // Write operations (except on read-like POST endpoints)
    let is_read_post = matches!(
        path.as_str(),
        "/query" | "/search" | "/similar" | "/ask" | "/explain"
    ) || path.starts_with("/explain/");

    if needs_write && !is_read_post && !user_info.role.can_write() {
        if method != axum::http::Method::GET {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    // Inject user info into request extensions
    request.extensions_mut().insert(user_info);
    next.run(request).await
}

// ── Handler helpers ──

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg.into() })))
}

type AuthResult = Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>;

fn extract_token(request: &Request) -> Option<String> {
    request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

// ── Endpoint handlers ──

/// GET /auth/status
pub async fn auth_status(State(state): State<AppState>) -> AuthResult {
    let store = state
        .user_store
        .read()
        .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
    let status = if store.is_empty() {
        "setup_required"
    } else {
        "ready"
    };
    Ok(Json(serde_json::json!({
        "status": status,
        "users_count": store.len(),
    })))
}

/// POST /auth/setup — first-time admin account creation.
pub async fn auth_setup(
    State(state): State<AppState>,
    Json(body): Json<SetupRequest>,
) -> AuthResult {
    // Only works if no users exist
    {
        let store = state
            .user_store
            .read()
            .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
        if !store.is_empty() {
            return Err(api_err(
                StatusCode::CONFLICT,
                "admin account already exists, use /auth/login",
            ));
        }
    }

    // Create admin user
    {
        let mut store = state
            .user_store
            .write()
            .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
        store
            .create_user(&body.username, &body.password, Role::Admin, 1.0)
            .map_err(|e| api_err(StatusCode::BAD_REQUEST, e))?;
        store
            .save()
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("save failed: {e}")))?;
    }

    // Also create/open secrets store with this password
    unlock_secrets(&state, &body.password);

    // Auto-login: create session
    let user = {
        let store = state
            .user_store
            .read()
            .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
        store.get(&body.username).cloned().ok_or_else(|| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "user just created not found")
        })?
    };

    let token = create_session(&state.sessions, &user)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(serde_json::json!({
        "token": token,
        "username": user.username,
        "role": user.role,
        "trust_level": user.trust_level,
        "expires_in": SESSION_DURATION.as_secs(),
    })))
}

/// POST /auth/login
pub async fn auth_login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> AuthResult {
    let user = {
        let store = state
            .user_store
            .read()
            .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
        store
            .verify_password(&body.username, &body.password)
            .cloned()
    };

    let user = match user {
        Some(u) => u,
        None => return Err(api_err(StatusCode::UNAUTHORIZED, "invalid credentials")),
    };

    // If admin, also unlock secrets store (if not yet unlocked)
    if user.role == Role::Admin {
        unlock_secrets(&state, &body.password);
    }

    let token = create_session(&state.sessions, &user)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(serde_json::json!({
        "token": token,
        "username": user.username,
        "role": user.role,
        "trust_level": user.trust_level,
        "expires_in": SESSION_DURATION.as_secs(),
    })))
}

/// POST /auth/logout
pub async fn auth_logout(State(state): State<AppState>, request: Request) -> AuthResult {
    if let Some(token) = extract_token(&request) {
        if let Ok(mut sessions) = state.sessions.write() {
            sessions.remove(&token);
        }
    }
    Ok(Json(serde_json::json!({ "logged_out": true })))
}

/// GET /auth/users — admin only (enforced by middleware)
pub async fn list_users(State(state): State<AppState>) -> AuthResult {
    let store = state
        .user_store
        .read()
        .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
    let users: Vec<serde_json::Value> = store
        .list()
        .iter()
        .map(|u| {
            serde_json::json!({
                "username": u.username,
                "role": u.role,
                "trust_level": u.trust_level,
                "enabled": u.enabled,
                "created_at": u.created_at,
                "api_key_count": u.api_keys.len(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "users": users })))
}

/// POST /auth/users — create user (admin only)
pub async fn create_user(
    State(state): State<AppState>,
    Json(body): Json<CreateUserRequest>,
) -> AuthResult {
    let mut store = state
        .user_store
        .write()
        .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
    store
        .create_user(
            &body.username,
            &body.password,
            body.role,
            body.trust_level.unwrap_or(0.7),
        )
        .map_err(|e| api_err(StatusCode::BAD_REQUEST, e))?;
    store
        .save()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("save: {e}")))?;
    Ok(Json(serde_json::json!({
        "created": body.username,
        "role": body.role,
    })))
}

/// PUT /auth/users/:username — update user (admin only)
pub async fn update_user(
    State(state): State<AppState>,
    axum::extract::Path(username): axum::extract::Path<String>,
    Json(body): Json<UpdateUserRequest>,
) -> AuthResult {
    let mut store = state
        .user_store
        .write()
        .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
    store
        .update_user(&username, body.role, body.trust_level, body.enabled)
        .map_err(|e| api_err(StatusCode::NOT_FOUND, e))?;
    store
        .save()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("save: {e}")))?;

    // If disabled, invalidate their sessions
    if body.enabled == Some(false) {
        invalidate_user_sessions(&state.sessions, &username);
    }

    Ok(Json(serde_json::json!({ "updated": username })))
}

/// DELETE /auth/users/:username — delete user (admin only)
pub async fn delete_user(
    State(state): State<AppState>,
    axum::extract::Path(username): axum::extract::Path<String>,
    axum::Extension(user_info): axum::Extension<UserInfo>,
) -> AuthResult {
    // Prevent self-deletion
    if user_info.username == username {
        return Err(api_err(StatusCode::BAD_REQUEST, "cannot delete yourself"));
    }

    let mut store = state
        .user_store
        .write()
        .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
    if !store.delete_user(&username) {
        return Err(api_err(StatusCode::NOT_FOUND, "user not found"));
    }
    store
        .save()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("save: {e}")))?;

    invalidate_user_sessions(&state.sessions, &username);

    Ok(Json(serde_json::json!({ "deleted": username })))
}

/// POST /auth/change-password — any authenticated user changes their own password.
pub async fn change_password(
    State(state): State<AppState>,
    axum::Extension(user_info): axum::Extension<UserInfo>,
    Json(body): Json<ChangePasswordRequest>,
) -> AuthResult {

    // Verify old password
    {
        let store = state
            .user_store
            .read()
            .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
        if store
            .verify_password(&user_info.username, &body.old_password)
            .is_none()
        {
            return Err(api_err(StatusCode::UNAUTHORIZED, "incorrect current password"));
        }
    }

    // Update password
    {
        let mut store = state
            .user_store
            .write()
            .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
        store
            .change_password(&user_info.username, &body.new_password)
            .map_err(|e| api_err(StatusCode::BAD_REQUEST, e))?;
        store
            .save()
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("save: {e}")))?;
    }

    // If admin, re-encrypt secrets with new password
    if user_info.role == Role::Admin {
        if let Ok(mut guard) = state.secrets.write() {
            if let Some(ref mut s) = *guard {
                let _ = s.change_password(&body.new_password);
            }
        }
    }

    Ok(Json(serde_json::json!({ "changed": true })))
}

/// GET /auth/api-keys — list current user's API keys (hashes only, not raw keys).
pub async fn list_api_keys(
    State(state): State<AppState>,
    axum::Extension(user_info): axum::Extension<UserInfo>,
) -> AuthResult {
    let store = state
        .user_store
        .read()
        .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
    let user = store
        .get(&user_info.username)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "user not found"))?;
    let keys: Vec<serde_json::Value> = user
        .api_keys
        .iter()
        .map(|k| {
            serde_json::json!({
                "id": &k.key_hash[..16],
                "label": k.label,
                "created_at": k.created_at,
            })
        })
        .collect();
    Ok(Json(serde_json::json!(keys)))
}

/// POST /auth/api-keys — generate a new API key for the current user.
pub async fn create_api_key(
    State(state): State<AppState>,
    axum::Extension(user_info): axum::Extension<UserInfo>,
    Json(body): Json<CreateApiKeyRequest>,
) -> AuthResult {
    if body.label.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "label is required"));
    }
    let raw_key = {
        let mut store = state
            .user_store
            .write()
            .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
        let key = store
            .create_api_key(&user_info.username, &body.label)
            .map_err(|e| api_err(StatusCode::BAD_REQUEST, e))?;
        store
            .save()
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("save: {e}")))?;
        key
    };

    Ok(Json(serde_json::json!({
        "key": raw_key,
        "label": body.label,
        "warning": "Store this key securely. It will not be shown again.",
    })))
}

/// DELETE /auth/api-keys/{id} — revoke an API key by its ID (first 16 chars of hash).
pub async fn revoke_api_key(
    State(state): State<AppState>,
    axum::Extension(user_info): axum::Extension<UserInfo>,
    axum::extract::Path(key_id): axum::extract::Path<String>,
) -> AuthResult {
    let mut store = state
        .user_store
        .write()
        .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock poisoned"))?;
    let removed = store
        .revoke_api_key(&user_info.username, &key_id)
        .map_err(|e| api_err(StatusCode::BAD_REQUEST, e))?;
    if !removed {
        return Err(api_err(StatusCode::NOT_FOUND, "API key not found"));
    }
    store
        .save()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("save: {e}")))?;
    Ok(Json(serde_json::json!({ "revoked": true })))
}

// ── Helper: unlock secrets store ──

fn unlock_secrets(state: &AppState, password: &str) {
    // Check if already unlocked
    if let Ok(guard) = state.secrets.read() {
        if guard.is_some() {
            return;
        }
    }

    if let Some(ref path) = state.secrets_path {
        let store_result = if path.exists() {
            crate::secrets::SecretStore::open(path, password)
        } else {
            crate::secrets::SecretStore::create(path, password)
        };
        match store_result {
            Ok(store) => {
                if let Ok(mut guard) = state.secrets.write() {
                    *guard = Some(store);
                }
                tracing::info!("secrets store unlocked");
            }
            Err(e) => {
                tracing::warn!("failed to unlock secrets: {e}");
            }
        }
    }
}

// ── Request types ──

#[derive(Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: Role,
    pub trust_level: Option<f32>,
}

#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub role: Option<Role>,
    pub trust_level: Option<f32>,
    pub enabled: Option<bool>,
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct CreateApiKeyRequest {
    pub label: String,
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_verify_user() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.users");
        let mut store = UserStore::load(&path);

        store
            .create_user("alice", "password123", Role::Admin, 1.0)
            .unwrap();
        assert_eq!(store.len(), 1);

        assert!(store.verify_password("alice", "password123").is_some());
        assert!(store.verify_password("alice", "wrong").is_none());
        assert!(store.verify_password("bob", "password123").is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.users");

        {
            let mut store = UserStore::load(&path);
            store
                .create_user("admin", "secret12345", Role::Admin, 1.0)
                .unwrap();
            store
                .create_user("bob", "bobpass12345", Role::Analyst, 0.7)
                .unwrap();
            store.save().unwrap();
        }

        let store = UserStore::load(&path);
        assert_eq!(store.len(), 2);
        assert!(store.verify_password("admin", "secret12345").is_some());
        assert!(store.verify_password("bob", "bobpass12345").is_some());
        assert_eq!(store.get("bob").unwrap().role, Role::Analyst);
    }

    #[test]
    fn duplicate_user_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.users");
        let mut store = UserStore::load(&path);
        store
            .create_user("alice", "password123", Role::Admin, 1.0)
            .unwrap();
        assert!(store
            .create_user("alice", "other12345", Role::Reader, 0.5)
            .is_err());
    }

    #[test]
    fn short_password_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.users");
        let mut store = UserStore::load(&path);
        assert!(store.create_user("alice", "short", Role::Admin, 1.0).is_err());
    }

    #[test]
    fn disabled_user_cannot_login() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.users");
        let mut store = UserStore::load(&path);
        store
            .create_user("alice", "password123", Role::Admin, 1.0)
            .unwrap();
        store.update_user("alice", None, None, Some(false)).unwrap();
        assert!(store.verify_password("alice", "password123").is_none());
    }

    #[test]
    fn change_password_works() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.brain.users");
        let mut store = UserStore::load(&path);
        store
            .create_user("alice", "oldpass12345", Role::Admin, 1.0)
            .unwrap();
        store.change_password("alice", "newpass12345").unwrap();
        assert!(store.verify_password("alice", "oldpass12345").is_none());
        assert!(store.verify_password("alice", "newpass12345").is_some());
    }

    #[test]
    fn role_permissions() {
        assert!(Role::Admin.can_write());
        assert!(Role::Admin.can_delete());
        assert!(Role::Admin.can_admin());

        assert!(Role::Analyst.can_write());
        assert!(Role::Analyst.can_delete());
        assert!(!Role::Analyst.can_admin());

        assert!(!Role::Reader.can_write());
        assert!(!Role::Reader.can_delete());
        assert!(!Role::Reader.can_admin());
    }

    #[test]
    fn token_generation_unique() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2);
        assert_eq!(t1.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn session_lifecycle() {
        let sessions: Arc<RwLock<HashMap<String, Session>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let user = UserRecord {
            username: "test".to_string(),
            password_hash: String::new(),
            salt: String::new(),
            role: Role::Analyst,
            trust_level: 0.8,
            enabled: true,
            created_at: 0,
            api_keys: Vec::new(),
        };

        let token = create_session(&sessions, &user).unwrap();
        let info = validate_session(&sessions, &token).unwrap();
        assert_eq!(info.username, "test");
        assert_eq!(info.role, Role::Analyst);

        // Invalid token
        assert!(validate_session(&sessions, "bogus").is_none());

        // Invalidate
        invalidate_user_sessions(&sessions, "test");
        assert!(validate_session(&sessions, &token).is_none());
    }
}
