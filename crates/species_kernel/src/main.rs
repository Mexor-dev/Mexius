use std::path::PathBuf;
use species_kernel::Entity;
use lattice_kernel::U64S_PER_VEC;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let dna = std::env::args().nth(1)
        .unwrap_or_else(|| r"D:\species_dna.bin".to_string());

    let entity = Entity::load(PathBuf::from(&dna))?;

    println!("Entity online. Submitting test thought...");
    // Build a sparse test thought: set bit 0 of every 10th byte → u64 words.
    let mut bytes = vec![0u8; U64S_PER_VEC * 8];
    for i in (0..bytes.len()).step_by(10) { bytes[i] = 0b0000_0001; }
    let thought = bytes_to_words(&bytes);
    entity.think(&thought);

    let top = entity.top_k(10);
    println!("Top {} resonant tokens:", top.len());
    for t in &top {
        println!("  token {:>6}  hamming={}", t.token_id, t.distance);
    }
    Ok(())
}

/// Reinterpret a 1280-byte slice as 160 little-endian u64 words.
fn bytes_to_words(bytes: &[u8]) -> [u64; U64S_PER_VEC] {
    assert_eq!(bytes.len(), U64S_PER_VEC * 8);
    let mut out = [0u64; U64S_PER_VEC];
    for (i, chunk) in bytes.chunks_exact(8).enumerate() {
        out[i] = u64::from_le_bytes(chunk.try_into().unwrap());
    }
    out
}
