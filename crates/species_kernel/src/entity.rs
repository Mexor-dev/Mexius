// ============================================================================
//  entity.rs — species_kernel
//
//  Entity: a living, Lattice-Born cognitive agent.
//
//  Structure
//  ─────────
//  ┌─────────────────────────────────────────────────────────────────────┐
//  │ Entity                                                              │
//  │                                                                     │
//  │  dna   : Arc<MappedDna>   — read-only 151 k-token DNA database     │
//  │                             (mmap'd + VirtualLock'd in MappedDna)   │
//  │  state : RwLock<StateBuf> — mutable 10,240-bit thought state        │
//  │                             backed by an anonymous mmap so a second  │
//  │                             VirtualLock can pin it in physical RAM   │
//  └─────────────────────────────────────────────────────────────────────┘
//
//  Think cycle (per 64-bit word)
//  ──────────────────────────────
//  mask  = !(rng & rng & rng)           ~87.5 % bits set  → sparse erasure
//  noise =  rng & rng & rng & rng       ~6.25 % bits set  → stochastic drift
//
//  state[i] = (state[i] & mask) ^ noise  ← homeostatic decay
//  state[i] ^= input[i]                  ← superposition / bundle
//
//  The RwLock lets the Tauri UI snapshot or query the state concurrently
//  while think() holds the write lock for at most one tight inner loop.
// ============================================================================

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use memmap2::MmapMut;
use parking_lot::RwLock;

use lattice_kernel::{MappedDna, U64S_PER_VEC, find_top_k};
use lattice_kernel::pulse::TokenScore;

// ─── Windows VirtualLock / VirtualUnlock ────────────────────────────────────
//
//  Mirroring the pattern in lattice_kernel::dna_map — raw "system" FFI keeps
//  the dependency tree lean while giving us full control over error handling.

#[cfg(target_os = "windows")]
mod win {
    use std::ffi::c_void;
    extern "system" {
        pub fn VirtualLock  (lpAddress: *const c_void, dwSize: usize) -> i32;
        pub fn VirtualUnlock(lpAddress: *const c_void, dwSize: usize) -> i32;
    }
}

// ─── StateBuf ────────────────────────────────────────────────────────────────

/// Page-aligned anonymous mmap that owns the mutable 10,240-bit thought state.
///
/// Using an anonymous mmap (rather than a stack or heap array) gives us:
/// * guaranteed ≥ page-alignment (4096 B) so `VirtualLock` accepts the range
/// * OS-zero-initialised memory on allocation
/// * a clean `Drop` for the paired `VirtualUnlock`
#[repr(C, align(64))]
pub struct AlignedState(pub [u64; U64S_PER_VEC]);

struct StateBuf {
    mmap: MmapMut,
}


// SAFETY: MmapMut wraps a raw pointer, but exclusive mutation is always
// serialised through the write-lock on Entity::state.  `Sync` is provided
// by the RwLock wrapper itself.
unsafe impl Send for StateBuf {}

impl StateBuf {
    /// Allocate one OS page (4096 B ≥ 1280 B needed), then VirtualLock it.
    fn new() -> Result<Self> {
        const PAGE: usize = 4096;
        let len = PAGE.max(U64S_PER_VEC * 8); // 4096 always wins here

        let mmap = MmapMut::map_anon(len)?;

        #[cfg(target_os = "windows")]
        {
            let ok = unsafe { win::VirtualLock(mmap.as_ptr() as *const _, len) };
            if ok == 0 {
                tracing::warn!(
                    "[species_kernel] VirtualLock on state buffer failed — \
                     run as Administrator or raise the working-set quota for \
                     guaranteed cache-resident performance."
                );
            }
        }

        Ok(Self { mmap })
    }

    /// Read-only view of the 160 × u64 thought state.
    ///
    /// # Safety
    /// * `mmap` is page-aligned → satisfies `u64`'s 8-byte alignment requirement.
    /// * `mmap.len()` ≥ `U64S_PER_VEC * 8` = 1280 bytes.
    /// * Memory is OS-zero-initialised; all bit patterns are valid for `u64`.
    #[inline(always)]
    fn words(&self) -> &[u64; U64S_PER_VEC] {
        unsafe { &(*(self.mmap.as_ptr() as *const AlignedState)).0 }
    }

    /// Mutable view of the 160 × u64 thought state.
    ///
    /// # Safety
    /// Exclusive access is guaranteed by the write-lock held in `Entity::think`.
    #[inline(always)]
    fn words_mut(&mut self) -> &mut [u64; U64S_PER_VEC] {
        unsafe { &mut (*(self.mmap.as_mut_ptr() as *mut AlignedState)).0 }
    }
}

impl Drop for StateBuf {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        unsafe {
            win::VirtualUnlock(self.mmap.as_ptr() as *const _, self.mmap.len());
        }
    }
}

// ─── Entity ──────────────────────────────────────────────────────────────────

/// A Lattice-Born cognitive entity.
///
/// Maintains a continuously-evolving 10,240-bit thought state.  Each call to
/// [`think`] applies homeostatic decay (preventing fixation) then bundles the
/// new input through XOR superposition.
///
/// The [`RwLock`] lets the Tauri UI call [`read_state`] or [`top_k`] from
/// the command thread while [`think`] runs on any other thread.
pub struct Entity {
    dna:   Arc<MappedDna>,
    state: RwLock<StateBuf>,
}

impl Entity {
    // ── Construction ─────────────────────────────────────────────────────────

    /// Open `path` as the DNA database and allocate a zeroed thought state.
    ///
    /// * `MappedDna::open` memory-maps the file and VirtualLocks the DNA pages.
    /// * `StateBuf::new` allocates an anonymous mmap and VirtualLocks the state
    ///   page so the hot 1280-byte buffer stays resident even under OS pressure.
    pub fn load(path: PathBuf) -> Result<Self> {
        let path_str = path.display().to_string();

        let dna = MappedDna::open(&path)
            .map_err(|e| anyhow::anyhow!("Cannot open DNA at {path_str}: {e}"))?;
        let dna = Arc::new(dna);

        let state = StateBuf::new()?;

        tracing::info!(
            path   = %path_str,
            tokens = dna.token_count,
            "Entity loaded",
        );

        Ok(Self {
            dna,
            state: RwLock::new(state),
        })
    }

    // ── Cognitive loop ───────────────────────────────────────────────────────

    /// Apply one homeostatic-decay + superposition cycle.
    ///
    /// # Algorithm (per 64-bit word `i`)
    ///
    /// ```text
    /// // Homeostatic decay — prevents the entity from fixating
    /// mask     = !(rng & rng & rng)         // ~87.5 % bits set
    /// noise    =  rng & rng & rng & rng     // ~6.25 % bits set
    /// state[i] = (state[i] & mask) ^ noise
    ///
    /// // Superposition / bundle — integrate new input
    /// state[i] ^= input[i]
    /// ```
    ///
    /// The decay phase selectively zeroes ~12.5 % of bits and randomly flips
    /// ~6.25 % so the thought state drifts away from any local attractor.
    /// The bundle phase XOR-folds `input` onto the decayed state: bits that
    /// agree cancel (reinforce), bits that differ mix, biasing the state toward
    /// the new input without hard-overwriting accumulated context.
    ///
    /// `input` must be exactly [`U64S_PER_VEC`] (160) words.
    pub fn think(&self, input: &[u64; U64S_PER_VEC]) {
        let mut rng   = Xorshift64::seeded();
        let mut guard = self.state.write();
        let words     = guard.words_mut();

        for i in 0..U64S_PER_VEC {
            // ── Homeostatic decay ─────────────────────────────────────────────
            // Triple-AND: E[popcount] = 64 × 0.5³ ≈ 8 bits set per word
            // Inverted mask ≈ 87.5 % bits set → ~12.5 % of state bits cleared.
            let decay_zeros = rng.next() & rng.next() & rng.next();
            let mask  = !decay_zeros;

            // Quad-AND: E[popcount] = 64 × 0.5⁴ = 4 bits set per word (~6.25 %)
            let noise = rng.next() & rng.next() & rng.next() & rng.next();

            words[i] = (words[i] & mask) ^ noise;

            // ── Superposition / bundle ────────────────────────────────────────
            // XOR merges input onto the decayed state.  Matching bits cancel
            // (net zero contribution), differing bits blend both signals.
            words[i] ^= input[i];
        }
    }

    // ── State access ─────────────────────────────────────────────────────────

    /// Non-blocking snapshot of the current thought state.
    ///
    /// Acquires the read-lock, copies 1280 bytes, then releases immediately.
    /// Multiple callers (UI, logging) can snapshot concurrently.
    pub fn read_state(&self) -> Box<[u64; U64S_PER_VEC]> {
        let guard = self.state.read();
        let mut out = Box::new([0u64; U64S_PER_VEC]);
        out.copy_from_slice(guard.words());
        out
    }

    /// Return the `k` DNA tokens closest to the current thought state.
    ///
    /// Uses SIMD-accelerated Hamming search (`find_top_k` in `lattice_kernel`:
    /// AVX-512 VPOPCNTDQ → AVX2 VPSHUFB nibble-LUT → scalar fallback).
    ///
    /// The read-lock is released as soon as the 1280-byte snapshot is taken,
    /// so `think()` is not blocked for the duration of the O(n) search.
    pub fn top_k(&self, k: usize) -> Vec<TokenScore> {
        let snap = self.read_state();
        find_top_k(snap.as_ref(), &self.dna, k)
    }

    /// Look up the DNA bit-vector for token `token_id % token_count` and
    /// return it as 160 little-endian u64 words ready for [`think`].
    ///
    /// Used by `inject_word` to translate a hashed word into a bundle-ready
    /// thought vector drawn directly from the trained embedding space.
    pub fn dna_words(&self, token_id: usize) -> [u64; U64S_PER_VEC] {
        let idx   = token_id % self.dna.token_count;
        let bytes = self.dna.token_vec(idx);
        let mut out = [0u64; U64S_PER_VEC];
        for (i, chunk) in bytes.chunks_exact(8).enumerate() {
            out[i] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        out
    }

    /// Number of tokens in the loaded DNA database.
    #[inline(always)]
    pub fn token_count(&self) -> usize { self.dna.token_count }
}

// ─── Xorshift64 PRNG ─────────────────────────────────────────────────────────

/// Minimal xorshift64 PRNG seeded per-call from the CPU timestamp counter.
///
/// Using RDTSC (rather than shared atomic state) means:
/// * no inter-thread contention
/// * every `think()` call gets a unique seed without a mutex
/// * the PRNG is consumed within one call, so period length is irrelevant
struct Xorshift64(u64);

impl Xorshift64 {
    /// Capture the current RDTSC value as the seed.
    ///
    /// Falls back to `SystemTime` sub-second nanos on non-x86_64 targets.
    /// The seed is forced non-zero (xorshift is undefined for 0).
    #[cold]
    fn seeded() -> Self {
        let seed = {
            #[cfg(target_arch = "x86_64")]
            {
                // SAFETY: _rdtsc has no side-effects and is available on all x86_64 CPUs.
                unsafe { std::arch::x86_64::_rdtsc() }
            }
            #[cfg(not(target_arch = "x86_64"))]
            {
                use std::time::{SystemTime, UNIX_EPOCH};
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.subsec_nanos() as u64)
                    .unwrap_or(0xDEAD_BEEF_CAFE_BABE)
            }
        };
        Self(seed | 1) // guarantee non-zero seed
    }

    /// Advance the state and return the next pseudo-random u64.
    #[inline(always)]
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}
