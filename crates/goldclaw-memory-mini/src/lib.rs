use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;
use rustc_hash::FxHasher;
use std::hash::Hasher;
use parking_lot::RwLock;
use std::sync::Arc;
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub category: String,
    pub content: String,
    // keep minimal fields expected by gateway
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub score: Option<f32>,
}

#[derive(Clone)]
pub struct SqliteMemory {
    // Map keyed by precomputed FxHash (u64) of the entry id for zero-cost lookups
    inner: Arc<RwLock<FxHashMap<u64, MemoryEntry>>>,
}

// Public MemoryCategory exported by this crate so callers can construct it
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MemoryCategory {
    Core,
    Daily,
    Conversation,
    Custom(String),
}

impl SqliteMemory {
    // Synchronous constructor expected by gateway
    pub fn new<P: AsRef<Path>>(_path: P) -> anyhow::Result<Self> {
        Ok(SqliteMemory { inner: Arc::new(RwLock::new(FxHashMap::default())) })
    }

    pub async fn recall(&self, _q: &str, _limit: usize, _ns: Option<&str>, _since: Option<&str>, _score: Option<f32>) -> anyhow::Result<Vec<MemoryEntry>> {
        let map = self.inner.read();
        // Avoid cloning the entire store; only collect up to `_limit` entries.
        let cap = std::cmp::min(_limit, map.len());
        let mut v: Vec<MemoryEntry> = Vec::with_capacity(cap);
        for entry in map.values().take(_limit) {
            v.push(entry.clone());
            if v.len() >= _limit { break; }
        }
        Ok(v)
    }

    pub async fn store(&self, entry_id: &str, content: &str, _cat: MemoryCategory, _session: Option<&str>) -> anyhow::Result<()> {
        let mut map = self.inner.write();
        let e = MemoryEntry { id: entry_id.to_string(), category: "default".into(), content: content.to_string(), timestamp: None, namespace: None, score: None };
        // Use precomputed FxHash of the entry id as the map key
        let mut hasher = FxHasher::default();
        hasher.write(e.id.as_bytes());
        let key = hasher.finish();
        map.insert(key, e);
        Ok(())
    }

    pub async fn list(&self, _limit: Option<usize>, _category: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>> {
        let map = self.inner.read();
        let mut v: Vec<MemoryEntry> = map.values().cloned().collect();
        // Apply a simple limit if requested
        if let Some(n) = _limit {
            v.truncate(n);
        }
        Ok(v)
    }

    pub async fn forget(&self, id: &str) -> anyhow::Result<bool> {
        let mut map = self.inner.write();
        let mut hasher = FxHasher::default();
        hasher.write(id.as_bytes());
        let key = hasher.finish();
        Ok(map.remove(&key).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn basic_store_recall() {
        let m = SqliteMemory::new("/tmp/x").unwrap();
        let e = MemoryEntry { id: "1".into(), category: "c".into(), content: "hi".into(), timestamp: None, namespace: None, score: None };
        m.store("1", &e.content, MemoryCategory::Core, None).await.unwrap();
        let got_list = m.recall("", 10, None, None, None).await.unwrap();
        assert!(!got_list.is_empty());
    }
}
