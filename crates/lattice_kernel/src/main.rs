// ============================================================================
//  main.rs — Benchmark / smoke-test binary (`lattice_bench`)
// ============================================================================

use std::time::Instant;
use lattice_kernel::{
    MappedDna, HammingIndex, PulseThread,
    BYTES_PER_VEC, TOP_K,
};
use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dna_path = args.get(1)
        .map(|s| s.as_str())
        .unwrap_or(r"D:\species_dna.bin");

    println!("=== Lattice Kernel Bench ===");
    println!("Opening DNA: {dna_path}");

    let t0 = Instant::now();
    let dna = Arc::new(MappedDna::open(dna_path).expect("Failed to open species_dna.bin"));
    println!("Mapped {} tokens in {:.1}ms", dna.token_count, t0.elapsed().as_secs_f64() * 1000.0);

    // ── Hamming search benchmark ───────────────────────────────────────────
    let index = HammingIndex::new(&dna);

    // Build a deterministic query: alternating bits (10% density)
    let mut query = Box::new([0u8; BYTES_PER_VEC]);
    for i in (0..BYTES_PER_VEC).step_by(10) {
        query[i] = 0b0000_0001;
    }

    println!("Warming up Hamming search...");
    let _ = index.top_k(&query);  // warm up CPU caches

    let runs = 20;
    let t1 = Instant::now();
    for _ in 0..runs {
        std::hint::black_box(index.top_k(&query));
    }
    let elapsed_ms = t1.elapsed().as_secs_f64() * 1000.0 / runs as f64;
    println!("Hamming search ({} tokens, {runs} runs): {elapsed_ms:.2} ms/search",
        dna.token_count);

    // ── Pulse Thread test ──────────────────────────────────────────────────
    println!("Spawning Pulse Thread...");
    let pulse = PulseThread::spawn(Arc::clone(&dna));

    pulse.submit_thought(query.clone());

    std::thread::sleep(std::time::Duration::from_millis(50));

    let top = pulse.top_k();
    println!("Pulse Thread Top-{TOP_K} (sample after 50ms):");
    for ts in top.iter().take(10) {
        println!("  token_id={:>6}  hamming={}", ts.token_id, ts.distance);
    }

    println!("Done.");
}
