use super::*;

// ── ONNX model upload ────────────────────────────────────────────────

/// POST /config/onnx-model -- Upload ONNX embedding model and tokenizer files.
/// Accepts multipart/form-data with "model" and "tokenizer" file fields.
/// Files are streamed to disk as sidecars next to the .brain file.
pub async fn upload_onnx_model(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> ApiResult<serde_json::Value> {
    use std::io::Write;

    // Derive brain base path from config_path (strip .config suffix)
    let brain_path = state.config_path.as_ref()
        .and_then(|p| p.to_str())
        .and_then(|s| s.strip_suffix(".config"))
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine brain file path"))?;

    let model_path = format!("{}.model.onnx", brain_path);
    let tokenizer_path = format!("{}.tokenizer.json", brain_path);

    // Process each multipart field
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        api_err(StatusCode::BAD_REQUEST, format!("multipart error: {e}"))
    })? {
        let name = field.name().unwrap_or("").to_string();
        let dest = match name.as_str() {
            "model" => &model_path,
            "tokenizer" => &tokenizer_path,
            _ => continue,
        };
        let data = field.bytes().await.map_err(|e| {
            api_err(StatusCode::BAD_REQUEST, format!("read field '{name}' failed: {e}"))
        })?;
        let mut f = std::fs::File::create(dest)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write {name} failed: {e}")))?;
        f.write_all(&data)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write {name} failed: {e}")))?;
    }

    // Check what files exist now
    let model_exists = std::path::Path::new(&model_path).exists();
    let tokenizer_exists = std::path::Path::new(&tokenizer_path).exists();
    let model_size = if model_exists {
        std::fs::metadata(&model_path).map(|m| m.len()).unwrap_or(0)
    } else { 0 };

    // Hot-load the ONNX embedder if both files are present
    let mut activated = false;
    #[cfg(feature = "onnx")]
    if model_exists && tokenizer_exists {
        match engram_core::OnnxEmbedder::load(
            std::path::Path::new(&model_path),
            std::path::Path::new(&tokenizer_path),
        ) {
            Ok(embedder) => {
                let dim = embedder.dim();
                let mut g = state.graph.write().map_err(|_| write_lock_err())?;
                g.set_embedder(Box::new(embedder));
                drop(g);
                state.set_embedder_info("ONNX Local".into(), dim, "local".into());
                tracing::info!("hot-loaded ONNX embedder ({}D) from {}", dim, model_path);
                activated = true;
            }
            Err(e) => {
                tracing::warn!("ONNX embedder load failed (files saved but not activated): {e}");
            }
        }
    }

    let message = if !model_exists || !tokenizer_exists {
        "Partial upload. Both model.onnx and tokenizer.json are required."
    } else if activated {
        "ONNX embedder activated. You can now reindex."
    } else {
        "ONNX model files installed. Restart the server to activate."
    };

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_path": model_path,
        "tokenizer_path": tokenizer_path,
        "model_exists": model_exists,
        "tokenizer_exists": tokenizer_exists,
        "model_size_mb": model_size as f64 / 1_048_576.0,
        "activated": activated,
        "message": message,
    })))
}

/// GET /config/onnx-model -- Check if ONNX model files exist.
pub async fn check_onnx_model(
    State(_state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    // Check ~/.engram/models/embed/ for any installed model
    let embed_dir = engram_home().map(|h| h.join("models").join("embed"));
    let found_model = embed_dir.as_ref().and_then(|dir| {
        std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).find(|e| {
            let p = e.path();
            p.join("model.onnx").exists() && p.join("tokenizer.json").exists()
        }).map(|e| e.path())
    });

    if let Some(ref model_dir) = found_model {
        let model_path = model_dir.join("model.onnx");
        let model_size = std::fs::metadata(&model_path).map(|m| m.len()).unwrap_or(0);
        Ok(Json(serde_json::json!({
            "ready": true,
            "model_exists": true,
            "tokenizer_exists": true,
            "model_path": model_path.to_string_lossy(),
            "model_size_mb": model_size as f64 / 1_048_576.0,
        })))
    } else {
        Ok(Json(serde_json::json!({
            "ready": false,
            "model_exists": false,
            "tokenizer_exists": false,
            "model_size_mb": 0.0,
        })))
    }
}

/// POST /config/onnx-download -- Download ONNX embedding model from HuggingFace.
///
/// Accepts JSON: { "model_url": "...", "tokenizer_url": "..." }
/// Downloads both files server-side and installs them as the active embedder.
pub async fn download_onnx_model(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let model_url = body["model_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_url required"))?;
    let tokenizer_url = body["tokenizer_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "tokenizer_url required"))?;

    // Validate URLs point to huggingface.co
    for url in [model_url, tokenizer_url] {
        if !url.starts_with("https://huggingface.co/") {
            return Err(api_err(StatusCode::BAD_REQUEST, "only HuggingFace URLs are allowed"));
        }
    }

    // Derive model name from URL or request body
    let model_name = body["model_id"].as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Extract from URL: https://huggingface.co/intfloat/multilingual-e5-small/resolve/...
            model_url.split('/').nth(4).unwrap_or("default").to_string()
        });

    // Save to ~/.engram/models/embed/<model_name>/
    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("embed").join(&model_name);
    tokio::fs::create_dir_all(&model_dir).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create model dir failed: {e}")))?;

    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    // Skip download if files already exist and model is >1MB (not a stub)
    let force = body["force"].as_bool().unwrap_or(false);
    if !force && model_path.exists() && tokenizer_path.exists() {
        let size = tokio::fs::metadata(&model_path).await.map(|m| m.len()).unwrap_or(0);
        if size > 1_000_000 {
            tracing::info!("ONNX embed model already installed ({} MB), skipping download", size / 1_048_576);
            // Still hot-load if not yet active
            let mut activated = false;
            #[cfg(feature = "onnx")]
            {
                match engram_core::OnnxEmbedder::load(&model_path, &tokenizer_path) {
                    Ok(embedder) => {
                        let dim = embedder.dim();
                        let mut g = state.graph.write().map_err(|_| write_lock_err())?;
                        g.set_embedder(Box::new(embedder));
                        drop(g);
                        state.set_embedder_info(model_name.clone(), dim, "local".into());
                        activated = true;
                    }
                    Err(e) => {
                        tracing::debug!("ONNX hot-load on skip: {e}");
                    }
                }
            }
            return Ok(Json(serde_json::json!({
                "status": "ok",
                "skipped": true,
                "message": "model already installed",
                "model_size_mb": size / 1_048_576,
                "activated": activated,
            })));
        }
    }

    let client = reqwest::Client::new();

    // Download tokenizer first (small, quick validation)
    tracing::info!("downloading tokenizer from {}", tokenizer_url);
    let resp = client.get(tokenizer_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download returned {}", resp.status())));
    }
    let tokenizer_bytes = resp.bytes().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer read failed: {e}")))?;
    tokio::fs::write(&tokenizer_path, &tokenizer_bytes).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write tokenizer failed: {e}")))?;

    // Download model (large, stream to disk)
    tracing::info!("downloading model from {}", model_url);
    let resp = client.get(model_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("model download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("model download returned {}", resp.status())));
    }
    let content_length = resp.content_length().unwrap_or(0);

    let mut file = tokio::fs::File::create(&model_path).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create model file failed: {e}")))?;

    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("download stream error: {e}")))?;
        file.write_all(&chunk).await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write model failed: {e}")))?;
        downloaded += chunk.len() as u64;
    }
    file.flush().await.map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("flush failed: {e}")))?;

    tracing::info!("downloaded {} MB model to {}", downloaded / 1_048_576, model_path.display());

    // Hot-load the ONNX embedder
    let mut activated = false;
    let mut load_error: Option<String> = None;
    #[cfg(feature = "onnx")]
    {
        match engram_core::OnnxEmbedder::load(&model_path, &tokenizer_path) {
            Ok(embedder) => {
                let dim = embedder.dim();
                let mut g = state.graph.write().map_err(|_| write_lock_err())?;
                g.set_embedder(Box::new(embedder));
                drop(g);
                state.set_embedder_info(model_name.clone(), dim, "local".into());
                tracing::info!("hot-loaded ONNX embedder {} ({}D)", model_name, dim);
                activated = true;
            }
            Err(e) => {
                tracing::warn!("ONNX load failed after download: {e}");
                load_error = Some(e.to_string());
            }
        }
    }

    let message = if activated {
        "ONNX embedder downloaded and activated.".to_string()
    } else if let Some(ref err) = load_error {
        format!("ONNX model downloaded but hot-load failed: {err}. Restart the server to activate.")
    } else {
        "ONNX model downloaded. Restart the server to activate.".to_string()
    };

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_path": model_path.to_string_lossy(),
        "tokenizer_path": tokenizer_path.to_string_lossy(),
        "model_size_mb": downloaded as f64 / 1_048_576.0,
        "content_length_mb": content_length as f64 / 1_048_576.0,
        "activated": activated,
        "message": message,
    })))
}

// ── POST /config/ollama-pull -- Pull a model from Ollama ──

pub async fn ollama_pull(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let model = body["model"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model required"))?;

    // Get Ollama endpoint from config -- prefer llm_endpoint, skip non-HTTP (e.g. onnx://)
    let ollama_base = {
        let cfg = state.config.read().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "config lock"))?;
        let candidates = [cfg.llm_endpoint.clone(), cfg.embed_endpoint.clone()];
        candidates.into_iter()
            .flatten()
            .find(|ep| ep.starts_with("http://") || ep.starts_with("https://"))
            .unwrap_or_else(|| "http://localhost:11434".to_string())
    };
    // Extract base URL (strip path)
    let base = if let Some(idx) = ollama_base.find("/api/") {
        &ollama_base[..idx]
    } else if let Some(idx) = ollama_base.find("/v1/") {
        &ollama_base[..idx]
    } else {
        ollama_base.trim_end_matches('/')
    };

    let pull_url = format!("{}/api/pull", base);
    tracing::info!("pulling Ollama model '{}' from {}", model, pull_url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600)) // 10 min timeout for large models
        .build()
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("client error: {e}")))?;

    let resp = client.post(&pull_url)
        .json(&serde_json::json!({ "name": model, "stream": false }))
        .send()
        .await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Ollama pull failed: {e}. Is Ollama running at {}?", base)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("Ollama pull returned {}: {}", status, body)));
    }

    let result = resp.text().await.unwrap_or_default();
    tracing::info!("Ollama pull complete for '{}'", model);

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model": model,
        "response": result,
    })))
}

// ── POST /config/ner-download -- Download GLiNER ONNX NER model from HuggingFace ──

/// Download a GLiNER ONNX NER model from HuggingFace to `~/.engram/models/ner/`.
///
/// Downloads `tokenizer.json` + `onnx/model_{variant}.onnx` (saved as `model.onnx`).
/// Default model: `knowledgator/gliner-x-small`, default variant: `quantized` (173 MB).
// ── POST /config/gliner2-download -- Download GLiNER2 ONNX model from HuggingFace ──

/// Download GLiNER2 multi-file ONNX model from HuggingFace.
///
/// Request: `{"repo_id": "dx111ge/gliner2-multi-v1-onnx", "variant": "fp16", "force": false}`
///
/// Downloads all required ONNX files + tokenizer + config to `~/.engram/models/gliner2/<model>/`.
pub async fn download_gliner2_model(
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let repo_id = body["repo_id"].as_str()
        .unwrap_or("dx111ge/gliner2-multi-v1-onnx")
        .to_string();
    let variant = body["variant"].as_str().unwrap_or("fp16").to_string();
    let force = body["force"].as_bool().unwrap_or(false);

    if repo_id.matches('/').count() != 1 || repo_id.contains("..") || repo_id.contains('\\') {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid repo_id format"));
    }

    // Model name from repo_id (e.g., "gliner2-multi-v1-onnx")
    let model_name = repo_id.split('/').last().unwrap_or("gliner2");

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("gliner2").join(model_name);

    // Skip if already installed
    let config_path = model_dir.join("gliner2_config.json");
    if !force && config_path.exists() {
        return Ok(Json(serde_json::json!({
            "status": "ok",
            "skipped": true,
            "message": "model already installed",
            "model_dir": model_dir.to_string_lossy(),
        })));
    }

    tokio::fs::create_dir_all(&model_dir).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create dir: {e}")))?;

    let base_url = format!("https://huggingface.co/{}/resolve/main", repo_id);
    let client = reqwest::Client::new();

    // Files to download: config first (tells us which ONNX files to get)
    let config_files = vec![
        "gliner2_config.json",
        "tokenizer.json",
        "tokenizer_config.json",
        "special_tokens_map.json",
        "added_tokens.json",
        "spm.model",
    ];

    // Download config files
    for filename in &config_files {
        let url = format!("{}/{}", base_url, filename);
        let dest = model_dir.join(filename);
        tracing::info!(file = %filename, "downloading");
        let resp = client.get(&url).send().await
            .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("download {filename}: {e}")))?;
        if resp.status().is_success() {
            let bytes = resp.bytes().await
                .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("read {filename}: {e}")))?;
            tokio::fs::write(&dest, &bytes).await
                .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write {filename}: {e}")))?;
        }
    }

    // Read config to find ONNX files for requested variant
    let cfg_str = tokio::fs::read_to_string(&config_path).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("read config: {e}")))?;
    let cfg: serde_json::Value = serde_json::from_str(&cfg_str)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("parse config: {e}")))?;

    let onnx_files = &cfg["onnx_files"][&variant];
    if onnx_files.is_null() {
        return Err(api_err(StatusCode::BAD_REQUEST,
            format!("variant '{}' not found in config", variant)));
    }

    // Collect unique ONNX filenames (+ their .data files)
    let mut files_to_download: Vec<String> = Vec::new();
    for (_key, val) in onnx_files.as_object().unwrap_or(&serde_json::Map::new()) {
        if let Some(fname) = val.as_str() {
            if !files_to_download.contains(&fname.to_string()) {
                files_to_download.push(fname.to_string());
                files_to_download.push(format!("{}.data", fname));
            }
        }
    }

    // Stream-download ONNX files
    let mut total_bytes: u64 = 0;
    for filename in &files_to_download {
        let url = format!("{}/{}", base_url, filename);
        let dest = model_dir.join(filename);

        if !force && dest.exists() {
            let size = tokio::fs::metadata(&dest).await.map(|m| m.len()).unwrap_or(0);
            if size > 0 {
                total_bytes += size;
                continue;
            }
        }

        tracing::info!(file = %filename, "downloading ONNX file");
        let resp = client.get(&url).send().await
            .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("download {filename}: {e}")))?;

        if !resp.status().is_success() {
            // .data files might not exist for single-file models (e.g., int8)
            if filename.ends_with(".data") {
                continue;
            }
            return Err(api_err(StatusCode::BAD_GATEWAY,
                format!("{} returned {}", filename, resp.status())));
        }

        let mut file = tokio::fs::File::create(&dest).await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create {filename}: {e}")))?;
        let mut stream = resp.bytes_stream();
        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("stream {filename}: {e}")))?;
            total_bytes += chunk.len() as u64;
            file.write_all(&chunk).await
                .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write {filename}: {e}")))?;
        }
    }

    Ok(Json(serde_json::json!({
        "status": "ok",
        "repo_id": repo_id,
        "variant": variant,
        "model_dir": model_dir.to_string_lossy(),
        "total_mb": total_bytes / 1_048_576,
    })))
}

///
/// Request: `{"model_id": "knowledgator/gliner-x-small", "variant": "quantized", "force": false}`
///
/// For air-gapped systems, use `/config/model-upload` instead.
pub async fn download_ner_model(
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let model_id = body["model_id"].as_str().unwrap_or("knowledgator/gliner-x-small").to_string();
    let variant = body["variant"].as_str().unwrap_or("quantized").to_string();
    let force = body["force"].as_bool().unwrap_or(false);

    // Validate model_id format: must contain exactly one "/"
    if model_id.matches('/').count() != 1 || model_id.contains("..") || model_id.contains('\\') {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid model_id format (expected 'org/model')"));
    }

    // Derive safe local directory name (replace / with _)
    let safe_name = model_id.replace('/', "_");

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("ner").join(&safe_name);

    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    // Skip if already installed
    if !force && model_path.exists() && tokenizer_path.exists() {
        let size = tokio::fs::metadata(&model_path).await.map(|m| m.len()).unwrap_or(0);
        if size > 1_000_000 {
            return Ok(Json(serde_json::json!({
                "status": "ok",
                "skipped": true,
                "message": "model already installed",
                "model_id": model_id,
                "model_size_mb": size / 1_048_576,
                "model_dir": model_dir.to_string_lossy(),
            })));
        }
    }

    tokio::fs::create_dir_all(&model_dir).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create dir failed: {e}")))?;

    // Construct HuggingFace download URLs
    let base_url = format!("https://huggingface.co/{}/resolve/main", model_id);
    let onnx_filename = if variant == "full" || variant == "fp32" {
        "onnx/model.onnx".to_string()
    } else {
        format!("onnx/model_{variant}.onnx")
    };
    let model_url = format!("{}/{}", base_url, onnx_filename);
    let tokenizer_url = format!("{}/tokenizer.json", base_url);

    let client = reqwest::Client::new();

    // Download tokenizer first (small, ~16 MB)
    tracing::info!(model = %model_id, "downloading tokenizer.json");
    let resp = client.get(&tokenizer_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY,
            format!("tokenizer download returned {} for {}", resp.status(), tokenizer_url)));
    }
    let tokenizer_bytes = resp.bytes().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer read failed: {e}")))?;
    tokio::fs::write(&tokenizer_path, &tokenizer_bytes).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write tokenizer failed: {e}")))?;

    // Download ONNX model (large, stream to disk)
    tracing::info!(model = %model_id, variant = %variant, "downloading ONNX model");
    let resp = client.get(&model_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("model download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY,
            format!("model download returned {} for {}", resp.status(), model_url)));
    }

    // Stream response body to file to avoid holding entire model in memory
    let mut file = tokio::fs::File::create(&model_path).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create model file failed: {e}")))?;
    let mut stream = resp.bytes_stream();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("model download stream error: {e}")))?;
        file.write_all(&chunk).await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write model failed: {e}")))?;
    }
    file.flush().await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("flush model failed: {e}")))?;

    let size = tokio::fs::metadata(&model_path).await.map(|m| m.len()).unwrap_or(0);
    tracing::info!(
        model = %model_id,
        variant = %variant,
        size_mb = size / 1_048_576,
        dir = %model_dir.display(),
        "NER ONNX model downloaded"
    );

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_id": model_id,
        "variant": variant,
        "model_size_mb": size / 1_048_576,
        "model_dir": model_dir.to_string_lossy(),
    })))
}

// ── POST /config/ner-download-onnx -- Download legacy ONNX NER model ──

/// Legacy endpoint: download ONNX-format NER model files from HuggingFace URLs.
pub async fn download_ner_model_onnx(
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let model_id = body["model_id"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_id required"))?;
    let model_url = body["model_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_url required"))?;
    let tokenizer_url = body["tokenizer_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "tokenizer_url required"))?;

    // Validate URLs point to huggingface.co
    for url in [model_url, tokenizer_url] {
        if !url.starts_with("https://huggingface.co/") {
            return Err(api_err(StatusCode::BAD_REQUEST, "only HuggingFace URLs are allowed"));
        }
    }

    // Sanitize model_id (alphanumeric, hyphens, underscores, dots only)
    if model_id.contains("..") || model_id.contains('/') || model_id.contains('\\') {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid model_id"));
    }

    // Target: ~/.engram/models/ner/{model_id}/
    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("ner").join(model_id);
    tokio::fs::create_dir_all(&model_dir).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create dir failed: {e}")))?;

    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    // Skip download if files already exist and model is >1MB (not a stub)
    let force = body["force"].as_bool().unwrap_or(false);
    if !force && model_path.exists() && tokenizer_path.exists() {
        let size = tokio::fs::metadata(&model_path).await.map(|m| m.len()).unwrap_or(0);
        if size > 1_000_000 {
            tracing::info!("NER ONNX model already installed ({} MB), skipping download", size / 1_048_576);
            return Ok(Json(serde_json::json!({
                "status": "ok",
                "skipped": true,
                "message": "model already installed",
                "model_id": model_id,
                "model_size_mb": size / 1_048_576,
            })));
        }
    }

    let client = reqwest::Client::new();

    // Download tokenizer first (small)
    tracing::info!("NER: downloading tokenizer from {}", tokenizer_url);
    let resp = client.get(tokenizer_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download returned {}", resp.status())));
    }
    let tokenizer_bytes = resp.bytes().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer read failed: {e}")))?;
    tokio::fs::write(&tokenizer_path, &tokenizer_bytes).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write tokenizer failed: {e}")))?;

    // Download model (large, stream to disk)
    tracing::info!("NER: downloading model from {}", model_url);
    let resp = client.get(model_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("model download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("model download returned {}", resp.status())));
    }
    let content_length = resp.content_length().unwrap_or(0);

    let mut file = tokio::fs::File::create(&model_path).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create model file failed: {e}")))?;

    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("download stream error: {e}")))?;
        file.write_all(&chunk).await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write model failed: {e}")))?;
        downloaded += chunk.len() as u64;
    }
    file.flush().await.map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("flush failed: {e}")))?;

    tracing::info!("NER: downloaded {} MB model to {}", downloaded / 1_048_576, model_dir.display());

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_id": model_id,
        "model_dir": model_dir.to_string_lossy(),
        "model_size_mb": downloaded as f64 / 1_048_576.0,
        "content_length_mb": content_length as f64 / 1_048_576.0,
    })))
}

/// GET /config/ner-model?id={model_id} -- Check if a NER model is installed.
pub async fn check_ner_model(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<serde_json::Value> {
    let model_id = params.get("id")
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "id query parameter required"))?;

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("ner").join(model_id);

    let has_model = model_dir.join("model.onnx").exists();
    let has_tokenizer = model_dir.join("tokenizer.json").exists();
    let ready = has_model && has_tokenizer;

    let model_size = if has_model {
        std::fs::metadata(model_dir.join("model.onnx"))
            .map(|m| m.len() as f64 / 1_048_576.0)
            .unwrap_or(0.0)
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "model_id": model_id,
        "ready": ready,
        "has_model": has_model,
        "has_tokenizer": has_tokenizer,
        "model_size_mb": model_size,
        "model_dir": model_dir.to_string_lossy(),
    })))
}

// ── POST /config/rel-download -- Download GLiREL model from HuggingFace ──

pub async fn download_rel_model(
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let model_id = body["model_id"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_id required"))?;
    let model_url = body["model_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_url required"))?;
    let tokenizer_url = body["tokenizer_url"].as_str()
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "tokenizer_url required"))?;

    for url in [model_url, tokenizer_url] {
        if !url.starts_with("https://huggingface.co/") {
            return Err(api_err(StatusCode::BAD_REQUEST, "only HuggingFace URLs are allowed"));
        }
    }

    if model_id.contains("..") || model_id.contains('/') || model_id.contains('\\') {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid model_id"));
    }

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("rel").join(model_id);
    tokio::fs::create_dir_all(&model_dir).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create dir failed: {e}")))?;

    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    // Skip download if files already exist and model is >1MB (not a stub)
    let force = body["force"].as_bool().unwrap_or(false);
    if !force && model_path.exists() && tokenizer_path.exists() {
        let size = tokio::fs::metadata(&model_path).await.map(|m| m.len()).unwrap_or(0);
        if size > 1_000_000 {
            tracing::info!("REL model already installed ({} MB), skipping download", size / 1_048_576);
            return Ok(Json(serde_json::json!({
                "status": "ok",
                "skipped": true,
                "message": "model already installed",
                "model_id": model_id,
                "model_size_mb": size / 1_048_576,
            })));
        }
    }

    let client = reqwest::Client::new();

    tracing::info!("REL: downloading tokenizer from {}", tokenizer_url);
    let resp = client.get(tokenizer_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("tokenizer download returned {}", resp.status())));
    }
    let tokenizer_bytes = resp.bytes().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("tokenizer read failed: {e}")))?;
    tokio::fs::write(&tokenizer_path, &tokenizer_bytes).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write tokenizer failed: {e}")))?;

    tracing::info!("REL: downloading model from {}", model_url);
    let resp = client.get(model_url).send().await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("model download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(api_err(StatusCode::BAD_GATEWAY, format!("model download returned {}", resp.status())));
    }
    let content_length = resp.content_length().unwrap_or(0);

    let mut file = tokio::fs::File::create(&model_path).await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create model file failed: {e}")))?;

    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("download stream error: {e}")))?;
        file.write_all(&chunk).await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write model failed: {e}")))?;
        downloaded += chunk.len() as u64;
    }
    file.flush().await.map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("flush failed: {e}")))?;

    tracing::info!("REL: downloaded {} MB model to {}", downloaded / 1_048_576, model_dir.display());

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_id": model_id,
        "model_dir": model_dir.to_string_lossy(),
        "model_size_mb": downloaded as f64 / 1_048_576.0,
        "content_length_mb": content_length as f64 / 1_048_576.0,
    })))
}

/// GET /config/rel-model?id={model_id} -- Check if a GLiREL model is installed.
pub async fn check_rel_model(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<serde_json::Value> {
    let model_id = params.get("id")
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "id query parameter required"))?;

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;
    let model_dir = home.join("models").join("rel").join(model_id);

    let has_model = model_dir.join("model.onnx").exists();
    let has_tokenizer = model_dir.join("tokenizer.json").exists();
    let ready = has_model && has_tokenizer;

    let model_size = if has_model {
        std::fs::metadata(model_dir.join("model.onnx"))
            .map(|m| m.len() as f64 / 1_048_576.0)
            .unwrap_or(0.0)
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "model_id": model_id,
        "ready": ready,
        "has_model": has_model,
        "has_tokenizer": has_tokenizer,
        "model_size_mb": model_size,
        "model_dir": model_dir.to_string_lossy(),
    })))
}

// ── POST /config/model-upload -- Upload model files for air-gapped systems ──

/// Upload model files via multipart/form-data for air-gapped systems.
///
/// Fields:
/// - `model_type`: "embed" | "ner" | "rel"
/// - `model_id`: directory name (e.g. "multilingual-MiniLMv2-L6-mnli-xnli")
/// - File fields: saved to `~/.engram/models/{type}/{id}/{filename}`
pub async fn upload_model(
    mut multipart: axum::extract::Multipart,
) -> ApiResult<serde_json::Value> {
    use tokio::io::AsyncWriteExt;

    let home = engram_home()
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "cannot determine home directory"))?;

    let mut model_type: Option<String> = None;
    let mut model_id: Option<String> = None;
    let mut files_written: Vec<String> = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut model_dir: Option<std::path::PathBuf> = None;

    while let Some(field) = multipart.next_field().await
        .map_err(|e| api_err(StatusCode::BAD_REQUEST, format!("multipart read error: {e}")))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "model_type" => {
                let val = field.text().await
                    .map_err(|e| api_err(StatusCode::BAD_REQUEST, format!("read model_type: {e}")))?;
                if !matches!(val.as_str(), "embed" | "ner" | "rel") {
                    return Err(api_err(StatusCode::BAD_REQUEST, "model_type must be 'embed', 'ner', or 'rel'"));
                }
                model_type = Some(val);
            }
            "model_id" => {
                let val = field.text().await
                    .map_err(|e| api_err(StatusCode::BAD_REQUEST, format!("read model_id: {e}")))?;
                if val.contains("..") || val.contains('/') || val.contains('\\') {
                    return Err(api_err(StatusCode::BAD_REQUEST, "invalid model_id (no path traversal)"));
                }
                model_id = Some(val);
            }
            _ => {
                // File field -- save to model directory
                let mt = model_type.as_ref()
                    .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_type must be sent before file fields"))?;
                let mid = model_id.as_ref()
                    .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "model_id must be sent before file fields"))?;

                let dir = home.join("models").join(mt).join(mid);
                if model_dir.is_none() {
                    tokio::fs::create_dir_all(&dir).await
                        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create dir: {e}")))?;
                    model_dir = Some(dir.clone());
                }

                let filename = field.file_name().unwrap_or(&field_name).to_string();
                if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
                    return Err(api_err(StatusCode::BAD_REQUEST, format!("invalid filename: {filename}")));
                }

                let file_path = dir.join(&filename);
                let data = field.bytes().await
                    .map_err(|e| api_err(StatusCode::BAD_REQUEST, format!("read file {filename}: {e}")))?;

                let mut f = tokio::fs::File::create(&file_path).await
                    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("create {filename}: {e}")))?;
                f.write_all(&data).await
                    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("write {filename}: {e}")))?;
                f.flush().await
                    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("flush {filename}: {e}")))?;

                total_bytes += data.len() as u64;
                files_written.push(filename);
                tracing::info!(file = %file_path.display(), bytes = data.len(), "model file uploaded");
            }
        }
    }

    if files_written.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "no files uploaded"));
    }

    Ok(Json(serde_json::json!({
        "status": "ok",
        "model_type": model_type,
        "model_id": model_id,
        "files": files_written,
        "total_bytes": total_bytes,
        "model_dir": model_dir.map(|d| d.to_string_lossy().to_string()),
    })))
}
