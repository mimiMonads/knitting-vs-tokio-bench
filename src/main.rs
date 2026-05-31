mod csv;
mod display;
mod payloads;
mod report;

use std::{env, io, sync::Arc, time::{Duration, Instant}};

use futures::future::join_all;
use tokio::sync::{mpsc, oneshot};

use csv::{unix_time_millis, write_csv_report};
use display::{fmt_binary_bytes, print_header, print_stats};
use payloads::{make_byte_payloads, power_of_two_sizes, BYTE_FILL_VALUES};
use report::{push_record, summarize_samples, warmup_iters, BenchRecord, BATCH_SIZES, ITERATIONS};

const PAYLOAD_BYTES: usize = 1024 * 1024;
const UINT8ARRAY_SIZE_SWEEP_BATCH: usize = 100;
const UINT8ARRAY_SIZE_SWEEP_MIN_BYTES: usize = 8;
const UINT8ARRAY_SIZE_SWEEP_MAX_BYTES: usize = PAYLOAD_BYTES;
const ARC_COMPARE_BATCH_SIZE: usize = 100;
const ARC_COMPARE_MIN_BYTES: usize = 8;
const ARC_COMPARE_MAX_BYTES: usize = 512;
const ARC_COMPARE_LABEL: &str = "Uint8Array arc comparison size sweep (batch=100)";
const RUNTIME_LABEL: &str = "tokio 1.x mpsc (worker_threads = 1)";

// ── Bench runners ─────────────────────────────────────────────────────────────

async fn run_bench_f64(
    label: &str,
    payload: f64,
    records: &mut Vec<BenchRecord>,
    runtime: &str,
    generated_at_unix_ms: u128,
) {
    print_header(label, "batch");

    for &n in BATCH_SIZES {
        let warmup = warmup_iters(n);
        let mut samples: Vec<Duration> = Vec::with_capacity(ITERATIONS + warmup);
        let (req_tx, mut req_rx) = mpsc::channel::<(f64, oneshot::Sender<f64>)>(n * 2);

        tokio::spawn(async move {
            while let Some((msg, reply_tx)) = req_rx.recv().await {
                let _ = reply_tx.send(msg);
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();
            let mut handles = Vec::with_capacity(n);
            for _ in 0..n {
                let tx = req_tx.clone();
                handles.push(tokio::spawn(async move {
                    let (reply_tx, reply_rx) = oneshot::channel::<f64>();
                    tx.send((payload, reply_tx)).await.unwrap();
                    reply_rx.await.unwrap()
                }));
            }
            for task in join_all(handles).await { let _ = task.unwrap(); }
            if i >= warmup { samples.push(start.elapsed()); }
        }

        let stats = summarize_samples(&mut samples);
        let column_label = format!("n={}", n);
        print_stats(&column_label, stats);
        push_record(records, runtime, generated_at_unix_ms, label, "batch", n, column_label, warmup, stats);
    }
}

async fn run_bench_bytes(
    label: &str,
    payloads: Vec<Vec<u8>>,
    records: &mut Vec<BenchRecord>,
    runtime: &str,
    generated_at_unix_ms: u128,
) {
    print_header(label, "batch");

    for &n in BATCH_SIZES {
        let warmup = warmup_iters(n);
        let mut samples: Vec<Duration> = Vec::with_capacity(ITERATIONS + warmup);
        let (req_tx, mut req_rx) = mpsc::channel::<(Vec<u8>, oneshot::Sender<Vec<u8>>)>(n * 2);

        tokio::spawn(async move {
            while let Some((msg, reply_tx)) = req_rx.recv().await {
                // clone mirrors the TS transport: data is materialized in both directions
                let _ = reply_tx.send(msg.clone());
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();
            let mut handles = Vec::with_capacity(n);
            for j in 0..n {
                let tx = req_tx.clone();
                let payload = payloads[(i + j) % payloads.len()].clone();
                handles.push(tokio::spawn(async move {
                    let (reply_tx, reply_rx) = oneshot::channel::<Vec<u8>>();
                    tx.send((payload, reply_tx)).await.unwrap();
                    reply_rx.await.unwrap()
                }));
            }
            for task in join_all(handles).await { let _ = task.unwrap(); }
            if i >= warmup { samples.push(start.elapsed()); }
        }

        let stats = summarize_samples(&mut samples);
        let column_label = format!("n={}", n);
        print_stats(&column_label, stats);
        push_record(records, runtime, generated_at_unix_ms, label, "batch", n, column_label, warmup, stats);
    }
}

// Zero-copy: Arc::clone only bumps the refcount, no heap allocation.
// Fair comparison for knitting's ProcessSharedBuffer (which passes only an fd reference).
async fn run_bench_arc_bytes(
    label: &str,
    payloads: Vec<Arc<Vec<u8>>>,
    records: &mut Vec<BenchRecord>,
    runtime: &str,
    generated_at_unix_ms: u128,
) {
    print_header(label, "batch");

    for &n in BATCH_SIZES {
        let warmup = warmup_iters(n);
        let mut samples: Vec<Duration> = Vec::with_capacity(ITERATIONS + warmup);
        let (req_tx, mut req_rx) = mpsc::channel::<(Arc<Vec<u8>>, oneshot::Sender<Arc<Vec<u8>>>)>(n * 2);

        tokio::spawn(async move {
            while let Some((msg, reply_tx)) = req_rx.recv().await {
                let _ = reply_tx.send(msg);
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();
            let mut handles = Vec::with_capacity(n);
            for j in 0..n {
                let tx = req_tx.clone();
                let payload = Arc::clone(&payloads[(i + j) % payloads.len()]);
                handles.push(tokio::spawn(async move {
                    let (reply_tx, reply_rx) = oneshot::channel::<Arc<Vec<u8>>>();
                    tx.send((payload, reply_tx)).await.unwrap();
                    reply_rx.await.unwrap()
                }));
            }
            for task in join_all(handles).await { let _ = task.unwrap(); }
            if i >= warmup { samples.push(start.elapsed()); }
        }

        let stats = summarize_samples(&mut samples);
        let column_label = format!("n={}", n);
        print_stats(&column_label, stats);
        push_record(records, runtime, generated_at_unix_ms, label, "batch", n, column_label, warmup, stats);
    }
}

async fn run_bench_bytes_size_sweep(
    records: &mut Vec<BenchRecord>,
    runtime: &str,
    generated_at_unix_ms: u128,
) {
    let benchmark_label = format!("Uint8Array size sweep (batch={})", UINT8ARRAY_SIZE_SWEEP_BATCH);
    print_header(
        &format!("Uint8Array size sweep (batch={}, {} -> {})",
            UINT8ARRAY_SIZE_SWEEP_BATCH,
            fmt_binary_bytes(UINT8ARRAY_SIZE_SWEEP_MIN_BYTES),
            fmt_binary_bytes(UINT8ARRAY_SIZE_SWEEP_MAX_BYTES)),
        "size",
    );

    let warmup = warmup_iters(UINT8ARRAY_SIZE_SWEEP_BATCH);
    for bytes in power_of_two_sizes(UINT8ARRAY_SIZE_SWEEP_MIN_BYTES, UINT8ARRAY_SIZE_SWEEP_MAX_BYTES) {
        let mut samples: Vec<Duration> = Vec::with_capacity(ITERATIONS + warmup);
        let payloads = make_byte_payloads(bytes);
        let (req_tx, mut req_rx) = mpsc::channel::<(Vec<u8>, oneshot::Sender<Vec<u8>>)>(UINT8ARRAY_SIZE_SWEEP_BATCH * 2);

        tokio::spawn(async move {
            while let Some((msg, reply_tx)) = req_rx.recv().await {
                let _ = reply_tx.send(msg.clone());
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();
            let mut handles = Vec::with_capacity(UINT8ARRAY_SIZE_SWEEP_BATCH);
            for j in 0..UINT8ARRAY_SIZE_SWEEP_BATCH {
                let tx = req_tx.clone();
                let payload = payloads[(i + j) % payloads.len()].clone();
                handles.push(tokio::spawn(async move {
                    let (reply_tx, reply_rx) = oneshot::channel::<Vec<u8>>();
                    tx.send((payload, reply_tx)).await.unwrap();
                    reply_rx.await.unwrap()
                }));
            }
            for task in join_all(handles).await { let _ = task.unwrap(); }
            if i >= warmup { samples.push(start.elapsed()); }
        }

        let stats = summarize_samples(&mut samples);
        let column_label = fmt_binary_bytes(bytes);
        print_stats(&column_label, stats);
        push_record(records, runtime, generated_at_unix_ms, &benchmark_label, "size_bytes", bytes, column_label, warmup, stats);
    }
}

async fn run_bench_arc_bytes_size_sweep(
    records: &mut Vec<BenchRecord>,
    runtime: &str,
    generated_at_unix_ms: u128,
) {
    print_header(
        &format!("{} ({} -> {})", ARC_COMPARE_LABEL, fmt_binary_bytes(ARC_COMPARE_MIN_BYTES), fmt_binary_bytes(ARC_COMPARE_MAX_BYTES)),
        "size",
    );

    let warmup = warmup_iters(ARC_COMPARE_BATCH_SIZE);
    for bytes in power_of_two_sizes(ARC_COMPARE_MIN_BYTES, ARC_COMPARE_MAX_BYTES) {
        let mut samples: Vec<Duration> = Vec::with_capacity(ITERATIONS + warmup);
        let payloads: Vec<Arc<Vec<u8>>> = BYTE_FILL_VALUES.iter().map(|&fill| Arc::new(vec![fill; bytes])).collect();
        let (req_tx, mut req_rx) = mpsc::channel::<(Arc<Vec<u8>>, oneshot::Sender<Arc<Vec<u8>>>)>(ARC_COMPARE_BATCH_SIZE * 2);

        tokio::spawn(async move {
            while let Some((msg, reply_tx)) = req_rx.recv().await {
                let _ = reply_tx.send(msg);
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();
            let mut handles = Vec::with_capacity(ARC_COMPARE_BATCH_SIZE);
            for j in 0..ARC_COMPARE_BATCH_SIZE {
                let tx = req_tx.clone();
                let payload = Arc::clone(&payloads[(i + j) % payloads.len()]);
                handles.push(tokio::spawn(async move {
                    let (reply_tx, reply_rx) = oneshot::channel::<Arc<Vec<u8>>>();
                    tx.send((payload, reply_tx)).await.unwrap();
                    reply_rx.await.unwrap()
                }));
            }
            for task in join_all(handles).await { let _ = task.unwrap(); }
            if i >= warmup { samples.push(start.elapsed()); }
        }

        let stats = summarize_samples(&mut samples);
        let column_label = fmt_binary_bytes(bytes);
        print_stats(&column_label, stats);
        push_record(records, runtime, generated_at_unix_ms, ARC_COMPARE_LABEL, "size_bytes", bytes, column_label, warmup, stats);
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main(worker_threads = 1)]
async fn main() -> io::Result<()> {
    let write_csv = env::args().skip(1).any(|arg| arg == "--csv");
    let generated_at_unix_ms = unix_time_millis();
    let mut records = Vec::new();

    println!("runtime: {}", RUNTIME_LABEL);
    println!("task: send payload -> worker echo -> return, join_all");
    println!("(whole-batch latency; warmup n=1: {}, others: {})", report::WARMUP_N1, report::WARMUP);
    println!("(bytes rotate 4 payload variants per batch)");
    println!("(Vec<u8> clones on send and reply — data materialized in both directions)");
    println!("(Arc<Vec<u8>> passes refcount only — zero-copy, fair comparison for ProcessSharedBuffer)");

    // ── f64: pure scheduling overhead, nothing copied ──
    run_bench_f64("number: f64 (8 bytes)", 42.0, &mut records, RUNTIME_LABEL, generated_at_unix_ms).await;

    // ── Vec<u8> copy (1 MiB): clone() = heap alloc + memcpy in both directions ──
    run_bench_bytes(
        "Uint8Array: 1MB (1048576 bytes)",
        make_byte_payloads(PAYLOAD_BYTES),
        &mut records, RUNTIME_LABEL, generated_at_unix_ms,
    ).await;

    // ── Arc<Vec<u8>>: zero-copy, only refcount moves — fair comparison for ProcessSharedBuffer ──
    run_bench_arc_bytes(
        "Arc<Vec<u8>>: 1MB (1048576 bytes)",
        BYTE_FILL_VALUES.iter().map(|&fill| Arc::new(vec![fill; PAYLOAD_BYTES])).collect(),
        &mut records, RUNTIME_LABEL, generated_at_unix_ms,
    ).await;

    // ── Size sweeps ──
    run_bench_bytes_size_sweep(&mut records, RUNTIME_LABEL, generated_at_unix_ms).await;
    run_bench_arc_bytes_size_sweep(&mut records, RUNTIME_LABEL, generated_at_unix_ms).await;

    if write_csv {
        let output_path = write_csv_report(&records)?;
        println!("csv: {}", output_path.display());
    }

    println!();
    Ok(())
}
