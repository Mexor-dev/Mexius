// ============================================================================
//  hamming.rs
//  SIMD Hamming Distance search:
//    1. AVX-512 VPOPCNTDQ path  (fastest – Zen4 / Sapphire Rapids+)
//    2. AVX2 popcount via VPSHUFB nibble-LUT  (Haswell+)
//    3. Scalar fallback (portable)
// ============================================================================

use crate::{BYTES_PER_VEC, TOP_K, U64S_PER_VEC};
use crate::dna_map::MappedDna;
use crate::pulse::TokenScore;

// ─── Public struct ───────────────────────────────────────────────────────────

/// Wraps the mmap'd DNA and performs bulk Hamming search.
pub struct HammingIndex<'a> {
    dna: &'a MappedDna,
}

impl<'a> HammingIndex<'a> {
    pub fn new(dna: &'a MappedDna) -> Self {
        Self { dna }
    }

    /// Find the top-K tokens with smallest Hamming distance to `query`.
    ///
    /// `query` must be exactly `BYTES_PER_VEC` (1280) bytes.
    pub fn top_k(&self, query: &[u8; BYTES_PER_VEC]) -> Vec<TokenScore> {
        let n = self.dna.token_count;
        let base = self.dna.as_ptr();

        // Heap of (Reverse(distance), token_id) — max-heap bounded to TOP_K
        let mut heap: Vec<TokenScore> = Vec::with_capacity(TOP_K + 1);

        for idx in 0..n {
            let off = idx * BYTES_PER_VEC;
            let token_bytes = unsafe {
                std::slice::from_raw_parts(base.add(off), BYTES_PER_VEC)
            };
            let dist = hamming_distance(query, token_bytes);

            if heap.len() < TOP_K {
                heap.push(TokenScore { token_id: idx as u32, distance: dist });
                if heap.len() == TOP_K {
                    // Turn into max-heap by distance so we can evict worst
                    heap.sort_unstable_by(|a, b| b.distance.cmp(&a.distance));
                }
            } else if dist < heap[0].distance {
                // Replace worst
                heap[0] = TokenScore { token_id: idx as u32, distance: dist };
                // Sift down to maintain max-heap property
                sift_down(&mut heap, 0);
            }
        }

        // Sort ascending by distance for the caller
        heap.sort_unstable_by(|a, b| a.distance.cmp(&b.distance));
        heap
    }
}

// ─── Hamming dispatch ────────────────────────────────────────────────────────

/// Computes Hamming distance (number of differing bits) between two 1280-byte slices.
#[inline(always)]
fn hamming_distance(a: &[u8], b: &[u8]) -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx512vpopcntdq") && is_x86_feature_detected!("avx512f") {
            return unsafe { hamming_avx512(a, b) };
        }
        if is_x86_feature_detected!("avx2") {
            return unsafe { hamming_avx2(a, b) };
        }
    }
    hamming_scalar(a, b)
}

// ─── Scalar fallback ─────────────────────────────────────────────────────────

#[inline]
fn hamming_scalar(a: &[u8], b: &[u8]) -> u32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x ^ y).count_ones()).sum()
}

// ─── AVX2 path (VPSHUFB nibble-LUT) ─────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn hamming_avx2(a: &[u8], b: &[u8]) -> u32 {
    use std::arch::x86_64::*;

    // Nibble popcount LUT: popcount(nibble 0..15)
    let lut = _mm256_set_epi8(
        4,3,3,2,3,2,2,1,3,2,2,1,2,1,1,0,
        4,3,3,2,3,2,2,1,3,2,2,1,2,1,1,0,
    );
    let lo_mask = _mm256_set1_epi8(0x0Fi8);

    let mut acc = _mm256_setzero_si256();
    let chunks = a.len() / 32;

    let pa = a.as_ptr() as *const __m256i;
    let pb = b.as_ptr() as *const __m256i;

    for i in 0..chunks {
        let va = _mm256_loadu_si256(pa.add(i));
        let vb = _mm256_loadu_si256(pb.add(i));
        let xor = _mm256_xor_si256(va, vb);

        let lo  = _mm256_and_si256(xor, lo_mask);
        let hi  = _mm256_and_si256(_mm256_srli_epi16(xor, 4), lo_mask);
        let cnt = _mm256_add_epi8(
            _mm256_shuffle_epi8(lut, lo),
            _mm256_shuffle_epi8(lut, hi),
        );
        // Accumulate into 64-bit lanes via SAD against zero
        acc = _mm256_add_epi64(acc, _mm256_sad_epu8(cnt, _mm256_setzero_si256()));
    }

    // Horizontal sum of four 64-bit accumulators
    let lo128 = _mm256_castsi256_si128(acc);
    let hi128 = _mm256_extracti128_si256(acc, 1);
    let sum128 = _mm_add_epi64(lo128, hi128);
    let sum64  = (_mm_cvtsi128_si64(sum128) + _mm_extract_epi64(sum128, 1)) as u32;

    // Tail bytes (a.len() % 32)
    let tail_start = chunks * 32;
    let tail = hamming_scalar(&a[tail_start..], &b[tail_start..]);

    sum64 + tail
}

// ─── AVX-512 path (VPOPCNTDQ on 64-byte chunks) ──────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f", enable = "avx512bw", enable = "avx512vpopcntdq")]
unsafe fn hamming_avx512(a: &[u8], b: &[u8]) -> u32 {
    use std::arch::x86_64::*;

    let mut acc = _mm512_setzero_si512();
    let chunks = a.len() / 64;

    let pa = a.as_ptr() as *const __m512i;
    let pb = b.as_ptr() as *const __m512i;

    for i in 0..chunks {
        let va  = _mm512_loadu_si512(pa.add(i));
        let vb  = _mm512_loadu_si512(pb.add(i));
        let xor = _mm512_xor_si512(va, vb);
        // VPOPCNTDQ on 64-bit elements
        let cnt = _mm512_popcnt_epi64(xor);
        acc = _mm512_add_epi64(acc, cnt);
    }

    // Reduce 8 × u64 → u32
    let sum = _mm512_reduce_add_epi64(acc) as u32;

    let tail_start = chunks * 64;
    let tail = hamming_scalar(&a[tail_start..], &b[tail_start..]);

    sum + tail
}

// ─── Max-heap helpers ────────────────────────────────────────────────────────

fn sift_down(heap: &mut Vec<TokenScore>, mut i: usize) {
    let n = heap.len();
    loop {
        let mut largest = i;
        let l = 2 * i + 1;
        let r = 2 * i + 2;
        if l < n && heap[l].distance > heap[largest].distance { largest = l; }
        if r < n && heap[r].distance > heap[largest].distance { largest = r; }
        if largest == i { break; }
        heap.swap(i, largest);
        i = largest;
    }
}

// ============================================================================
//  Aligned u64 API — find_top_k
//
//  Operates on 64-word (512-bit / 1280-byte) token vectors viewed as &[u64].
//  Both the mmap'd database (page-aligned ≥ 4096 B) and the caller-supplied
//  query slice must carry ≥ 64-byte pointer alignment so the SIMD paths can
//  issue aligned loads (_mm512_load / _mm256_load) for maximum throughput.
//
//  Vector geometry:
//    160 u64 = 20 × 512-bit chunks   (AVX-512, no tail)
//    160 u64 = 40 × 256-bit chunks   (AVX2,    no tail)
// ============================================================================

/// Find the `k` nearest tokens to `query` by Hamming distance.
///
/// # Arguments
/// * `query` — Exactly `U64S_PER_VEC` (160) `u64` words representing the
///   10,240-bit query vector.  The slice's **base pointer must be 64-byte
///   aligned** so that the AVX-512 and AVX2 paths can use aligned loads.
/// * `dna`   — The memory-mapped DNA buffer (always page-aligned).
/// * `k`     — Number of nearest neighbours to return (e.g. `100`).
///
/// Returns up to `k` [`TokenScore`] values sorted ascending by distance.
pub fn find_top_k(query: &[u64], dna: &MappedDna, k: usize) -> Vec<TokenScore> {
    assert_eq!(
        query.len(), U64S_PER_VEC,
        "query must be exactly {U64S_PER_VEC} u64 words ({} bytes)",
        U64S_PER_VEC * 8,
    );
    debug_assert_eq!(
        query.as_ptr() as usize % 64, 0,
        "query pointer is not 64-byte aligned — aligned SIMD loads require it"
    );

    let n  = dna.token_count;
    let qp = query.as_ptr();
    // mmap base is at least page-aligned (4096 B), so ≥ 64-byte aligned.
    let bp = dna.as_ptr() as *const u64;

    // Bounded max-heap: heap[0] is always the current worst (largest distance).
    let mut heap: Vec<TokenScore> = Vec::with_capacity(k + 1);

    for idx in 0..n {
        let tp   = unsafe { bp.add(idx * U64S_PER_VEC) };
        let dist = hamming_distance_u64(qp, tp);

        if heap.len() < k {
            heap.push(TokenScore { token_id: idx as u32, distance: dist });
            if heap.len() == k {
                // Heapify: maintain max-heap invariant so we can evict the worst.
                heap.sort_unstable_by(|a, b| b.distance.cmp(&a.distance));
            }
        } else if dist < heap[0].distance {
            heap[0] = TokenScore { token_id: idx as u32, distance: dist };
            sift_down(&mut heap, 0);
        }
    }

    // Sort the final result ascending by distance for the caller.
    heap.sort_unstable_by(|a, b| a.distance.cmp(&b.distance));
    heap
}

// ─── Runtime dispatch (u64 pointers) ─────────────────────────────────────────

#[inline(always)]
fn hamming_distance_u64(a: *const u64, b: *const u64) -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx512vpopcntdq") && is_x86_feature_detected!("avx512f") {
            return unsafe { hamming_avx512_aligned(a, b) };
        }
        if is_x86_feature_detected!("avx2") {
            return unsafe { hamming_avx2_aligned(a, b) };
        }
    }
    unsafe { hamming_scalar_u64(a, b) }
}

// ─── Scalar u64 fallback ──────────────────────────────────────────────────────

/// Portable scalar Hamming distance over 160 u64 words.
#[inline]
unsafe fn hamming_scalar_u64(a: *const u64, b: *const u64) -> u32 {
    let mut acc: u32 = 0;
    for i in 0..U64S_PER_VEC {
        acc += (*a.add(i) ^ *b.add(i)).count_ones();
    }
    acc
}

// ─── AVX2 aligned path ───────────────────────────────────────────────────────

/// Hamming distance using AVX2 VPSHUFB nibble-LUT with **32-byte aligned** loads.
///
/// 160 u64 = 40 × 256-bit registers — computed in exactly 40 iterations, no tail.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn hamming_avx2_aligned(a: *const u64, b: *const u64) -> u32 {
    use std::arch::x86_64::*;

    // Fallback to unaligned loads if not 32-byte aligned
    if (a as usize) % 32 != 0 || (b as usize) % 32 != 0 {
        return hamming_avx2_unaligned(a, b);
    }

    // Nibble popcount lookup table: popcnt(nibble 0..=15)
    let lut = _mm256_set_epi8(
        4,3,3,2,3,2,2,1,3,2,2,1,2,1,1,0,
        4,3,3,2,3,2,2,1,3,2,2,1,2,1,1,0,
    );
    let lo_mask = _mm256_set1_epi8(0x0Fi8);
    let mut acc = _mm256_setzero_si256();

    // 160 u64 / 4 u64-per-ymm = 40 chunks (exact, no tail)
    const CHUNKS: usize = U64S_PER_VEC / 4;
    let pa = a as *const __m256i;
    let pb = b as *const __m256i;

    for i in 0..CHUNKS {
        // 32-byte aligned load (pointers are ≥ 64-byte aligned)
        let va  = _mm256_load_si256(pa.add(i));
        let vb  = _mm256_load_si256(pb.add(i));
        let xor = _mm256_xor_si256(va, vb);

        // Nibble-LUT popcount
        let lo  = _mm256_and_si256(xor, lo_mask);
        let hi  = _mm256_and_si256(_mm256_srli_epi16(xor, 4), lo_mask);
        let cnt = _mm256_add_epi8(
            _mm256_shuffle_epi8(lut, lo),
            _mm256_shuffle_epi8(lut, hi),
        );
        // SAD accumulates byte popcounts into four u64 lanes
        acc = _mm256_add_epi64(acc, _mm256_sad_epu8(cnt, _mm256_setzero_si256()));
    }

    // Horizontal reduce: four u64 lanes → u32
    let lo128  = _mm256_castsi256_si128(acc);
    let hi128  = _mm256_extracti128_si256(acc, 1);
    let sum128 = _mm_add_epi64(lo128, hi128);
    (_mm_cvtsi128_si64(sum128) + _mm_extract_epi64(sum128, 1)) as u32
}

// ─── AVX-512 aligned path ─────────────────────────────────────────────────────

/// Hamming distance using `_mm512_xor_si512` + `_mm512_popcnt_epi64` (VPOPCNTDQ)
/// with **64-byte aligned** loads.
///
/// 160 u64 = 20 × 512-bit registers — computed in exactly 20 iterations, no tail.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f", enable = "avx512bw", enable = "avx512vpopcntdq")]
unsafe fn hamming_avx512_aligned(a: *const u64, b: *const u64) -> u32 {
    use std::arch::x86_64::*;

    // Fallback to unaligned loads if not 64-byte aligned
    if (a as usize) % 64 != 0 || (b as usize) % 64 != 0 {
        return hamming_avx512_unaligned(a, b);
    }

    // 160 u64 / 8 u64-per-zmm = 20 chunks (exact, no tail)
    const CHUNKS: usize = U64S_PER_VEC / 8;
    let pa = a as *const __m512i;
    let pb = b as *const __m512i;
    let mut acc = _mm512_setzero_si512();

    for i in 0..CHUNKS {
        // 64-byte aligned load
        let va  = _mm512_load_si512(pa.add(i));
        let vb  = _mm512_load_si512(pb.add(i));
        // XOR then VPOPCNTDQ on each 64-bit lane
        let xor = _mm512_xor_si512(va, vb);
        let cnt = _mm512_popcnt_epi64(xor);
        acc = _mm512_add_epi64(acc, cnt);
    }

    // Horizontal reduce: eight u64 lanes → u32
    _mm512_reduce_add_epi64(acc) as u32
}

// Unaligned AVX2 fallback
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn hamming_avx2_unaligned(a: *const u64, b: *const u64) -> u32 {
    use std::arch::x86_64::*;
    let lut = _mm256_set_epi8(
        4,3,3,2,3,2,2,1,3,2,2,1,2,1,1,0,
        4,3,3,2,3,2,2,1,3,2,2,1,2,1,1,0,
    );
    let lo_mask = _mm256_set1_epi8(0x0Fi8);
    let mut acc = _mm256_setzero_si256();
    const CHUNKS: usize = U64S_PER_VEC / 4;
    let pa = a as *const __m256i;
    let pb = b as *const __m256i;
    for i in 0..CHUNKS {
        let va  = _mm256_loadu_si256(pa.add(i));
        let vb  = _mm256_loadu_si256(pb.add(i));
        let xor = _mm256_xor_si256(va, vb);
        let lo  = _mm256_and_si256(xor, lo_mask);
        let hi  = _mm256_and_si256(_mm256_srli_epi16(xor, 4), lo_mask);
        let cnt = _mm256_add_epi8(
            _mm256_shuffle_epi8(lut, lo),
            _mm256_shuffle_epi8(lut, hi),
        );
        acc = _mm256_add_epi64(acc, _mm256_sad_epu8(cnt, _mm256_setzero_si256()));
    }
    let lo128  = _mm256_castsi256_si128(acc);
    let hi128  = _mm256_extracti128_si256(acc, 1);
    let sum128 = _mm_add_epi64(lo128, hi128);
    (_mm_cvtsi128_si64(sum128) + _mm_extract_epi64(sum128, 1)) as u32
}

// Unaligned AVX-512 fallback
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f", enable = "avx512bw", enable = "avx512vpopcntdq")]
unsafe fn hamming_avx512_unaligned(a: *const u64, b: *const u64) -> u32 {
    use std::arch::x86_64::*;
    const CHUNKS: usize = U64S_PER_VEC / 8;
    let pa = a as *const __m512i;
    let pb = b as *const __m512i;
    let mut acc = _mm512_setzero_si512();
    for i in 0..CHUNKS {
        let va  = _mm512_loadu_si512(pa.add(i));
        let vb  = _mm512_loadu_si512(pb.add(i));
        let xor = _mm512_xor_si512(va, vb);
        let cnt = _mm512_popcnt_epi64(xor);
        acc = _mm512_add_epi64(acc, cnt);
    }
    _mm512_reduce_add_epi64(acc) as u32
}
