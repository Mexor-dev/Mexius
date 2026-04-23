// ============================================================================
//  tests/integration.rs
// ============================================================================

use lattice_kernel::{MappedDna, HammingIndex, PulseThread, BYTES_PER_VEC};
use std::sync::Arc;

/// Creates a tiny synthetic DNA in a temp file for tests.
fn make_temp_dna(token_count: usize) -> tempfile::NamedTempFile {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    let mut data = vec![0u8; token_count * BYTES_PER_VEC];
    // Token i has bits set at positions i%BYTES_PER_VEC*8 ..
    for t in 0..token_count {
        for b in 0..128usize {   // 10% of 1280 bytes = 128 bytes with at least one bit
            let idx = t * BYTES_PER_VEC + (b * 10).min(BYTES_PER_VEC - 1);
            data[idx] |= 1 << (t % 8);
        }
    }
    f.write_all(&data).unwrap();
    f
}

#[test]
fn hamming_finds_identical_token() {
    let n = 100;
    let tmp = make_temp_dna(n);
    let dna = Arc::new(MappedDna::open(tmp.path()).unwrap());
    let index = HammingIndex::new(&dna);

    // Query = exact copy of token 7
    let mut query = Box::new([0u8; BYTES_PER_VEC]);
    query.copy_from_slice(dna.token_vec(7));

    let top = index.top_k(&query);
    assert!(!top.is_empty());
    assert_eq!(top[0].token_id, 7, "Closest match must be token 7 itself");
    assert_eq!(top[0].distance, 0, "Hamming distance to itself must be 0");
}

#[test]
fn pulse_thread_returns_results() {
    let n = 200;
    let tmp = make_temp_dna(n);
    let dna = Arc::new(MappedDna::open(tmp.path()).unwrap());
    let pulse = PulseThread::spawn(Arc::clone(&dna));

    let thought = Box::new([0xFFu8; BYTES_PER_VEC]);
    pulse.submit_thought(thought);

    std::thread::sleep(std::time::Duration::from_millis(100));

    let top = pulse.top_k();
    assert!(!top.is_empty(), "Pulse thread must produce results");
}
