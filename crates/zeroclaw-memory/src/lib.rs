use serde::Serialize;
use std::path::PathBuf;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub enum MemoryCategory {
    Core,
    Daily,
    Conversation,
    Custom(String),
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryCategory::Core => write!(f, "core"),
            MemoryCategory::Daily => write!(f, "daily"),
            MemoryCategory::Conversation => write!(f, "conversation"),
            MemoryCategory::Custom(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MemoryEntry {
    pub key: String,
    pub content: String,
    pub category: MemoryCategory,
    pub session_id: Option<String>,
    pub timestamp: Option<i64>,
    // Optional fields used by the gateway when backed by a vector DB
    pub id: Option<String>,
    pub score: Option<f64>,
    pub namespace: Option<String>,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone)]
pub struct SqliteMemory {
    path: PathBuf,
    store: Arc<Mutex<Vec<MemoryEntry>>>,
}

impl SqliteMemory {
    pub fn new<P: AsRef<Path>>(p: P) -> Result<Self, Error> {
        Ok(SqliteMemory { path: p.as_ref().to_path_buf(), store: Arc::new(Mutex::new(Vec::new())) })
    }

    pub async fn recall(&self, _query: &str, _limit: usize, _a: Option<usize>, _b: Option<usize>, _c: Option<usize>) -> Result<Vec<MemoryEntry>, Error> {
        let s = self.store.lock().unwrap();
        Ok(s.clone())
    }

    pub async fn list(&self, _a: Option<usize>, _b: Option<usize>) -> Result<Vec<MemoryEntry>, Error> {
        let s = self.store.lock().unwrap();
        Ok(s.clone())
    }

    pub async fn store(&self, key: &str, content: &str, category: MemoryCategory, session: Option<&str>) -> Result<(), Error> {
        let mut s = self.store.lock().unwrap();
        // Use key as a fallback id for vector_id; score/namespace unknown in this stub
        s.push(MemoryEntry {
            key: key.to_string(),
            content: content.to_string(),
            category,
            session_id: session.map(|x| x.to_string()),
            timestamp: None,
            id: Some(key.to_string()),
            score: None,
            namespace: None,
        });
        Ok(())
    }

    pub async fn forget(&self, key: &str) -> Result<bool, Error> {
        let mut s = self.store.lock().unwrap();
        let orig = s.len();
        s.retain(|e| { if let Some(ref id) = e.id { id != key } else { e.key != key } });
        Ok(s.len() != orig)
    }
}

// Manual Serialize implementations to avoid proc-macro derive in the stub
impl serde::ser::Serialize for MemoryCategory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            MemoryCategory::Core => "core".to_string(),
            MemoryCategory::Daily => "daily".to_string(),
            MemoryCategory::Conversation => "conversation".to_string(),
            MemoryCategory::Custom(v) => v.clone(),
        };
        serializer.serialize_str(&s)
    }
}

impl serde::ser::Serialize for MemoryEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut st = serializer.serialize_struct("MemoryEntry", 8)?;
        st.serialize_field("key", &self.key)?;
        st.serialize_field("content", &self.content)?;
        st.serialize_field("category", &self.category.to_string())?;
        st.serialize_field("session_id", &self.session_id)?;
        st.serialize_field("timestamp", &self.timestamp)?;
        st.serialize_field("id", &self.id)?;
        st.serialize_field("score", &self.score)?;
        st.serialize_field("namespace", &self.namespace)?;
        st.end()
    }
}
