use crate::model::UsageStats;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub const CACHE_VERSION: u32 = 3;

#[derive(Debug, Serialize, Deserialize)]
pub struct UsageCache {
    pub version: u32,
    pub files: HashMap<String, CachedFile>,
}

impl Default for UsageCache {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
            files: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFile {
    pub mtime_ms: u128,
    pub size: u64,
    pub stats: HashMap<String, UsageStats>,
    #[serde(default)]
    pub cwd: Option<String>,
}

pub fn load(path: &Path) -> UsageCache {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return UsageCache::default();
    };
    match serde_json::from_str::<UsageCache>(&raw) {
        Ok(c) if c.version == CACHE_VERSION => c,
        _ => UsageCache::default(),
    }
}

pub fn save(path: &Path, cache: &UsageCache) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string(cache).expect("cache serializes");
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cache").join("usage-v1.json");
        let mut c = UsageCache::default();
        c.files.insert(
            "/a/b.jsonl".into(),
            CachedFile {
                mtime_ms: 123,
                size: 456,
                stats: Default::default(),
                cwd: None,
            },
        );
        save(&path, &c).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files["/a/b.jsonl"].mtime_ms, 123);
    }

    #[test]
    fn missing_or_corrupt_cache_loads_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(load(&tmp.path().join("nope.json")).files.is_empty());
        let bad = tmp.path().join("bad.json");
        std::fs::write(&bad, "{not json").unwrap();
        assert!(load(&bad).files.is_empty());
    }

    #[test]
    fn version_mismatch_loads_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("c.json");
        std::fs::write(
            &path,
            r#"{"version":999,"files":{"/x":{"mtime_ms":1,"size":1,"stats":{}}}}"#,
        )
        .unwrap();
        assert!(load(&path).files.is_empty());
    }
}
