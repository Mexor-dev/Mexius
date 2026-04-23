// ============================================================================
//  dna_map.rs
//  Memory-maps species_dna.bin and pins it into physical RAM with VirtualLock.
// ============================================================================

use std::path::Path;
use std::fs::File;
use memmap2::Mmap;

use crate::{BYTES_PER_VEC, DNA_EXPECTED_BYTES};

// ─── Windows VirtualLock / VirtualUnlock ────────────────────────────────────

#[cfg(target_os = "windows")]
mod win {
    use std::ffi::c_void;
    extern "system" {
        pub fn VirtualLock(lpAddress: *const c_void, dwSize: usize) -> i32;
        pub fn VirtualUnlock(lpAddress: *const c_void, dwSize: usize) -> i32;
    }
}

// ─── MappedDna ───────────────────────────────────────────────────────────────

/// Owns the memory-mapped + VirtualLock'd `species_dna.bin`.
pub struct MappedDna {
    _file: File,      // keep file handle alive
    map: Mmap,
    pub token_count: usize,
}

impl MappedDna {
    /// Open, mmap, and (Windows-only) VirtualLock the DNA file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let file = File::open(path.as_ref())
            .map_err(|e| format!("Cannot open DNA file {:?}: {e}", path.as_ref()))?;

        let map = unsafe { Mmap::map(&file) }
            .map_err(|e| format!("mmap failed: {e}"))?;

        let len = map.len();
        if len < DNA_EXPECTED_BYTES {
            return Err(format!(
                "DNA file too small: got {len} bytes, expected at least {DNA_EXPECTED_BYTES}"
            ).into());
        }

        let token_count = len / BYTES_PER_VEC;

        // ── VirtualLock on Windows ──────────────────────────────────────────
        #[cfg(target_os = "windows")]
        {
            let locked = unsafe {
                win::VirtualLock(map.as_ptr() as *const _, len)
            };
            if locked == 0 {
                // Not fatal — OS may deny without elevated privileges.
                // The kernel will still work; data just won't be guaranteed
                // resident in physical RAM at all times.
                eprintln!(
                    "[lattice_kernel] WARNING: VirtualLock failed (error={}). \
                     Run as Administrator or raise working-set quota for guaranteed \
                     cache-resident performance.",
                    unsafe { windows_last_error() }
                );
            } else {
                eprintln!("[lattice_kernel] VirtualLock: {} MiB pinned to physical RAM.",
                    len / 1_048_576);
            }
        }

        Ok(Self { _file: file, map, token_count })
    }

    /// Raw byte slice for one token's bit-vector (0-indexed).
    #[inline(always)]
    pub fn token_vec(&self, idx: usize) -> &[u8] {
        let off = idx * BYTES_PER_VEC;
        &self.map[off..off + BYTES_PER_VEC]
    }

    /// Pointer to the start of all token data.
    #[inline(always)]
    pub fn as_ptr(&self) -> *const u8 {
        self.map.as_ptr()
    }

    /// Total mapped bytes.
    #[inline(always)]
    pub fn len_bytes(&self) -> usize {
        self.map.len()
    }
}

impl Drop for MappedDna {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        unsafe {
            win::VirtualUnlock(self.map.as_ptr() as *const _, self.map.len());
        }
    }
}

// ─── helpers ────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
unsafe fn windows_last_error() -> u32 {
    extern "system" { fn GetLastError() -> u32; }
    GetLastError()
}
