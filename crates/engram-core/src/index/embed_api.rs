/// External API embedder -- calls an OpenAI-compatible /v1/embeddings endpoint.
///
/// Compatible with: OpenAI, Ollama, vLLM, LiteLLM, text-embeddings-inference, etc.
///
/// Configuration via environment:
///   ENGRAM_EMBED_ENDPOINT  -- base URL (e.g. "http://localhost:11434/v1")
///   ENGRAM_EMBED_MODEL     -- model name (e.g. "multilingual-e5-small")
///   ENGRAM_EMBED_DIM       -- expected dimension (default: 384)
///   ENGRAM_EMBED_API_KEY   -- optional API key

use super::embedding::{EmbedError, Embedder};
use std::io::Read;

pub struct ApiEmbedder {
    endpoint: String,
    model: String,
    dim: usize,
    api_key: Option<String>,
}

impl ApiEmbedder {
    /// Create from explicit parameters.
    pub fn new(endpoint: String, model: String, dim: usize, api_key: Option<String>) -> Self {
        Self {
            endpoint: normalize_endpoint(&endpoint),
            model,
            dim,
            api_key,
        }
    }

    /// Create from environment variables. Returns None if ENGRAM_EMBED_ENDPOINT is not set.
    ///
    /// If ENGRAM_EMBED_DIM is not set, sends a probe request to auto-detect the dimension.
    /// This handles Matryoshka models (like nomic-embed-text-v2-moe) that support multiple dimensions.
    pub fn from_env() -> Option<Self> {
        let endpoint = std::env::var("ENGRAM_EMBED_ENDPOINT").ok()?;
        let model =
            std::env::var("ENGRAM_EMBED_MODEL").unwrap_or_else(|_| "multilingual-e5-small".into());
        let api_key = std::env::var("ENGRAM_EMBED_API_KEY").ok();

        // If ENGRAM_EMBED_DIM is explicitly set, use it. Otherwise, auto-detect.
        let dim: usize = match std::env::var("ENGRAM_EMBED_DIM").ok().and_then(|s| s.parse().ok()) {
            Some(d) => d,
            None => {
                // Auto-detect: send a probe embedding and measure the dimension
                let embedder = Self::new(endpoint.clone(), model.clone(), 0, api_key.clone());
                match embedder.probe_dimension() {
                    Ok(d) => d,
                    Err(_) => 384, // safe fallback
                }
            }
        };

        Some(Self::new(endpoint, model, dim, api_key))
    }

    /// Send a probe embedding to detect the model's output dimension.
    ///
    /// For Ollama native endpoints (`/api`), uses `/api/embed` which returns
    /// `{"embeddings": [[...]]}`. For all others, uses the OpenAI-compatible
    /// `/embeddings` path which returns `{"data":[{"embedding":[...]}]}`.
    pub fn probe_dimension(&self) -> Result<usize, EmbedError> {
        let body = format!(
            r#"{{"model":"{}","input":"dimension probe"}}"#,
            escape_json(&self.model)
        );

        // Ollama native API uses /api/embed (not /api/embeddings which returns empty)
        let url = if self.endpoint.ends_with("/api") {
            format!("{}/embed", self.endpoint)
        } else {
            format!("{}/embeddings", self.endpoint)
        };

        let response_body = http_post(&url, &body, self.api_key.as_deref())
            .map_err(EmbedError::RuntimeError)?;

        parse_probe_response(&response_body)
    }

}

/// Parse a probe response from either OpenAI or Ollama format to extract dimension.
///
/// OpenAI format: `{"data":[{"embedding":[0.1, 0.2, ...]}]}`
/// Ollama format: `{"embeddings":[[0.1, 0.2, ...]]}`
///
/// Both contain `"embedding"` as a substring, so we find the innermost `[...]`
/// array containing actual float values.
fn parse_probe_response(response_body: &str) -> Result<usize, EmbedError> {
    // Find "embedding" (matches both "embedding" and "embeddings")
    let embed_key = response_body
        .find("\"embedding")
        .ok_or_else(|| EmbedError::RuntimeError("probe: missing 'embedding' field".into()))?;

    // Find the first '[' after the key
    let first_open = response_body[embed_key..]
        .find('[')
        .map(|p| embed_key + p)
        .ok_or_else(|| EmbedError::RuntimeError("probe: missing array".into()))?;

    // Check if this is a nested array (Ollama: [[...]]) by looking at next non-whitespace
    let after_open = &response_body[first_open + 1..];
    let inner_start = after_open.find(|c: char| !c.is_whitespace()).unwrap_or(0);
    let arr_open = if after_open.as_bytes().get(inner_start) == Some(&b'[') {
        // Nested array -- skip to inner array
        first_open + 1 + inner_start
    } else {
        // Flat array -- use as-is
        first_open
    };

    let arr_close = response_body[arr_open..]
        .find(']')
        .map(|p| arr_open + p)
        .ok_or_else(|| EmbedError::RuntimeError("probe: unclosed array".into()))?;

    let arr_str = &response_body[arr_open + 1..arr_close];
    let count = arr_str.split(',').filter(|s| !s.trim().is_empty()).count();
    if count == 0 {
        return Err(EmbedError::RuntimeError("probe: empty embedding".into()));
    }
    Ok(count)
}

impl ApiEmbedder {
    fn call_api(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        // Build request body: OpenAI-compatible format
        let body = if texts.len() == 1 {
            format!(
                r#"{{"model":"{}","input":"{}"}}"#,
                escape_json(&self.model),
                escape_json(texts[0])
            )
        } else {
            let inputs: Vec<String> = texts
                .iter()
                .map(|t| format!("\"{}\"", escape_json(t)))
                .collect();
            format!(
                r#"{{"model":"{}","input":[{}]}}"#,
                escape_json(&self.model),
                inputs.join(",")
            )
        };

        // Ollama native API uses /api/embed, others use /embeddings
        let url = if self.endpoint.ends_with("/api") {
            format!("{}/embed", self.endpoint)
        } else {
            format!("{}/embeddings", self.endpoint)
        };
        let response_body = http_post(&url, &body, self.api_key.as_deref())
            .map_err(|e| EmbedError::RuntimeError(e))?;

        // Parse response: { "data": [ { "embedding": [...], "index": 0 }, ... ] }
        parse_embeddings_response(&response_body, texts.len(), self.dim)
    }
}

impl Embedder for ApiEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        let results = self.call_api(&[text])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbedError::RuntimeError("empty response from API".into()))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        // Batch in chunks of 32 to avoid overwhelming the API
        let mut all_results = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(32) {
            let mut results = self.call_api(chunk)?;
            all_results.append(&mut results);
        }
        Ok(all_results)
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}

/// Normalize an embedding endpoint URL so that appending `/embeddings` produces
/// a valid API path for any common provider.
///
/// All major embedding servers expose an OpenAI-compatible endpoint at `/v1/embeddings`.
/// Users may enter just the base URL (e.g. `http://localhost:11434`) without the `/v1`
/// path prefix. This function detects that and adds `/v1` automatically.
///
/// Rules:
///   1. Strip trailing slashes.
///   2. If the path already ends with `/embeddings` or `/embed`, use as-is (user gave full path).
///   3. If the path ends with a recognized API prefix (`/v1`, `/api`), keep it.
///   4. Otherwise (bare host like `http://localhost:11434`), append `/v1`.
///
/// Result: caller can always do `format!("{}/embeddings", normalized)` and get a valid URL.
///
/// Examples:
///   `http://localhost:11434`          -> `http://localhost:11434/v1`       (Ollama, LM Studio)
///   `http://localhost:11434/v1`       -> `http://localhost:11434/v1`       (already correct)
///   `http://localhost:11434/api`      -> `http://localhost:11434/api`      (Ollama native)
///   `https://api.openai.com/v1`      -> `https://api.openai.com/v1`      (OpenAI)
///   `http://localhost:8000/v1`        -> `http://localhost:8000/v1`       (vLLM)
///   `http://localhost:1234/v1`        -> `http://localhost:1234/v1`       (LM Studio)
///   `http://host:8080/v1/embeddings` -> `http://host:8080/v1`            (user gave full path)
fn normalize_endpoint(raw: &str) -> String {
    let s = raw.trim().trim_end_matches('/');

    // If user pasted the full embeddings URL, strip the tail so we don't double it
    if let Some(base) = s.strip_suffix("/embeddings") {
        return base.to_string();
    }
    if let Some(base) = s.strip_suffix("/embed") {
        return base.to_string();
    }

    // If the path already ends with a known API prefix, keep it
    if s.ends_with("/v1") || s.ends_with("/api") || s.ends_with("/v2") {
        return s.to_string();
    }

    // Bare host+port (no meaningful path) -- add /v1 (OpenAI-compatible standard)
    // Detect by checking the path component after the authority
    let after_scheme = if let Some(rest) = s.strip_prefix("https://") {
        rest
    } else if let Some(rest) = s.strip_prefix("http://") {
        rest
    } else {
        // Unknown scheme, return as-is
        return s.to_string();
    };

    let path = match after_scheme.find('/') {
        Some(i) => &after_scheme[i..],
        None => "",
    };

    // Empty path or just "/" means bare host -- add /v1
    if path.is_empty() || path == "/" {
        return format!("{}/v1", s);
    }

    // Has some other path we don't recognize -- keep as-is, user knows best
    s.to_string()
}

/// Minimal JSON string escape.
fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// HTTPS POST using reqwest blocking client. Enabled with the `https` feature.
/// Supports cloud embedding providers (OpenAI, Cohere, Voyage, Jina, etc.).
#[cfg(feature = "https")]
fn https_post(url: &str, body: &str, api_key: Option<&str>) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTPS client init: {e}"))?;

    let mut req = client
        .post(url)
        .header("Content-Type", "application/json")
        .body(body.to_owned());

    if let Some(key) = api_key {
        req = req.header("Authorization", format!("Bearer {key}"));
    }

    let resp = req.send().map_err(|e| format!("HTTPS request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().map_err(|e| format!("HTTPS read body: {e}"))?;

    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status.as_u16(), text));
    }
    Ok(text)
}

/// Minimal blocking HTTP POST using std::net::TcpStream.
/// Supports http:// URLs. Cloud providers (HTTPS) use reqwest via the `https` feature.
fn http_post(url: &str, body: &str, api_key: Option<&str>) -> Result<String, String> {
    let url = url.trim();

    // Parse URL
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        ("http", rest)
    } else {
        return Err(format!("unsupported URL scheme: {url}"));
    };

    if scheme == "https" {
        #[cfg(feature = "https")]
        {
            return https_post(url, body, api_key);
        }
        #[cfg(not(feature = "https"))]
        {
            return Err(
                "HTTPS not supported. Build with --features https for cloud embedding providers (OpenAI, Cohere, etc.).".into()
            );
        }
    }

    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    let (host, port) = match host_port.rfind(':') {
        Some(i) => (&host_port[..i], host_port[i + 1..].parse::<u16>().unwrap_or(80)),
        None => (host_port, 80),
    };

    // Connect with timeout
    let addr = format!("{host}:{port}");
    let stream = std::net::TcpStream::connect_timeout(
        &addr
            .parse::<std::net::SocketAddr>()
            .or_else(|_| {
                // Try DNS resolution
                use std::net::ToSocketAddrs;
                addr.to_socket_addrs()
                    .map_err(|e| e.to_string())?
                    .next()
                    .ok_or_else(|| "DNS resolution failed".to_string())
            })
            .map_err(|e| format!("connect failed: {e}"))?,
        std::time::Duration::from_secs(10),
    )
    .map_err(|e| format!("connect failed: {e}"))?;

    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok();
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(10)))
        .ok();

    // Build HTTP request
    let mut request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(key) = api_key {
        request.push_str(&format!("Authorization: Bearer {key}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);

    // Send
    use std::io::Write;
    let mut stream = std::io::BufWriter::new(stream);
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("write failed: {e}"))?;
    stream.flush().map_err(|e| format!("flush failed: {e}"))?;

    // Read response
    let mut stream = stream.into_inner().map_err(|e| format!("unwrap failed: {e}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| format!("read failed: {e}"))?;

    let response_str =
        String::from_utf8(response).map_err(|_| "invalid UTF-8 in response".to_string())?;

    // Parse HTTP response -- find body after \r\n\r\n
    let body_start = response_str
        .find("\r\n\r\n")
        .map(|i| i + 4)
        .unwrap_or(0);

    // Check status line
    if let Some(status_line) = response_str.lines().next() {
        if !status_line.contains("200") {
            return Err(format!("API error: {status_line}"));
        }
    }

    // Handle chunked transfer encoding
    let headers_section = &response_str[..body_start];
    let body_str = &response_str[body_start..];

    if headers_section
        .to_lowercase()
        .contains("transfer-encoding: chunked")
    {
        // Decode chunked encoding
        Ok(decode_chunked(body_str))
    } else {
        Ok(body_str.to_string())
    }
}

/// Simple chunked transfer encoding decoder.
fn decode_chunked(input: &str) -> String {
    let mut result = String::new();
    let mut remaining = input;

    loop {
        // Skip leading whitespace/newlines
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        // Read chunk size (hex)
        let size_end = remaining
            .find("\r\n")
            .unwrap_or(remaining.len());
        let size_str = &remaining[..size_end];
        let chunk_size =
            usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);

        if chunk_size == 0 {
            break;
        }

        // Skip past the size line
        remaining = &remaining[size_end + 2..];

        // Read chunk data
        let end = chunk_size.min(remaining.len());
        result.push_str(&remaining[..end]);
        remaining = &remaining[end..];

        // Skip trailing \r\n
        if remaining.starts_with("\r\n") {
            remaining = &remaining[2..];
        }
    }

    result
}

/// Parse OpenAI-compatible embeddings response.
fn parse_embeddings_response(
    body: &str,
    expected_count: usize,
    expected_dim: usize,
) -> Result<Vec<Vec<f32>>, EmbedError> {
    // Supports two response formats:
    // OpenAI: {"data": [{"embedding": [0.1, ...], "index": 0}, ...]}
    // Ollama: {"embeddings": [[0.1, ...], ...]}

    if let Some(data_start) = body.find("\"data\"") {
        // OpenAI format
        parse_openai_embeddings(body, data_start, expected_count, expected_dim)
    } else if let Some(embeds_start) = body.find("\"embeddings\"") {
        // Ollama format: {"embeddings": [[...], [...]]}
        parse_ollama_embeddings(body, embeds_start, expected_count, expected_dim)
    } else {
        Err(EmbedError::RuntimeError("missing 'data' or 'embeddings' field in response".into()))
    }
}

fn parse_openai_embeddings(
    body: &str,
    data_start: usize,
    expected_count: usize,
    expected_dim: usize,
) -> Result<Vec<Vec<f32>>, EmbedError> {
    let mut embeddings = Vec::with_capacity(expected_count);
    let mut search_from = data_start;

    for _ in 0..expected_count {
        let embed_key = match body[search_from..].find("\"embedding\"") {
            Some(pos) => search_from + pos,
            None => break,
        };

        let arr_open = match body[embed_key..].find('[') {
            Some(pos) => embed_key + pos,
            None => break,
        };

        let arr_close = match body[arr_open..].find(']') {
            Some(pos) => arr_open + pos,
            None => break,
        };

        let arr_str = &body[arr_open + 1..arr_close];
        let values: Vec<f32> = arr_str
            .split(',')
            .filter_map(|s| s.trim().parse::<f32>().ok())
            .collect();

        if expected_dim > 0 && values.len() != expected_dim {
            return Err(EmbedError::DimensionMismatch {
                expected: expected_dim,
                got: values.len(),
            });
        }

        embeddings.push(values);
        search_from = arr_close + 1;
    }

    if embeddings.len() != expected_count {
        return Err(EmbedError::RuntimeError(format!(
            "expected {} embeddings, got {}",
            expected_count,
            embeddings.len()
        )));
    }

    Ok(embeddings)
}

fn parse_ollama_embeddings(
    body: &str,
    embeds_start: usize,
    expected_count: usize,
    expected_dim: usize,
) -> Result<Vec<Vec<f32>>, EmbedError> {
    // Find the outer array: "embeddings": [[...], [...]]
    let outer_open = body[embeds_start..]
        .find('[')
        .map(|p| embeds_start + p)
        .ok_or_else(|| EmbedError::RuntimeError("missing embeddings array".into()))?;

    let mut embeddings = Vec::with_capacity(expected_count);
    let mut pos = outer_open + 1;

    for _ in 0..expected_count {
        // Find next inner array [...]
        let inner_open = match body[pos..].find('[') {
            Some(p) => pos + p,
            None => break,
        };
        let inner_close = match body[inner_open..].find(']') {
            Some(p) => inner_open + p,
            None => break,
        };

        let arr_str = &body[inner_open + 1..inner_close];
        let values: Vec<f32> = arr_str
            .split(',')
            .filter_map(|s| s.trim().parse::<f32>().ok())
            .collect();

        if expected_dim > 0 && values.len() != expected_dim {
            return Err(EmbedError::DimensionMismatch {
                expected: expected_dim,
                got: values.len(),
            });
        }

        embeddings.push(values);
        pos = inner_close + 1;
    }

    if embeddings.len() != expected_count {
        return Err(EmbedError::RuntimeError(format!(
            "expected {} embeddings, got {}",
            expected_count,
            embeddings.len()
        )));
    }

    Ok(embeddings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_embedding() {
        let json = r#"{"object":"list","data":[{"object":"embedding","embedding":[0.1,0.2,0.3,0.4],"index":0}],"model":"test","usage":{"prompt_tokens":5,"total_tokens":5}}"#;
        let result = parse_embeddings_response(json, 1, 4).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn parse_batch_embeddings() {
        let json = r#"{"data":[{"embedding":[1.0,2.0],"index":0},{"embedding":[3.0,4.0],"index":1}]}"#;
        let result = parse_embeddings_response(json, 2, 2).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], vec![1.0, 2.0]);
        assert_eq!(result[1], vec![3.0, 4.0]);
    }

    #[test]
    fn parse_dimension_mismatch() {
        let json = r#"{"data":[{"embedding":[0.1,0.2,0.3],"index":0}]}"#;
        let result = parse_embeddings_response(json, 1, 4);
        assert!(matches!(result, Err(EmbedError::DimensionMismatch { .. })));
    }

    #[test]
    fn escape_json_special_chars() {
        assert_eq!(escape_json("hello \"world\""), r#"hello \"world\""#);
        assert_eq!(escape_json("line\nnewline"), r#"line\nnewline"#);
    }

    #[test]
    fn chunked_decode() {
        let input = "4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n";
        assert_eq!(decode_chunked(input), "Wikipedia");
    }

    #[test]
    fn api_embedder_from_env_missing() {
        // Without ENGRAM_EMBED_ENDPOINT, should return None
        // SAFETY: test environment, single-threaded access to env var
        unsafe { std::env::remove_var("ENGRAM_EMBED_ENDPOINT"); }
        assert!(ApiEmbedder::from_env().is_none());
    }

    #[test]
    fn normalize_endpoint_bare_host() {
        // Bare host should get /v1 appended
        assert_eq!(normalize_endpoint("http://localhost:11434"), "http://localhost:11434/v1");
        assert_eq!(normalize_endpoint("http://localhost:8000"), "http://localhost:8000/v1");
        assert_eq!(normalize_endpoint("http://localhost:1234"), "http://localhost:1234/v1");
    }

    #[test]
    fn normalize_endpoint_with_v1() {
        // Already has /v1 -- keep as-is
        assert_eq!(normalize_endpoint("http://localhost:11434/v1"), "http://localhost:11434/v1");
        assert_eq!(normalize_endpoint("https://api.openai.com/v1"), "https://api.openai.com/v1");
        assert_eq!(normalize_endpoint("http://localhost:8000/v1"), "http://localhost:8000/v1");
    }

    #[test]
    fn normalize_endpoint_with_api() {
        // Has /api prefix -- keep as-is (Ollama native)
        assert_eq!(normalize_endpoint("http://localhost:11434/api"), "http://localhost:11434/api");
    }

    #[test]
    fn normalize_endpoint_full_path() {
        // User pasted the full embeddings URL -- strip /embeddings
        assert_eq!(normalize_endpoint("http://localhost:11434/v1/embeddings"), "http://localhost:11434/v1");
        assert_eq!(normalize_endpoint("http://localhost:11434/api/embeddings"), "http://localhost:11434/api");
        assert_eq!(normalize_endpoint("http://host:8080/v1/embed"), "http://host:8080/v1");
    }

    #[test]
    fn normalize_endpoint_trailing_slash() {
        assert_eq!(normalize_endpoint("http://localhost:11434/"), "http://localhost:11434/v1");
        assert_eq!(normalize_endpoint("http://localhost:11434/v1/"), "http://localhost:11434/v1");
    }
}
