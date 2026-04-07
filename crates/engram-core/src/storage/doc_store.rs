/// Segmented append-only blob store for document content caching.
///
/// Stores compressed document content (text, PDF, images) in segmented files
/// with a persisted index for fast lookup. Content-addressable by SHA-256 hash.
///
/// File layout:
///   {brain_path}.docs.0     — segment 0 (sealed, max 256MB default)
///   {brain_path}.docs.1     — segment 1 (sealed)
///   {brain_path}.docs.N     — active segment (appending)
///   {brain_path}.docs.idx   — persisted index spanning all segments
///
/// Crash-safe: append-only writes + per-entry CRC32 checksums.
/// Index is derived — can be rebuilt from segments if corrupted.

use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};

// ── Constants ──

const DEFAULT_MAX_SEGMENT_BYTES: u64 = 256 * 1024 * 1024; // 256 MB
const ZSTD_LEVEL: i32 = 3;
const INDEX_MAGIC: &[u8; 4] = b"EDXI";
const INDEX_VERSION: u32 = 1;
const INDEX_HEADER_SIZE: usize = 12; // magic(4) + version(4) + count(4)
const INDEX_ENTRY_SIZE: usize = 50;  // hash(32) + seg(1) + offset(8) + clen(4) + mime(1) + crc(4)

// ── Public types ──

/// MIME type tag for stored content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MimeType {
    Text = 0,
    Html = 1,
    Pdf = 2,
    Png = 3,
    Jpeg = 4,
    Markdown = 5,
    Json = 6,
    Xml = 7,
    OctetStream = 255,
}

impl MimeType {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Text,
            1 => Self::Html,
            2 => Self::Pdf,
            3 => Self::Png,
            4 => Self::Jpeg,
            5 => Self::Markdown,
            6 => Self::Json,
            7 => Self::Xml,
            _ => Self::OctetStream,
        }
    }

    /// Infer MIME type from a string hint (e.g., "text/plain", "application/pdf").
    pub fn from_mime_str(s: &str) -> Self {
        let lower = s.to_lowercase();
        if lower.contains("html") { Self::Html }
        else if lower.contains("pdf") { Self::Pdf }
        else if lower.contains("png") { Self::Png }
        else if lower.contains("jpeg") || lower.contains("jpg") { Self::Jpeg }
        else if lower.contains("markdown") || lower.contains("md") { Self::Markdown }
        else if lower.contains("json") { Self::Json }
        else if lower.contains("xml") { Self::Xml }
        else if lower.contains("text") { Self::Text }
        else { Self::OctetStream }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text/plain",
            Self::Html => "text/html",
            Self::Pdf => "application/pdf",
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Markdown => "text/markdown",
            Self::Json => "application/json",
            Self::Xml => "application/xml",
            Self::OctetStream => "application/octet-stream",
        }
    }
}

/// Statistics returned by compaction.
#[derive(Debug, Default)]
pub struct CompactStats {
    pub entries_kept: u32,
    pub entries_removed: u32,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Hash type alias for content addressing.
pub type ContentHash = [u8; 32];

// ── Internal types ──

#[derive(Debug, Clone)]
struct IndexEntry {
    segment_id: u16,
    offset: u64,
    compressed_length: u32,
    mime_type: MimeType,
    checksum: u32,
}

// ── Segment entry header (written before compressed bytes) ──
// Layout: hash(32) + version(1) + mime(1) + flags(1) + reserved(1)
//       + raw_len(4) + compressed_len(4) + crc(4) = 48 bytes
const SEGMENT_ENTRY_HEADER_SIZE: usize = 48;

// ── DocStore ──

/// Content-addressable segmented blob store.
pub struct DocStore {
    brain_path: PathBuf,
    index: HashMap<ContentHash, IndexEntry>,
    active_segment: u16,
    max_segment_bytes: u64,
}

impl DocStore {
    /// Create an empty placeholder DocStore (no backing files).
    /// Use `open()` to create a real store with persistence.
    pub fn empty() -> Self {
        DocStore {
            brain_path: PathBuf::new(),
            index: HashMap::new(),
            active_segment: 0,
            max_segment_bytes: DEFAULT_MAX_SEGMENT_BYTES,
        }
    }

    /// Open or create a DocStore for the given brain file path.
    pub fn open(brain_path: &Path) -> io::Result<Self> {
        let mut store = DocStore {
            brain_path: brain_path.to_path_buf(),
            index: HashMap::new(),
            active_segment: 0,
            max_segment_bytes: DEFAULT_MAX_SEGMENT_BYTES,
        };
        store.load_or_rebuild_index()?;
        Ok(store)
    }

    /// Open with a custom max segment size (for testing).
    pub fn open_with_max_segment(brain_path: &Path, max_bytes: u64) -> io::Result<Self> {
        let mut store = DocStore {
            brain_path: brain_path.to_path_buf(),
            index: HashMap::new(),
            active_segment: 0,
            max_segment_bytes: max_bytes,
        };
        store.load_or_rebuild_index()?;
        Ok(store)
    }

    /// Store content, returning its SHA-256 hash. Deduplicates automatically.
    pub fn store(&mut self, content: &[u8], mime: MimeType) -> io::Result<ContentHash> {
        let hash = Self::hash_content(content);
        if self.index.contains_key(&hash) {
            return Ok(hash); // already stored
        }
        self.ensure_active_segment_has_space(content.len())?;
        let (offset, compressed_length, checksum) = self.append_to_segment(
            self.active_segment, &hash, content, mime,
        )?;
        let entry = IndexEntry {
            segment_id: self.active_segment,
            offset,
            compressed_length,
            mime_type: mime,
            checksum,
        };
        self.append_index_entry(&hash, &entry)?;
        self.index.insert(hash, entry);
        Ok(hash)
    }

    /// Load content by hash. Decompresses and verifies CRC.
    pub fn load(&self, hash: &ContentHash) -> io::Result<(Vec<u8>, MimeType)> {
        let entry = self.index.get(hash).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "document not found in store")
        })?;
        let seg_path = self.segment_path(entry.segment_id);
        let mut file = File::open(&seg_path)?;
        file.seek(SeekFrom::Start(entry.offset))?;
        let mut header_buf = [0u8; SEGMENT_ENTRY_HEADER_SIZE];
        file.read_exact(&mut header_buf)?;
        let compressed_length = u32::from_le_bytes(
            header_buf[40..44].try_into().unwrap(),
        ) as usize;
        let stored_crc = u32::from_le_bytes(
            header_buf[44..48].try_into().unwrap(),
        );
        let mut compressed = vec![0u8; compressed_length];
        file.read_exact(&mut compressed)?;
        let actual_crc = crc32fast::hash(&compressed);
        if actual_crc != stored_crc {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("CRC mismatch: expected {stored_crc:#x}, got {actual_crc:#x}"),
            ));
        }
        let flags = header_buf[34];
        let decompressed = if flags & 1 != 0 {
            zstd::stream::decode_all(&compressed[..])?
        } else {
            compressed
        };
        Ok((decompressed, entry.mime_type))
    }

    /// Check if a hash exists in the store.
    pub fn contains(&self, hash: &ContentHash) -> bool {
        self.index.contains_key(hash)
    }

    /// Logically delete an entry (removes from index, space reclaimed on compaction).
    pub fn remove(&mut self, hash: &ContentHash) -> bool {
        let removed = self.index.remove(hash).is_some();
        if removed {
            // Rewrite the full index without this entry
            let _ = self.persist_full_index();
        }
        removed
    }

    /// Number of stored entries.
    pub fn entry_count(&self) -> usize {
        self.index.len()
    }

    /// Return all stored content hashes (for iteration/diagnostics).
    pub fn hashes(&self) -> Vec<ContentHash> {
        self.index.keys().copied().collect()
    }

    /// Return all stored content hashes with their MIME types.
    pub fn hashes_with_mime(&self) -> Vec<(ContentHash, MimeType)> {
        self.index.iter().map(|(h, e)| (*h, e.mime_type)).collect()
    }

    /// Compute SHA-256 hash of content.
    pub fn hash_content(content: &[u8]) -> ContentHash {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    /// Format a content hash as hex string.
    pub fn hash_hex(hash: &ContentHash) -> String {
        hash.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Format a short (8-char) hex label for graph nodes.
    pub fn hash_short(hash: &ContentHash) -> String {
        hash[..4].iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Rebuild the index by scanning all segment files.
    pub fn rebuild_index(&mut self) -> io::Result<u32> {
        self.index.clear();
        let mut count = 0u32;
        let mut seg_id: u16 = 0;
        loop {
            let path = self.segment_path(seg_id);
            if !path.exists() {
                break;
            }
            count += self.scan_segment(seg_id)?;
            seg_id += 1;
        }
        self.active_segment = if seg_id == 0 { 0 } else { seg_id - 1 };
        self.persist_full_index()?;
        Ok(count)
    }

    /// Total bytes across all segment files.
    pub fn total_bytes(&self) -> u64 {
        let mut total = 0u64;
        for seg_id in 0..=self.active_segment {
            if let Ok(meta) = std::fs::metadata(self.segment_path(seg_id)) {
                total += meta.len();
            }
        }
        total
    }
}

// ── Private implementation ──

impl DocStore {
    fn segment_path(&self, seg_id: u16) -> PathBuf {
        let mut base = self.brain_path.as_os_str().to_owned();
        base.push(format!(".docs.{seg_id}"));
        PathBuf::from(base)
    }

    fn index_path(&self) -> PathBuf {
        let mut base = self.brain_path.as_os_str().to_owned();
        base.push(".docs.idx");
        PathBuf::from(base)
    }

    fn load_or_rebuild_index(&mut self) -> io::Result<()> {
        let idx_path = self.index_path();
        if idx_path.exists() {
            match self.load_index_file() {
                Ok(()) => {
                    self.find_active_segment();
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("DocStore index corrupt ({}), rebuilding...", e);
                }
            }
        }
        // No index or corrupt — check if any segments exist
        if self.segment_path(0).exists() {
            let count = self.rebuild_index()?;
            tracing::info!("DocStore: rebuilt index with {count} entries");
        } else {
            // Fresh store, no segments yet
            self.active_segment = 0;
        }
        Ok(())
    }

    fn load_index_file(&mut self) -> io::Result<()> {
        let mut file = File::open(self.index_path())?;
        let mut header = [0u8; INDEX_HEADER_SIZE];
        file.read_exact(&mut header)?;
        if &header[0..4] != INDEX_MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad index magic"));
        }
        let version = u32::from_le_bytes(header[4..8].try_into().unwrap());
        if version != INDEX_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported index version {version}"),
            ));
        }
        let count = u32::from_le_bytes(header[8..12].try_into().unwrap());
        self.index.clear();
        self.index.reserve(count as usize);
        let mut buf = [0u8; INDEX_ENTRY_SIZE];
        for _ in 0..count {
            if file.read_exact(&mut buf).is_err() {
                break; // truncated — use what we have
            }
            let (hash, entry) = Self::parse_index_entry(&buf);
            self.index.insert(hash, entry);
        }
        Ok(())
    }

    fn parse_index_entry(buf: &[u8; INDEX_ENTRY_SIZE]) -> (ContentHash, IndexEntry) {
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&buf[0..32]);
        let segment_id = buf[32] as u16;
        let offset = u64::from_le_bytes(buf[33..41].try_into().unwrap());
        let compressed_length = u32::from_le_bytes(buf[41..45].try_into().unwrap());
        let mime_type = MimeType::from_u8(buf[45]);
        let checksum = u32::from_le_bytes(buf[46..50].try_into().unwrap());
        (hash, IndexEntry { segment_id, offset, compressed_length, mime_type, checksum })
    }

    fn serialize_index_entry(hash: &ContentHash, entry: &IndexEntry) -> [u8; INDEX_ENTRY_SIZE] {
        let mut buf = [0u8; INDEX_ENTRY_SIZE];
        buf[0..32].copy_from_slice(hash);
        buf[32] = entry.segment_id as u8;
        buf[33..41].copy_from_slice(&entry.offset.to_le_bytes());
        buf[41..45].copy_from_slice(&entry.compressed_length.to_le_bytes());
        buf[45] = entry.mime_type as u8;
        buf[46..50].copy_from_slice(&entry.checksum.to_le_bytes());
        buf
    }

    fn persist_full_index(&self) -> io::Result<()> {
        let path = self.index_path();
        let mut file = File::create(&path)?;
        let mut header = [0u8; INDEX_HEADER_SIZE];
        header[0..4].copy_from_slice(INDEX_MAGIC);
        header[4..8].copy_from_slice(&INDEX_VERSION.to_le_bytes());
        header[8..12].copy_from_slice(&(self.index.len() as u32).to_le_bytes());
        file.write_all(&header)?;
        for (hash, entry) in &self.index {
            let buf = Self::serialize_index_entry(hash, entry);
            file.write_all(&buf)?;
        }
        file.sync_all()?;
        Ok(())
    }

    fn append_index_entry(&self, hash: &ContentHash, entry: &IndexEntry) -> io::Result<()> {
        let path = self.index_path();
        if !path.exists() {
            // Create with header first
            let mut file = File::create(&path)?;
            let mut header = [0u8; INDEX_HEADER_SIZE];
            header[0..4].copy_from_slice(INDEX_MAGIC);
            header[4..8].copy_from_slice(&INDEX_VERSION.to_le_bytes());
            header[8..12].copy_from_slice(&1u32.to_le_bytes());
            file.write_all(&header)?;
            let buf = Self::serialize_index_entry(hash, entry);
            file.write_all(&buf)?;
            file.sync_all()?;
            return Ok(());
        }
        let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
        // Update count in header
        file.seek(SeekFrom::Start(8))?;
        let new_count = (self.index.len() + 1) as u32; // +1 because not yet in index
        file.write_all(&new_count.to_le_bytes())?;
        // Append entry at end
        file.seek(SeekFrom::End(0))?;
        let buf = Self::serialize_index_entry(hash, entry);
        file.write_all(&buf)?;
        file.sync_all()?;
        Ok(())
    }

    fn find_active_segment(&mut self) {
        let mut seg_id: u16 = 0;
        loop {
            let next = self.segment_path(seg_id + 1);
            if next.exists() {
                seg_id += 1;
            } else {
                break;
            }
        }
        self.active_segment = seg_id;
    }

    fn ensure_active_segment_has_space(&mut self, content_len: usize) -> io::Result<()> {
        let seg_path = self.segment_path(self.active_segment);
        let current_size = if seg_path.exists() {
            std::fs::metadata(&seg_path)?.len()
        } else {
            0
        };
        let estimated = SEGMENT_ENTRY_HEADER_SIZE as u64 + content_len as u64;
        if current_size > 0 && current_size + estimated > self.max_segment_bytes {
            self.active_segment += 1;
        }
        Ok(())
    }

    fn append_to_segment(
        &self,
        seg_id: u16,
        hash: &ContentHash,
        content: &[u8],
        mime: MimeType,
    ) -> io::Result<(u64, u32, u32)> {
        let seg_path = self.segment_path(seg_id);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&seg_path)?;
        let offset = file.seek(SeekFrom::End(0))?;
        let compressed = zstd::stream::encode_all(content, ZSTD_LEVEL)?;
        let is_compressed = compressed.len() < content.len();
        let (stored_bytes, flags) = if is_compressed {
            (&compressed[..], 1u8)
        } else {
            (content, 0u8)
        };
        let checksum = crc32fast::hash(stored_bytes);
        // Write entry header
        let mut header = [0u8; SEGMENT_ENTRY_HEADER_SIZE];
        header[0..32].copy_from_slice(hash);
        header[32] = 1; // segment_version
        header[33] = mime as u8;
        header[34] = flags;
        header[35] = 0; // reserved
        header[36..40].copy_from_slice(&(content.len() as u32).to_le_bytes());
        header[40..44].copy_from_slice(&(stored_bytes.len() as u32).to_le_bytes());
        header[44..48].copy_from_slice(&checksum.to_le_bytes());
        file.write_all(&header)?;
        file.write_all(stored_bytes)?;
        file.sync_all()?;
        Ok((offset, stored_bytes.len() as u32, checksum))
    }

    fn scan_segment(&mut self, seg_id: u16) -> io::Result<u32> {
        let path = self.segment_path(seg_id);
        let mut file = File::open(&path)?;
        let file_len = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(0))?;
        let mut count = 0u32;
        loop {
            let offset = file.seek(SeekFrom::Current(0))?;
            if offset + SEGMENT_ENTRY_HEADER_SIZE as u64 > file_len {
                break; // not enough bytes for a header — truncated entry
            }
            let mut header = [0u8; SEGMENT_ENTRY_HEADER_SIZE];
            if file.read_exact(&mut header).is_err() {
                break;
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&header[0..32]);
            let compressed_length = u32::from_le_bytes(
                header[40..44].try_into().unwrap(),
            );
            let mime = MimeType::from_u8(header[33]);
            let checksum = u32::from_le_bytes(
                header[44..48].try_into().unwrap(),
            );
            // Verify we have enough bytes for the content
            let pos = file.seek(SeekFrom::Current(0))?;
            if pos + compressed_length as u64 > file_len {
                // Truncated entry — skip (crash recovery)
                tracing::warn!(
                    "DocStore: truncated entry at segment {seg_id} offset {offset}, truncating"
                );
                // Truncate the file at the start of this entry
                file.set_len(offset)?;
                break;
            }
            // Skip over content bytes
            file.seek(SeekFrom::Current(compressed_length as i64))?;
            self.index.insert(hash, IndexEntry {
                segment_id: seg_id,
                offset,
                compressed_length,
                mime_type: mime,
                checksum,
            });
            count += 1;
        }
        Ok(count)
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_brain() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let brain = dir.path().join("test.brain");
        (dir, brain)
    }

    #[test]
    fn test_store_and_load() {
        let (_dir, brain) = temp_brain();
        let mut store = DocStore::open(&brain).unwrap();
        let content = b"Vladimir Putin addressed the nation on March 1st.";
        let hash = store.store(content, MimeType::Text).unwrap();
        let (loaded, mime) = store.load(&hash).unwrap();
        assert_eq!(loaded, content);
        assert_eq!(mime, MimeType::Text);
    }

    #[test]
    fn test_dedup() {
        let (_dir, brain) = temp_brain();
        let mut store = DocStore::open(&brain).unwrap();
        let content = b"Duplicate content test";
        let h1 = store.store(content, MimeType::Text).unwrap();
        let h2 = store.store(content, MimeType::Text).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(store.entry_count(), 1);
    }

    #[test]
    fn test_different_content() {
        let (_dir, brain) = temp_brain();
        let mut store = DocStore::open(&brain).unwrap();
        let h1 = store.store(b"First document", MimeType::Text).unwrap();
        let h2 = store.store(b"Second document", MimeType::Text).unwrap();
        assert_ne!(h1, h2);
        assert_eq!(store.entry_count(), 2);
        let (c1, _) = store.load(&h1).unwrap();
        let (c2, _) = store.load(&h2).unwrap();
        assert_eq!(c1, b"First document");
        assert_eq!(c2, b"Second document");
    }

    #[test]
    fn test_mime_types() {
        let (_dir, brain) = temp_brain();
        let mut store = DocStore::open(&brain).unwrap();
        let h_text = store.store(b"text", MimeType::Text).unwrap();
        let h_pdf = store.store(b"fake pdf bytes", MimeType::Pdf).unwrap();
        let h_png = store.store(b"fake png bytes", MimeType::Png).unwrap();
        assert_eq!(store.load(&h_text).unwrap().1, MimeType::Text);
        assert_eq!(store.load(&h_pdf).unwrap().1, MimeType::Pdf);
        assert_eq!(store.load(&h_png).unwrap().1, MimeType::Png);
    }

    #[test]
    fn test_compression() {
        let (_dir, brain) = temp_brain();
        let mut store = DocStore::open(&brain).unwrap();
        // Highly compressible: repeated text
        let content: Vec<u8> = "Putin addresses Ukraine crisis. ".repeat(1000).into_bytes();
        store.store(&content, MimeType::Text).unwrap();
        let seg_size = std::fs::metadata(store.segment_path(0)).unwrap().len();
        assert!(
            seg_size < content.len() as u64,
            "compressed {seg_size} should be < raw {}",
            content.len()
        );
    }

    #[test]
    fn test_crc_integrity() {
        let (_dir, brain) = temp_brain();
        let mut store = DocStore::open(&brain).unwrap();
        let hash = store.store(b"integrity test data", MimeType::Text).unwrap();
        // Corrupt the stored bytes in the segment file
        let seg_path = store.segment_path(0);
        let mut data = std::fs::read(&seg_path).unwrap();
        // Flip a byte in the compressed content (after the 48-byte header)
        if data.len() > SEGMENT_ENTRY_HEADER_SIZE + 2 {
            data[SEGMENT_ENTRY_HEADER_SIZE + 1] ^= 0xFF;
        }
        std::fs::write(&seg_path, &data).unwrap();
        let result = store.load(&hash);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("CRC mismatch"));
    }

    #[test]
    fn test_segment_rollover() {
        let (_dir, brain) = temp_brain();
        // Tiny max segment to force rollover
        let mut store = DocStore::open_with_max_segment(&brain, 200).unwrap();
        let h1 = store.store(b"First chunk of data that is moderately long", MimeType::Text).unwrap();
        let h2 = store.store(b"Second chunk of data that should go to next segment", MimeType::Text).unwrap();
        let h3 = store.store(b"Third chunk going to yet another segment file", MimeType::Text).unwrap();
        assert_eq!(store.entry_count(), 3);
        // Should have multiple segments
        assert!(store.active_segment >= 1, "expected rollover, active={}", store.active_segment);
        // All content still retrievable
        assert_eq!(store.load(&h1).unwrap().0, b"First chunk of data that is moderately long");
        assert_eq!(store.load(&h2).unwrap().0, b"Second chunk of data that should go to next segment");
        assert_eq!(store.load(&h3).unwrap().0, b"Third chunk going to yet another segment file");
    }

    #[test]
    fn test_index_persist_and_reload() {
        let (_dir, brain) = temp_brain();
        let hash;
        {
            let mut store = DocStore::open(&brain).unwrap();
            hash = store.store(b"Persistent content", MimeType::Html).unwrap();
            assert_eq!(store.entry_count(), 1);
        }
        // Reopen — should load from .idx, not rebuild
        let store2 = DocStore::open(&brain).unwrap();
        assert_eq!(store2.entry_count(), 1);
        let (content, mime) = store2.load(&hash).unwrap();
        assert_eq!(content, b"Persistent content");
        assert_eq!(mime, MimeType::Html);
    }

    #[test]
    fn test_index_rebuild() {
        let (_dir, brain) = temp_brain();
        let hash;
        {
            let mut store = DocStore::open(&brain).unwrap();
            hash = store.store(b"Rebuild test", MimeType::Text).unwrap();
        }
        // Delete the index file
        let idx_path = {
            let mut p = brain.as_os_str().to_owned();
            p.push(".docs.idx");
            PathBuf::from(p)
        };
        std::fs::remove_file(&idx_path).unwrap();
        // Reopen — should rebuild from segments
        let store2 = DocStore::open(&brain).unwrap();
        assert_eq!(store2.entry_count(), 1);
        assert_eq!(store2.load(&hash).unwrap().0, b"Rebuild test");
    }

    #[test]
    fn test_crash_recovery() {
        let (_dir, brain) = temp_brain();
        let hash;
        {
            let mut store = DocStore::open(&brain).unwrap();
            hash = store.store(b"Good entry before crash", MimeType::Text).unwrap();
        }
        // Simulate crash: append garbage to segment (partial entry)
        let seg_path = {
            let mut p = brain.as_os_str().to_owned();
            p.push(".docs.0");
            PathBuf::from(p)
        };
        {
            let mut f = OpenOptions::new().append(true).open(&seg_path).unwrap();
            f.write_all(b"TRUNCATED_GARBAGE_PARTIAL_HEADER").unwrap();
        }
        // Delete index to force rebuild
        let idx_path = {
            let mut p = brain.as_os_str().to_owned();
            p.push(".docs.idx");
            PathBuf::from(p)
        };
        std::fs::remove_file(&idx_path).unwrap();
        // Reopen — should recover good entry, discard garbage
        let store2 = DocStore::open(&brain).unwrap();
        assert_eq!(store2.entry_count(), 1);
        assert_eq!(store2.load(&hash).unwrap().0, b"Good entry before crash");
    }

    #[test]
    fn test_logical_delete() {
        let (_dir, brain) = temp_brain();
        let mut store = DocStore::open(&brain).unwrap();
        let hash = store.store(b"Delete me", MimeType::Text).unwrap();
        assert!(store.contains(&hash));
        assert!(store.remove(&hash));
        assert!(!store.contains(&hash));
        assert!(store.load(&hash).is_err());
        assert_eq!(store.entry_count(), 0);
    }

    #[test]
    fn test_empty_store() {
        let (_dir, brain) = temp_brain();
        let store = DocStore::open(&brain).unwrap();
        assert_eq!(store.entry_count(), 0);
        let fake_hash = [0u8; 32];
        assert!(!store.contains(&fake_hash));
        assert!(store.load(&fake_hash).is_err());
    }

    #[test]
    fn test_hash_functions() {
        let hash = DocStore::hash_content(b"test content");
        let hex = DocStore::hash_hex(&hash);
        assert_eq!(hex.len(), 64);
        let short = DocStore::hash_short(&hash);
        assert_eq!(short.len(), 8);
        assert_eq!(&hex[..8], &short);
    }
}
