//! Lattice / Species-kernel integration module for the Mexius gateway.
//!
//! Exposes a `SharedEntity` type (thread-safe, optional `Entity`) and helpers
//! for the three API endpoints:
//!   GET  /api/lattice/top_k       - JSON array of top resonant tokens
//!   POST /api/lattice/init        - load DNA, return plain-text status
//!   POST /api/lattice/inject_word - hash word -> dna_words -> think()

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use species_kernel::Entity;
use lattice_kernel::U64S_PER_VEC;

/// Thread-safe slot holding the (lazily loaded) Entity.
/// `None` until `/api/lattice/init` is called.
pub type SharedEntity = Arc<Mutex<Option<Entity>>>;

/// Create a fresh, empty `SharedEntity`.
pub fn make_entity() -> SharedEntity {
    Arc::new(Mutex::new(None))
}

/// DNA binary path -- WSL-accessible Windows D: drive mount.
const DNA_PATH: &str = "/mnt/d/species_dna.bin";

// --- Autonomous 16 ms pulse -----------------------------------------------

/// Spawn a background tokio task that calls `entity.think(&ZERO)` every 16 ms,
/// applying pure homeostatic decay while the entity is loaded.
pub fn spawn_pulse(entity: SharedEntity) {
    tokio::spawn(async move {
        const ZERO: [u64; U64S_PER_VEC] = [0u64; U64S_PER_VEC];
        let mut interval = tokio::time::interval(Duration::from_millis(16));
        loop {
            interval.tick().await;
            if let Ok(guard) = entity.lock() {
                if let Some(ent) = guard.as_ref() {
                    ent.think(&ZERO);
                }
            }
        }
    });
}

// --- HTTP handlers --------------------------------------------------------

/// POST /api/lattice/init
/// Loads (or reloads) the DNA from DNA_PATH and seats the entity.
/// Returns a plain-text status message.
pub async fn handle_init(entity: &SharedEntity) -> String {
    match Entity::load(PathBuf::from(DNA_PATH)) {
        Ok(ent) => {
            let count = ent.token_count();
            if let Ok(mut guard) = entity.lock() {
                *guard = Some(ent);
            }
            format!("Entity online -- {} tokens loaded", count)
        }
        Err(e) => format!("err:lattice_init:{}", e),
    }
}

/// GET /api/lattice/top_k
/// Returns the top-100 resonant (token_id, distance) pairs as a JSON array.
pub async fn handle_top_k(entity: &SharedEntity) -> serde_json::Value {
    let guard = match entity.lock() {
        Ok(g) => g,
        Err(_) => return serde_json::json!([]),
    };
    match guard.as_ref() {
        None => serde_json::json!([]),
        Some(ent) => {
            let scores = ent.top_k(100);
            serde_json::Value::Array(
                scores
                    .into_iter()
                    .map(|t| serde_json::json!({
                        "token_id": t.token_id,
                        "distance": t.distance,
                    }))
                    .collect(),
            )
        }
    }
}

/// POST /api/lattice/inject_word
/// FNV-1a hash of `word` -> token_id -> DNA vector -> think().
pub async fn handle_inject(entity: &SharedEntity, word: String) {
    let guard = match entity.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if let Some(ent) = guard.as_ref() {
        let token_id = fnv1a_64(word.trim()) as usize;
        let words = ent.dna_words(token_id);
        ent.think(&words);
    }
}

// --- Helpers --------------------------------------------------------------

/// FNV-1a 64-bit hash -- matches the JS `fnv1a64Low` in GhostTerminal.tsx.
#[inline]
fn fnv1a_64(s: &str) -> u64 {
    const OFFSET: u64 = 14_695_981_039_346_656_037;
    const PRIME:  u64 = 1_099_511_628_211;
    let mut h = OFFSET;
    for byte in s.bytes() {
        h ^= byte as u64;
        h  = h.wrapping_mul(PRIME);
    }
    h
}
