/// File source: reads files from a directory, with optional watch mode.
///
/// Format auto-detect based on extension:
/// - `.txt` -> plain text
/// - `.json` -> JSON structured data (key-value maps)
/// - `.csv` -> CSV rows as structured data
/// - `.ndjson` / `.jsonl` -> newline-delimited JSON
///
/// Watch mode uses the `notify` crate when the `file-source` feature is
/// enabled. Falls back to poll-based watching when notify is unavailable.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::IngestError;
use crate::traits::{CostModel, Source, SourceCapabilities, SourceParams};
use crate::types::{Content, RawItem};

/// File source configuration.
#[derive(Debug, Clone)]
pub struct FileSourceConfig {
    /// Root directory to scan.
    pub root: PathBuf,
    /// File extensions to include (empty = all text-like files).
    pub extensions: Vec<String>,
    /// Whether to recurse into subdirectories.
    pub recursive: bool,
    /// Source name for provenance.
    pub name: String,
}

impl Default for FileSourceConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            extensions: vec![
                "txt".into(), "json".into(), "csv".into(),
                "ndjson".into(), "jsonl".into(), "md".into(),
                "pdf".into(),
            ],
            recursive: true,
            name: "file".into(),
        }
    }
}

/// File-based source that reads from a directory.
pub struct FileSource {
    config: FileSourceConfig,
}

impl FileSource {
    pub fn new(config: FileSourceConfig) -> Self {
        Self { config }
    }

    /// Scan directory and return raw items.
    pub fn scan(&self) -> Result<Vec<RawItem>, IngestError> {
        let mut items = Vec::new();
        self.scan_dir(&self.config.root, &mut items)?;
        Ok(items)
    }

    fn scan_dir(&self, dir: &Path, items: &mut Vec<RawItem>) -> Result<(), IngestError> {
        let entries = std::fs::read_dir(dir)
            .map_err(|e| IngestError::Io(e))?;

        for entry in entries {
            let entry = entry.map_err(|e| IngestError::Io(e))?;
            let path = entry.path();

            if path.is_dir() {
                if self.config.recursive {
                    self.scan_dir(&path, items)?;
                }
                continue;
            }

            if !self.matches_extension(&path) {
                continue;
            }

            match self.read_file(&path) {
                Ok(mut file_items) => items.append(&mut file_items),
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping file");
                }
            }
        }

        Ok(())
    }

    fn matches_extension(&self, path: &Path) -> bool {
        if self.config.extensions.is_empty() {
            return true;
        }
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                self.config.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
            })
    }

    /// Read a single file and convert to RawItems based on format.
    pub fn read_file(&self, path: &Path) -> Result<Vec<RawItem>, IngestError> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let modified = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let source_url = path.to_string_lossy().to_string();

        match ext.as_str() {
            "json" => self.read_json(path, &source_url, modified),
            "ndjson" | "jsonl" => self.read_ndjson(path, &source_url, modified),
            "csv" => self.read_csv(path, &source_url, modified),
            "pdf" => self.read_pdf(path, &source_url, modified),
            _ => self.read_text(path, &source_url, modified),
        }
    }

    fn read_text(&self, path: &Path, source_url: &str, fetched_at: i64) -> Result<Vec<RawItem>, IngestError> {
        let text = std::fs::read_to_string(path).map_err(IngestError::Io)?;
        if text.trim().is_empty() {
            return Ok(vec![]);
        }
        Ok(vec![RawItem {
            content: Content::Text(text),
            source_url: Some(source_url.to_string()),
            source_name: self.config.name.clone(),
            fetched_at,
            metadata: HashMap::from([
                ("file".into(), path.file_name().unwrap_or_default().to_string_lossy().to_string()),
            ]),
        }])
    }

    fn read_json(&self, path: &Path, source_url: &str, fetched_at: i64) -> Result<Vec<RawItem>, IngestError> {
        let text = std::fs::read_to_string(path).map_err(IngestError::Io)?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| IngestError::Parse(format!("JSON parse error: {}", e)))?;

        match value {
            serde_json::Value::Array(arr) => {
                Ok(arr.into_iter().filter_map(|v| self.json_to_item(v, source_url, fetched_at)).collect())
            }
            serde_json::Value::Object(_) => {
                Ok(self.json_to_item(value, source_url, fetched_at).into_iter().collect())
            }
            _ => {
                Ok(vec![RawItem {
                    content: Content::Text(text),
                    source_url: Some(source_url.to_string()),
                    source_name: self.config.name.clone(),
                    fetched_at,
                    metadata: Default::default(),
                }])
            }
        }
    }

    fn read_ndjson(&self, path: &Path, source_url: &str, fetched_at: i64) -> Result<Vec<RawItem>, IngestError> {
        let text = std::fs::read_to_string(path).map_err(IngestError::Io)?;
        let items: Vec<RawItem> = text.lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| {
                serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .and_then(|v| self.json_to_item(v, source_url, fetched_at))
            })
            .collect();
        Ok(items)
    }

    fn read_csv(&self, path: &Path, source_url: &str, fetched_at: i64) -> Result<Vec<RawItem>, IngestError> {
        let text = std::fs::read_to_string(path).map_err(IngestError::Io)?;
        let mut lines = text.lines();

        let headers: Vec<&str> = match lines.next() {
            Some(header) => header.split(',').map(|s| s.trim()).collect(),
            None => return Ok(vec![]),
        };

        let items: Vec<RawItem> = lines
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let values: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                let mut map = HashMap::new();
                for (i, header) in headers.iter().enumerate() {
                    if let Some(val) = values.get(i) {
                        map.insert(header.to_string(), val.to_string());
                    }
                }
                RawItem {
                    content: Content::Structured(map),
                    source_url: Some(source_url.to_string()),
                    source_name: self.config.name.clone(),
                    fetched_at,
                    metadata: Default::default(),
                }
            })
            .collect();
        Ok(items)
    }

    /// Read a PDF file as raw bytes for pipeline processing.
    #[cfg(feature = "pdf")]
    fn read_pdf(&self, path: &Path, source_url: &str, fetched_at: i64) -> Result<Vec<RawItem>, IngestError> {
        let bytes = std::fs::read(path).map_err(IngestError::Io)?;
        Ok(vec![RawItem {
            content: Content::Bytes {
                data: bytes,
                mime: "application/pdf".into(),
            },
            source_url: Some(source_url.to_string()),
            source_name: self.config.name.clone(),
            fetched_at,
            metadata: HashMap::from([
                ("file".into(), path.file_name().unwrap_or_default().to_string_lossy().to_string()),
                ("file_path".into(), path.to_string_lossy().to_string()),
            ]),
        }])
    }

    /// Fallback when pdf feature is disabled.
    #[cfg(not(feature = "pdf"))]
    fn read_pdf(&self, path: &Path, _source_url: &str, _fetched_at: i64) -> Result<Vec<RawItem>, IngestError> {
        tracing::warn!(path = %path.display(), "PDF support not enabled, skipping");
        Ok(vec![])
    }

    fn json_to_item(&self, value: serde_json::Value, source_url: &str, fetched_at: i64) -> Option<RawItem> {
        match value {
            serde_json::Value::Object(map) => {
                let structured: HashMap<String, String> = map
                    .into_iter()
                    .map(|(k, v)| {
                        let val = match v {
                            serde_json::Value::String(s) => s,
                            other => other.to_string(),
                        };
                        (k, val)
                    })
                    .collect();
                Some(RawItem {
                    content: Content::Structured(structured),
                    source_url: Some(source_url.to_string()),
                    source_name: self.config.name.clone(),
                    fetched_at,
                    metadata: Default::default(),
                })
            }
            serde_json::Value::String(s) => {
                Some(RawItem {
                    content: Content::Text(s),
                    source_url: Some(source_url.to_string()),
                    source_name: self.config.name.clone(),
                    fetched_at,
                    metadata: Default::default(),
                })
            }
            _ => None,
        }
    }
}

impl Source for FileSource {
    fn fetch(
        &self,
        _params: &SourceParams,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<RawItem>, IngestError>> + Send + '_>> {
        Box::pin(async move { self.scan() })
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            temporal_cursor: false,
            searchable: false,
            streaming: cfg!(feature = "file-source"), // watch mode available when notify is compiled
            cost_model: CostModel::Free,
        }
    }
}

/// Poll-based file watcher fallback.
/// Checks for changes by comparing file modification timestamps.
pub struct PollWatcher {
    config: FileSourceConfig,
    /// Tracked file modification times.
    known_files: std::sync::Mutex<HashMap<PathBuf, i64>>,
}

impl PollWatcher {
    pub fn new(config: FileSourceConfig) -> Self {
        Self {
            config,
            known_files: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Check for new or modified files since last poll.
    pub fn poll(&self) -> Result<Vec<PathBuf>, IngestError> {
        let source = FileSource::new(self.config.clone());
        let mut changed = Vec::new();
        let mut known = self.known_files.lock().unwrap();

        self.poll_dir(&source, &self.config.root, &mut changed, &mut known)?;
        Ok(changed)
    }

    fn poll_dir(
        &self,
        source: &FileSource,
        dir: &Path,
        changed: &mut Vec<PathBuf>,
        known: &mut HashMap<PathBuf, i64>,
    ) -> Result<(), IngestError> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && self.config.recursive {
                self.poll_dir(source, &path, changed, known)?;
                continue;
            }

            if !source.matches_extension(&path) {
                continue;
            }

            let mtime = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let is_new_or_changed = known
                .get(&path)
                .map(|&old_mtime| mtime > old_mtime)
                .unwrap_or(true);

            if is_new_or_changed {
                known.insert(path.clone(), mtime);
                changed.push(path);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir_with_files() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().to_path_buf();

        // Create test files
        std::fs::write(root.join("hello.txt"), "Hello World").unwrap();
        std::fs::write(
            root.join("data.json"),
            r#"[{"entity": "Apple", "type": "ORG"}, {"entity": "Tim Cook", "type": "PERSON"}]"#,
        ).unwrap();
        std::fs::write(
            root.join("lines.ndjson"),
            "{\"entity\": \"Google\"}\n{\"entity\": \"Microsoft\"}\n",
        ).unwrap();
        std::fs::write(
            root.join("people.csv"),
            "name,role\nAlice,CEO\nBob,CTO\n",
        ).unwrap();
        std::fs::write(root.join("ignore.bin"), &[0u8; 10]).unwrap();

        (dir, root)
    }

    #[test]
    fn scan_reads_text_files() {
        let (_dir, root) = temp_dir_with_files();
        let source = FileSource::new(FileSourceConfig {
            root: root.clone(),
            recursive: false,
            ..Default::default()
        });

        let items = source.scan().unwrap();
        // Should get: hello.txt (1), data.json (2), lines.ndjson (2), people.csv (2) = 7
        assert_eq!(items.len(), 7);
    }

    #[test]
    fn json_array_produces_multiple_items() {
        let (_dir, root) = temp_dir_with_files();
        let source = FileSource::new(FileSourceConfig {
            root: root.clone(),
            extensions: vec!["json".into()],
            recursive: false,
            ..Default::default()
        });

        let items = source.scan().unwrap();
        assert_eq!(items.len(), 2);
        // Both should be structured
        for item in &items {
            match &item.content {
                Content::Structured(map) => {
                    assert!(map.contains_key("entity"));
                }
                _ => panic!("expected structured content"),
            }
        }
    }

    #[test]
    fn ndjson_produces_one_item_per_line() {
        let (_dir, root) = temp_dir_with_files();
        let source = FileSource::new(FileSourceConfig {
            root: root.clone(),
            extensions: vec!["ndjson".into()],
            recursive: false,
            ..Default::default()
        });

        let items = source.scan().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn csv_produces_structured_items() {
        let (_dir, root) = temp_dir_with_files();
        let source = FileSource::new(FileSourceConfig {
            root: root.clone(),
            extensions: vec!["csv".into()],
            recursive: false,
            ..Default::default()
        });

        let items = source.scan().unwrap();
        assert_eq!(items.len(), 2);

        match &items[0].content {
            Content::Structured(map) => {
                assert_eq!(map.get("name").unwrap(), "Alice");
                assert_eq!(map.get("role").unwrap(), "CEO");
            }
            _ => panic!("expected structured content"),
        }
    }

    #[test]
    fn extension_filter_excludes_non_matching() {
        let (_dir, root) = temp_dir_with_files();
        let source = FileSource::new(FileSourceConfig {
            root: root.clone(),
            extensions: vec!["txt".into()],
            recursive: false,
            ..Default::default()
        });

        let items = source.scan().unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].content {
            Content::Text(t) => assert_eq!(t, "Hello World"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn recursive_scan_finds_nested_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(root.join("top.txt"), "top").unwrap();
        std::fs::write(sub.join("nested.txt"), "nested").unwrap();

        let source = FileSource::new(FileSourceConfig {
            root: root.to_path_buf(),
            extensions: vec!["txt".into()],
            recursive: true,
            ..Default::default()
        });

        let items = source.scan().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn poll_watcher_detects_new_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();

        std::fs::write(root.join("first.txt"), "first").unwrap();

        let watcher = PollWatcher::new(FileSourceConfig {
            root: root.to_path_buf(),
            extensions: vec!["txt".into()],
            recursive: false,
            ..Default::default()
        });

        // First poll: should find first.txt
        let changed = watcher.poll().unwrap();
        assert_eq!(changed.len(), 1);

        // Second poll without changes: should find nothing
        let changed = watcher.poll().unwrap();
        assert_eq!(changed.len(), 0);

        // Add a new file
        std::fs::write(root.join("second.txt"), "second").unwrap();
        let changed = watcher.poll().unwrap();
        assert_eq!(changed.len(), 1);
        assert!(changed[0].file_name().unwrap().to_str().unwrap() == "second.txt");
    }

    #[test]
    fn source_trait_impl_works() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello").unwrap();

        let source = FileSource::new(FileSourceConfig {
            root: dir.path().to_path_buf(),
            extensions: vec!["txt".into()],
            recursive: false,
            ..Default::default()
        });

        assert_eq!(source.name(), "file");
        let caps = source.capabilities();
        assert!(!caps.searchable);
        assert!(matches!(caps.cost_model, CostModel::Free));
    }
}
