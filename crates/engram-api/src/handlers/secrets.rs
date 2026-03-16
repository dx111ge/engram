use super::*;

// ── Secrets endpoints ────────────────────────────────────────────────

// GET /secrets -- List secret keys (never values)
pub async fn list_secrets(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let guard = state.secrets.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "secrets lock poisoned")
    })?;
    if let Some(ref s) = *guard {
        let keys: Vec<&str> = s.keys();
        Ok(Json(serde_json::json!({ "keys": keys })))
    } else {
        Ok(Json(serde_json::json!({ "keys": [], "message": "secrets store not unlocked (admin must login first)" })))
    }
}

// POST /secrets/:key -- Set a secret
pub async fn set_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let value = body.get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "missing 'value' field"))?
        .to_string();

    let mut guard = state.secrets.write().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "secrets lock poisoned")
    })?;
    if let Some(ref mut s) = *guard {
        s.set(&key, value);
        s.save().map_err(|e| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("failed to save secrets: {e}"))
        })?;
        Ok(Json(serde_json::json!({ "set": key })))
    } else {
        Err(api_err(StatusCode::SERVICE_UNAVAILABLE, "secrets store not unlocked"))
    }
}

// DELETE /secrets/:key -- Remove a secret
pub async fn delete_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut guard = state.secrets.write().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "secrets lock poisoned")
    })?;
    if let Some(ref mut s) = *guard {
        let removed = s.remove(&key);
        if removed {
            s.save().map_err(|e| {
                api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("failed to save secrets: {e}"))
            })?;
        }
        Ok(Json(serde_json::json!({ "deleted": removed, "key": key })))
    } else {
        Err(api_err(StatusCode::SERVICE_UNAVAILABLE, "secrets store not unlocked"))
    }
}

// GET /secrets/:key/check -- Check if a secret exists (never expose value)
pub async fn check_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> ApiResult<serde_json::Value> {
    let guard = state.secrets.read().map_err(|_| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "secrets lock poisoned")
    })?;
    if let Some(ref s) = *guard {
        Ok(Json(serde_json::json!({ "key": key, "exists": s.has(&key) })))
    } else {
        Ok(Json(serde_json::json!({ "key": key, "exists": false })))
    }
}
