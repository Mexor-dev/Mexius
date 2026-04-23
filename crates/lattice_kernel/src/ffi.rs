// ============================================================================
//  ffi.rs
//  C-ABI surface for Tauri IPC (call from JavaScript via Tauri invoke or
//  via a native sidecar that exposes these symbols).
//
//  Thread safety: all state lives behind Arc<RwLock/Mutex>; the C caller
//  must only call lattice_init() once and lattice_destroy() once.
// ============================================================================

use std::sync::Arc;
use std::ffi::c_char;

use parking_lot::RwLock;

use crate::{BYTES_PER_VEC, TOP_K};
use crate::dna_map::MappedDna;
use crate::pulse::{PulseThread, TokenScore};

// ─── Global singleton ───────────────────────────────────────────────────────

static KERNEL: std::sync::OnceLock<RwLock<Option<KernelState>>> =
    std::sync::OnceLock::new();

struct KernelState {
    _dna:   Arc<MappedDna>,
    pulse:  PulseThread,
}

fn kernel() -> &'static RwLock<Option<KernelState>> {
    KERNEL.get_or_init(|| RwLock::new(None))
}

// ─── Exported symbols ───────────────────────────────────────────────────────

/// Initialize the Lattice Kernel.
///
/// `path` must be a UTF-8, null-terminated path to `species_dna.bin`.
/// Returns 0 on success, non-zero on error (message printed to stderr).
#[no_mangle]
pub extern "C" fn lattice_init(path: *const c_char) -> i32 {
    let path_str = unsafe {
        match std::ffi::CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => { eprintln!("[lattice] lattice_init: invalid UTF-8 path"); return 1; }
        }
    };

    match MappedDna::open(path_str) {
        Ok(dna) => {
            let dna = Arc::new(dna);
            let pulse = PulseThread::spawn(Arc::clone(&dna));
            *kernel().write() = Some(KernelState { _dna: dna, pulse });
            eprintln!("[lattice] Initialized. Vocab={} tokens.", BYTES_PER_VEC);
            0
        }
        Err(e) => {
            eprintln!("[lattice] Init failed: {e}");
            2
        }
    }
}

/// Submit a 1280-byte Thought Vector packed as bits.
///
/// `thought_vec` must point to exactly 1280 bytes.
/// Returns 0 on success, 1 if kernel not initialized.
#[no_mangle]
pub extern "C" fn lattice_submit_thought(thought_vec: *const u8) -> i32 {
    let guard = kernel().read();
    let Some(state) = guard.as_ref() else {
        eprintln!("[lattice] lattice_submit_thought called before lattice_init");
        return 1;
    };

    let slice = unsafe { std::slice::from_raw_parts(thought_vec, BYTES_PER_VEC) };
    let mut boxed = Box::new([0u8; BYTES_PER_VEC]);
    boxed.copy_from_slice(slice);
    state.pulse.submit_thought(boxed);
    0
}

/// Fetch the top-50 resonant tokens.
///
/// `out_buf` must point to a buffer of at least `TOP_K` × `LatticeToken` structs.
/// `LatticeToken` layout (C):
///   struct LatticeToken { uint32_t token_id; uint32_t distance; }  // 8 bytes
///
/// Returns the number of results written (≤ TOP_K), or -1 if not initialized.
#[no_mangle]
pub extern "C" fn lattice_top_k(out_buf: *mut TokenScore, buf_len: u32) -> i32 {
    if out_buf.is_null() { return -1; }
    let guard = kernel().read();
    let Some(state) = guard.as_ref() else { return -1; };

    let results = state.pulse.top_k();
    let n = results.len().min(buf_len as usize).min(TOP_K);
    unsafe {
        let out_slice = std::slice::from_raw_parts_mut(out_buf, n);
        out_slice.copy_from_slice(&results[..n]);
    }
    n as i32
}

/// Destroy the kernel and release all resources.
#[no_mangle]
pub extern "C" fn lattice_destroy() {
    *kernel().write() = None;
}

// ─── Rust-native convenience (for non-FFI callers / tests) ──────────────────

/// High-level handle for Rust consumers (e.g., tests, bench, Tauri sidecar).
pub struct LatticeKernel;

impl LatticeKernel {
    pub fn open(dna_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let dna = Arc::new(MappedDna::open(dna_path)?);
        let pulse = PulseThread::spawn(Arc::clone(&dna));
        *kernel().write() = Some(KernelState { _dna: dna, pulse });
        Ok(())
    }

    pub fn submit_thought(thought: [u8; BYTES_PER_VEC]) {
        let guard = kernel().read();
        if let Some(s) = guard.as_ref() {
            s.pulse.submit_thought(Box::new(thought));
        }
    }

    pub fn top_k() -> Vec<TokenScore> {
        let guard = kernel().read();
        guard.as_ref().map(|s| s.pulse.top_k()).unwrap_or_default()
    }

    pub fn destroy() {
        *kernel().write() = None;
    }
}
