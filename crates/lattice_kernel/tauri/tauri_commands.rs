// tauri_commands.rs
// Drop this file into your Tauri `src-tauri/src/` and register the commands
// in `tauri::Builder::default().invoke_handler(...)`.

use std::ffi::CString;
use serde::Serialize;
use lattice_kernel::ffi::{LatticeKernel};
use lattice_kernel::pulse::TokenScore;
use lattice_kernel::BYTES_PER_VEC;

#[derive(Serialize)]
pub struct TauriTokenScore {
    pub token_id: u32,
    pub distance: u32,
}

impl From<TokenScore> for TauriTokenScore {
    fn from(t: TokenScore) -> Self {
        Self { token_id: t.token_id, distance: t.distance }
    }
}

#[tauri::command]
pub fn lattice_init(path: String) -> Result<(), String> {
    LatticeKernel::open(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn lattice_thought(bits: Vec<u8>) -> Result<(), String> {
    if bits.len() != BYTES_PER_VEC {
        return Err(format!("Expected {BYTES_PER_VEC} bytes, got {}", bits.len()));
    }
    let mut arr = [0u8; BYTES_PER_VEC];
    arr.copy_from_slice(&bits);
    LatticeKernel::submit_thought(arr);
    Ok(())
}

#[tauri::command]
pub fn lattice_top_k() -> Vec<TauriTokenScore> {
    LatticeKernel::top_k().into_iter().map(Into::into).collect()
}

#[tauri::command]
pub fn lattice_destroy() {
    LatticeKernel::destroy();
}
