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
            endpoint: endpoint.trim_end_matches('/').to_string(),
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
    pub fn probe_dimension(&self) -> Result<usize, EmbedError> {
        let body = format!(
            r#"{{"model":"{}","input":"dimension probe"}}"#,
            escape_json(&self.model)
        );
        let url = format!("{}/embeddings", self.endpoint);
        let response_body = http_post(&url, &body, self.api_key.as_deref())
            .map_err(EmbedError::RuntimeError)?;

        // Parse just the first embedding array to get its length
        let embed_key = response_body
            .find("\"embedding\"")
            .ok_or_else(|| EmbedError::RuntimeError("probe: missing 'embedding' field".into()))?;
        let arr_open = response_body[embed_key..]
            .find('[')
            .map(|p| embed_key + p)
            .ok_or_else(|| EmbedError::RuntimeError("probe: missing array".into()))?;
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

        let url = format!("{}/embeddings", self.endpoint);

        // Use ureq-style minimal HTTP via std::net -- but that's complex.
        // Instead, use a blocking TCP + HTTP/1.1 approach via std.
        // For robustness, we use a simple HTTP client built on std::net.
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

/// Minimal blocking HTTP POST using std::net::TcpStream.
/// Supports http:// URLs. For https, we'd need rustls/native-tls --
/// but most local embedding servers (Ollama, vLLM, TEI) use plain HTTP.
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
        return Err(
            "HTTPS not supported in built-in HTTP client. Use http:// for local embedding servers, or enable the 'embed-reqwest' feature.".into()
        );
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
    // Simple JSON parsing without serde dependency in engram-core.
    // We parse the "data" array and extract "embedding" arrays.

    // Find "data" array
    let data_start = body
        .find("\"data\"")
        .ok_or_else(|| EmbedError::RuntimeError("missing 'data' field in response".into()))?;

    let after_data = &body[data_start + 6..];
    let _arr_start = after_data
        .find('[')
        .ok_or_else(|| EmbedError::RuntimeError("missing data array".into()))?;

    // We need to extract embedding arrays. Strategy:
    // Find each "embedding": [...] block.
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
}
