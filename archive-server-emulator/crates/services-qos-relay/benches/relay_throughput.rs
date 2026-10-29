//! # ProtoTunnel UDP Relay SessionCipher Throughput Benchmark
//!
//! Validates that `SessionCipher` achieves p99 < 200 µs per datagram
//! across 10,000 datagram iterations, completely avoiding the ARC4-per-packet
//! RTT collapse (34ms -> 1300ms) seen in legacy revivals.

use std::time::{Duration, Instant};
use services_qos_relay::relay::SessionCipher;

fn main() {
    println!("===============================================================================");
    println!(" ProtoTunnel UDP Relay: SessionCipher Throughput Benchmark (10,000 Datagrams)  ");
    println!(" Target Invariant: p99 < 200 µs per datagram                                   ");
    println!("===============================================================================");

    // Initialize session cipher once per match per peer session
    let match_key = b"FIFA22_NETGAMEDIST_CANONICAL_KEY_FROSTBITE3";
    let cipher = SessionCipher::new(1001, 1, match_key);

    let iterations = 10_000usize;
    let mut latencies = Vec::with_capacity(iterations);

    // Warm-up run (100 packets)
    let mut warmup_packet = [0x5Au8; 128];
    for _ in 0..100 {
        cipher.process_datagram(&mut warmup_packet);
    }

    // Benchmark run
    let mut packet = [0xA5u8; 128]; // Standard 128-byte 60Hz controller frame
    let bench_start = Instant::now();

    for seq in 0..iterations {
        // Embed sequence into packet
        packet[0..4].copy_from_slice(&(seq as u32).to_be_bytes());

        let t0 = Instant::now();
        cipher.process_datagram(&mut packet);
        let elapsed = t0.elapsed();

        latencies.push(elapsed);
    }

    let total_elapsed = bench_start.elapsed();

    // Compute statistics
    latencies.sort();
    let p50 = latencies[iterations * 50 / 100];
    let p90 = latencies[iterations * 90 / 100];
    let p95 = latencies[iterations * 95 / 100];
    let p99 = latencies[iterations * 99 / 100];
    let p999 = latencies[iterations * 999 / 1000];
    let max = *latencies.last().unwrap();
    let reuses = cipher.get_reuse_count();

    println!("Total Elapsed Time:      {:?}", total_elapsed);
    println!("Datagrams Processed:     {}", iterations);
    println!("Cipher Reuses Count:     {}", reuses);
    println!("-------------------------------------------------------------------------------");
    println!("  p50 (Median):          {:?}", p50);
    println!("  p90:                   {:?}", p90);
    println!("  p95:                   {:?}", p95);
    println!("  p99:                   {:?}", p99);
    println!("  p99.9:                 {:?}", p999);
    println!("  Max:                   {:?}", max);
    println!("===============================================================================");

    let threshold = Duration::from_micros(200);
    assert!(
        p99 < threshold,
        "FAILED: p99 latency {:?} exceeded 200 µs threshold!",
        p99
    );

    println!("PASSED: p99 latency {:?} is well below the 200 µs threshold!", p99);
}
