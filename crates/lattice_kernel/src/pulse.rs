// ============================================================================
//  pulse.rs
//  The Pulse Thread — decaying Global State Vector + resonance query loop.
//
//  Design:
//   • A background OS thread holds the mutable "state" bit-vector (f32 weights
//     over each of the 10,240 projection dimensions).
//   • Every DECAY_INTERVAL_MS it multiplies the entire vector by DECAY_FACTOR,
//     simulating "Fluid Thinking" / forgetting.
//   • Callers submit a Thought Vector via `submit_thought()`.  The thread XORs
//     it (weighted) into the state, then runs a Hamming search and stores the
//     latest Top-50 in a shared slot.
//   • Every IPC_INTERVAL_MS the shared slot is readable by the FFI layer.
// ============================================================================

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};

use crate::{
    BYTES_PER_VEC, PROJ_DIM,
    DECAY_INTERVAL_MS, DECAY_FACTOR, IPC_INTERVAL_MS,
};
use crate::dna_map::MappedDna;
use crate::hamming::HammingIndex;

// ─── Public types ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct TokenScore {
    pub token_id: u32,
    pub distance: u32,
}

// ─── PulseThread ─────────────────────────────────────────────────────────────

/// Handle to the background Pulse Thread.
pub struct PulseThread {
    shared: Arc<Shared>,
    stop:   Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

struct Shared {
    /// Thought queue: latest submitted thought vector (packed bits).
    thought_in: Mutex<Option<Box<[u8; BYTES_PER_VEC]>>>,
    /// Latest top-K results, updated each search cycle.
    top_k_out:  RwLock<Vec<TokenScore>>,
    /// Current state as f32 logits per projection dimension.
    state:      Mutex<Box<[f32; PROJ_DIM]>>,
}

impl PulseThread {
    /// Spawn the background pulse thread.
    ///
    /// The thread borrows the mapped DNA by raw pointer for its lifetime.
    /// Safety: `dna` must outlive the `PulseThread`.
    pub fn spawn(dna: Arc<MappedDna>) -> Self {
        let shared = Arc::new(Shared {
            thought_in: Mutex::new(None),
            top_k_out:  RwLock::new(Vec::new()),
            state:      Mutex::new(Box::new([0f32; PROJ_DIM])),
        });
        let stop = Arc::new(AtomicBool::new(false));

        let shared2 = Arc::clone(&shared);
        let stop2   = Arc::clone(&stop);

        let handle = std::thread::Builder::new()
            .name("lattice_pulse".into())
            .stack_size(2 * 1024 * 1024) // 2 MiB stack
            .spawn(move || pulse_loop(dna, shared2, stop2))
            .expect("failed to spawn pulse thread");

        Self { shared, stop, handle: Some(handle) }
    }

    /// Submit a new Thought Vector.  The thread will incorporate it on its next tick.
    pub fn submit_thought(&self, vec: Box<[u8; BYTES_PER_VEC]>) {
        *self.shared.thought_in.lock() = Some(vec);
    }

    /// Read the latest Top-K resonant tokens.
    pub fn top_k(&self) -> Vec<TokenScore> {
        self.shared.top_k_out.read().clone()
    }

    /// Overwrite the entire state vector (useful for seeding / resetting).
    pub fn set_state(&self, weights: &[f32; PROJ_DIM]) {
        self.shared.state.lock().copy_from_slice(weights);
    }
}

impl Drop for PulseThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

// ─── The loop ────────────────────────────────────────────────────────────────

fn pulse_loop(dna: Arc<MappedDna>, shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    let index = HammingIndex::new(&dna);

    let decay_interval  = Duration::from_millis(DECAY_INTERVAL_MS);
    let ipc_interval    = Duration::from_millis(IPC_INTERVAL_MS);

    let mut last_decay  = Instant::now();
    let mut last_search = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();

        // ── 1. Decay state vector every 100 ms ──────────────────────────────
        if now.duration_since(last_decay) >= decay_interval {
            let mut st = shared.state.lock();
            for v in st.iter_mut() {
                *v *= DECAY_FACTOR;
            }
            last_decay = now;
        }

        // ── 2. Incorporate any queued thought ───────────────────────────────
        let new_thought = shared.thought_in.lock().take();
        if let Some(tv) = new_thought {
            let mut st = shared.state.lock();
            // For each set bit in the thought vector, boost that dimension.
            for (byte_idx, &byte) in tv.iter().enumerate() {
                for bit in 0..8u8 {
                    if byte & (1 << bit) != 0 {
                        let dim = byte_idx * 8 + bit as usize;
                        if dim < PROJ_DIM {
                            // Additive resonance; capped to avoid saturation
                            st[dim] = (st[dim] + 1.0).min(255.0);
                        }
                    }
                }
            }
        }

        // ── 3. Every ~16 ms: binarize state → Hamming search → update top-K ──
        if now.duration_since(last_search) >= ipc_interval {
            // Binarize: top 10% active bits
            let query_vec = binarize_state(&shared.state.lock());
            let results = index.top_k(&query_vec);
            *shared.top_k_out.write() = results;
            last_search = now;
        }

        // Short sleep to avoid spinning at 100% CPU
        std::thread::sleep(Duration::from_millis(1));
    }
}

// ─── Binarize state → packed bit-vector ─────────────────────────────────────

/// Convert f32 state weights into a BYTES_PER_VEC packed bit-vector.
/// Top `k` = 10% of PROJ_DIM dimensions become 1-bits.
fn binarize_state(state: &[f32; PROJ_DIM]) -> Box<[u8; BYTES_PER_VEC]> {
    let k = PROJ_DIM / 10; // 1 024
    let mut indexed: Vec<(u32, usize)> = state.iter()
        .enumerate()
        .map(|(i, &v)| (v.to_bits(), i))
        .collect();
    // Partial sort: find top-k by value (f32 bits are orderable for non-negatives)
    indexed.select_nth_unstable_by(k - 1, |a, b| b.0.cmp(&a.0));

    let mut out = Box::new([0u8; BYTES_PER_VEC]);
    for &(_, dim) in &indexed[..k] {
        out[dim / 8] |= 1 << (dim % 8);
    }
    out
}
