use std::time::Instant;
use mexius_memory::{SqliteMemory, MemoryCategory};

#[tokio::test(flavor = "multi_thread")]
async fn live_audit() {
    // Create in-memory-style store
    let m = SqliteMemory::new("/tmp/x").unwrap();

    // Insert N entries to ensure measurable work
    let n: usize = 10_000;
    for i in 0..n {
        let id = format!("id{}", i);
        let content = format!("payload {} - {} bytes", i, 256);
        m.store(&id, &content, MemoryCategory::Custom("test".into()), None).await.unwrap();
    }

    // Warmup
    let _ = m.recall("", 10, None, None, None).await.unwrap();

    // Measure multiple recall runs
    let repeats = 50usize;
    let mut times = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let now = Instant::now();
        let _res = m.recall("", 10, None, None, None).await.unwrap();
        times.push(now.elapsed());
    }

    let total_us: u128 = times.iter().map(|d| d.as_micros() as u128).sum();
    let avg_us = total_us / (times.len() as u128);
    println!("Inserted {} entries. Avg recall = {} µs ({} ms)", n, avg_us, (avg_us as f64) / 1000.0);

    // Show resident set size (VmRSS) to indicate RAM residency
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                println!("{}", line);
            }
        }
    }

    // Assert average less than 50ms (50_000 µs)
    assert!(avg_us < 50_000, "Average recall >= 50ms ({} µs)", avg_us);
}
