// ============================================================================
//  lattice_kernel / lib.rs
//  The RAM-Lattice Kernel for Semantic DNA lookup.
//
//  Architecture:
//    ┌──────────────────────────────────────────────────────────────────┐
//    │  LatticeKernel (main public struct)                              │
//    │                                                                  │
//    │  • MappedDNA      – mmap'd + VirtualLock'd species_dna.bin      │
//    │  • HammingIndex   – SIMD Hamming search (AVX-512 → AVX2 → safe) │
//    │  • PulseThread    – background thread, decaying state vector     │
//    │  • FFI surface    – C-ABI exports for Tauri IPC                  │
//    └──────────────────────────────────────────────────────────────────┘
// ============================================================================

#![allow(clippy::missing_safety_doc)]

pub mod dna_map;
pub mod hamming;
pub mod pulse;
pub mod ffi;

pub use dna_map::MappedDna;
pub use hamming::{HammingIndex, find_top_k};
pub use pulse::{PulseThread, TokenScore};
pub use ffi::*;

/// Number of projected bits per token (ceil(10240/8) = 1280 bytes).
pub const BYTES_PER_VEC: usize = 1280;
/// Number of u64 words per token vector (1280 / 8 = 160).
pub const U64S_PER_VEC: usize = BYTES_PER_VEC / 8;  // 160
/// Projection dimension.
pub const PROJ_DIM: usize = BYTES_PER_VEC * 8;   // 10 240
/// Number of tokens in the vocabulary.
pub const VOCAB_SIZE: usize = 151_936;
/// Expected minimum file size.
pub const DNA_EXPECTED_BYTES: usize = VOCAB_SIZE * BYTES_PER_VEC; // 194_478_080

/// Top-K resonant tokens returned per query.
pub const TOP_K: usize = 50;
/// State vector decay interval (ms).
pub const DECAY_INTERVAL_MS: u64 = 100;
/// State vector decay factor per interval (multiplied each tick).
pub const DECAY_FACTOR: f32 = 0.97;
/// IPC poll interval target (ms).
pub const IPC_INTERVAL_MS: u64 = 16;
