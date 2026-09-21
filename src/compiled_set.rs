//! The compiled schema set artifact: the deterministic output of a
//! batch compile (wire models + integrity digests) encoded with
//! bincode so both Rust tools and the Ruby ext can load it without
//! re-parsing any EXPRESS.
//!
//! Format v1: `EXSCS1` header, format version, producing versions,
//! the set digest (SHA-256 over sorted `wire_path\0source_sha`), and
//! the per-file entries. Derived tables (resolved paths, graph) are
//! rebuilt after load until they move into the artifact proper.

use serde::{Deserialize, Serialize};
use serde_json::Value as Wire;
use sha2::{Digest as _, Sha256};

pub const MAGIC: &[u8; 6] = b"EXSCS1";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct CompiledSetHeader {
    pub magic: [u8; 6],
    pub format_version: u32,
    pub expressir_rs_version: String,
    pub grammar_digest: String,
    pub set_digest: String,
}

#[derive(Serialize, Deserialize)]
pub struct CompiledFile {
    pub wire_path: String,
    pub source_sha: String,
    /// The wire model as its JSON text: bincode is not self-describing,
    /// so `serde_json::Value` itself cannot ride inside the envelope.
    pub wire_json: String,
}

impl CompiledFile {
    pub fn wire(&self) -> Result<Wire, String> {
        serde_json::from_str(&self.wire_json).map_err(|e| format!("wire decode: {e}"))
    }
}

#[derive(Serialize, Deserialize)]
pub struct CompiledSet {
    pub header: CompiledSetHeader,
    pub files: Vec<CompiledFile>,
}

/// SHA-256 hex digest of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of a file's bytes; empty string when unreadable.
pub fn file_sha256(path: &str) -> String {
    match std::fs::read(path) {
        Ok(bytes) => sha256_hex(&bytes),
        Err(_) => String::new(),
    }
}

/// The set digest over `wire_path`/`source_sha` pairs (sorted for
/// determinism).
pub fn set_digest(files: &[(String, String)]) -> String {
    let mut lines: Vec<String> = files
        .iter()
        .map(|(wire_path, source_sha)| format!("{wire_path}\x00{source_sha}"))
        .collect();
    lines.sort();
    let joined = lines.join("\n");
    sha256_hex(joined.as_bytes())
}

impl CompiledSet {
    /// Build a set from compiled entries, stamping header digests.
    pub fn build(
        files: Vec<CompiledFile>,
        expressir_rs_version: &str,
        grammar_digest: &str,
    ) -> Self {
        let pairs: Vec<(String, String)> = files
            .iter()
            .map(|f| (f.wire_path.clone(), f.source_sha.clone()))
            .collect();
        CompiledSet {
            header: CompiledSetHeader {
                magic: *MAGIC,
                format_version: FORMAT_VERSION,
                expressir_rs_version: expressir_rs_version.to_string(),
                grammar_digest: grammar_digest.to_string(),
                set_digest: set_digest(&pairs),
            },
            files,
        }
    }

    /// Encode to `path` (atomically: temp file + rename).
    pub fn write_to(&self, path: &str) -> Result<(), String> {
        let bytes = bincode::serialize(self).map_err(|e| format!("encode: {e}"))?;
        let tmp = format!("{path}.tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| format!("write {tmp}: {e}"))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("rename {path}: {e}"))
    }

    /// Decode from `path`, validating magic and format version.
    pub fn read_from(path: &str) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
        let set: CompiledSet = bincode::deserialize(&bytes).map_err(|e| format!("decode: {e}"))?;
        if set.header.magic != *MAGIC {
            return Err(format!("bad magic in {path}"));
        }
        if set.header.format_version != FORMAT_VERSION {
            return Err(format!(
                "format version {} != {} in {path}",
                set.header.format_version, FORMAT_VERSION
            ));
        }
        Ok(set)
    }

    /// Recompute the set digest from current file contents, mapped by
    /// wire path; `None` when any source is unreadable.
    pub fn matches_sources(&self, read_path_for: &dyn Fn(&str) -> Option<String>) -> Option<bool> {
        let mut pairs = Vec::with_capacity(self.files.len());
        for file in &self.files {
            let read_path = read_path_for(&file.wire_path)?;
            pairs.push((file.wire_path.clone(), file_sha256(&read_path)));
        }
        Some(set_digest(&pairs) == self.header.set_digest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(id: &str) -> Wire {
        serde_json::json!({
            "_class": "Expressir::Model::ExpFile",
            "schemas": [{ "_class": "Expressir::Model::Declarations::Schema", "id": id }]
        })
    }

    #[test]
    fn round_trips_and_verifies() {
        let files = vec![
            CompiledFile {
                wire_path: "a.exp".into(),
                source_sha: "aa".into(),
                wire_json: serde_json::to_string(&wire("a")).unwrap(),
            },
            CompiledFile {
                wire_path: "b.exp".into(),
                source_sha: "bb".into(),
                wire_json: serde_json::to_string(&wire("b")).unwrap(),
            },
        ];
        let set = CompiledSet::build(files, "0.1.0-test", "grammar-digest");
        let path = std::env::temp_dir().join(format!("expressir-set-{}.exscs", std::process::id()));
        set.write_to(path.to_str().unwrap()).unwrap();

        let loaded = CompiledSet::read_from(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.header.set_digest, set.header.set_digest);
        assert_eq!(loaded.files.len(), 2);
        assert_eq!(loaded.files[0].wire().unwrap()["schemas"][0]["id"], "a");

        // Digest over the same pairs verifies; tampering does not.
        assert_eq!(
            set_digest(&[("a.exp".into(), "aa".into()), ("b.exp".into(), "bb".into())]),
            loaded.header.set_digest
        );
        assert_ne!(
            set_digest(&[("a.exp".into(), "xx".into()), ("b.exp".into(), "bb".into())]),
            loaded.header.set_digest
        );
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn set_digest_is_order_independent() {
        let a = [
            ("x".to_string(), "1".to_string()),
            ("y".to_string(), "2".to_string()),
        ];
        let b = [
            ("y".to_string(), "2".to_string()),
            ("x".to_string(), "1".to_string()),
        ];
        assert_eq!(set_digest(&a), set_digest(&b));
    }
}
