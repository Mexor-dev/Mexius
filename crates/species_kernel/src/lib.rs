//! species_kernel — Lattice-Born entity engine.
//!
//! Owns the high-level intelligence loop:
//!   • loads species_dna.bin via lattice_kernel::MappedDna
//!   • drives the PulseThread (decay + Hamming search)
//!   • exposes an Entity API that the Tauri UI commands call

pub mod entity;
pub use entity::Entity;
