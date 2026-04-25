use once_cell::sync::Lazy;
use std::collections::VecDeque;
use parking_lot::RwLock;
use serde::{Serialize, Deserialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone)]
pub struct MemoryFragment {
    pub id: String,
    pub label: String,
    // Renamed to `text_chunk` to match UI expectation
    pub text_chunk: String,
    // Optional vector id and distance (if available from LanceDB)
    pub vector_id: Option<String>,
    pub distance: Option<f64>,
    pub ts: String,
}

static PINNED: Lazy<RwLock<VecDeque<MemoryFragment>>> = Lazy::new(|| RwLock::new(VecDeque::new()));
static AUDIT: Lazy<RwLock<VecDeque<Value>>> = Lazy::new(|| RwLock::new(VecDeque::new()));

pub async fn add_fragment(f: MemoryFragment) {
    let mut lock = PINNED.write();
    if lock.len() >= 1000 {
        lock.pop_front();
    }
    lock.push_back(f);
}

pub async fn top_pinned(n: usize) -> Vec<MemoryFragment> {
    let lock = PINNED.read();
    let v: Vec<MemoryFragment> = lock.iter().rev().take(n).cloned().collect();
    v
}

pub async fn add_audit_event(ev: Value) {
    let mut lock = AUDIT.write();
    if lock.len() >= 2000 {
        lock.pop_front();
    }
    lock.push_back(ev);
}

pub async fn last_audit_events(n: usize) -> Vec<Value> {
    let lock = AUDIT.read();
    lock.iter().rev().take(n).cloned().collect()
}
