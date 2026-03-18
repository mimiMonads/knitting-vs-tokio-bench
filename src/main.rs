use std::{
    env,
    fs::{create_dir_all, write},
    io,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures::future::join_all;
use tokio::sync::{mpsc, oneshot};

const BATCH_SIZES: &[usize] = &[1, 10, 100];
pub(crate) const ITERATIONS: usize = 500;
const WARMUP: usize = 50;
const WARMUP_N1: usize = 200;
const PAYLOAD_BYTES: usize = 1024 * 1024;
const BYTE_FILL_VALUES: &[u8] = &[0xAB, 0xBC, 0xCD, 0xDE];
const UINT8ARRAY_SIZE_SWEEP_BATCH: usize = 100;
const UINT8ARRAY_SIZE_SWEEP_MIN_BYTES: usize = 8;
const UINT8ARRAY_SIZE_SWEEP_MAX_BYTES: usize = PAYLOAD_BYTES;
const ARC_COMPARE_BATCH_SIZE: usize = 100;
const ARC_COMPARE_MIN_BYTES: usize = 8;
const ARC_COMPARE_MAX_BYTES: usize = 512;
const ARC_COMPARE_LABEL: &str = "Uint8Array arc comparison size sweep (batch=100)";
const LABEL_COLUMN_WIDTH: usize = 10;
const RUNTIME_LABEL: &str = "tokio 1.x mpsc (worker_threads = 1)";

#[derive(Clone, Copy)]
pub(crate) struct BenchStats {
    avg_ns: f64,
    min_ns: f64,
    p75_ns: f64,
    p99_ns: f64,
    max_ns: f64,
}

pub(crate) struct BenchRecord {
    implementation: &'static str,
    runtime: String,
    generated_at_unix_ms: u128,
    benchmark: String,
    column_kind: &'static str,
    column_value: usize,
    column_label: String,
    iterations: usize,
    warmup: usize,
    stats: BenchStats,
}

pub(crate) fn warmup_iters(batch: usize) -> usize {
    if batch == 1 {
        WARMUP_N1
    } else {
        WARMUP
    }
}

fn fmt_ns(ns: f64) -> String {
    if ns >= 1_000_000.0 {
        format!("{:.2} ms", ns / 1_000_000.0)
    } else if ns >= 1_000.0 {
        format!("{:.2} \u{00B5}s", ns / 1_000.0)
    } else {
        format!("{:.2} ns", ns)
    }
}

pub(crate) fn fmt_binary_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{} MiB", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        format!("{} KiB", bytes / 1024)
    } else {
        format!("{} B", bytes)
    }
}

pub(crate) fn print_header(label: &str, column_label: &str) {
    println!("\n--- {} ---", label);
    println!(
        "{:<width$} {:>12} {:>12} {:>12} {:>12} {:>12}",
        column_label,
        "avg",
        "min",
        "p75",
        "p99",
        "max",
        width = LABEL_COLUMN_WIDTH,
    );
    println!("{}", "-".repeat(70));
}

pub(crate) fn summarize_samples(samples: &mut [Duration]) -> BenchStats {
    samples.sort();
    let len = samples.len();
    let avg = samples.iter().sum::<Duration>() / len as u32;

    BenchStats {
        avg_ns: avg.as_nanos() as f64,
        min_ns: samples[0].as_nanos() as f64,
        p75_ns: samples[len * 75 / 100].as_nanos() as f64,
        p99_ns: samples[len * 99 / 100].as_nanos() as f64,
        max_ns: samples[len - 1].as_nanos() as f64,
    }
}

pub(crate) fn print_stats(label: &str, stats: BenchStats) {
    println!(
        "{:<width$} {:>12} {:>12} {:>12} {:>12} {:>12}",
        label,
        fmt_ns(stats.avg_ns),
        fmt_ns(stats.min_ns),
        fmt_ns(stats.p75_ns),
        fmt_ns(stats.p99_ns),
        fmt_ns(stats.max_ns),
        width = LABEL_COLUMN_WIDTH,
    );
}

fn make_byte_payloads(bytes: usize) -> Vec<Vec<u8>> {
    BYTE_FILL_VALUES
        .iter()
        .map(|fill_value| vec![*fill_value; bytes])
        .collect()
}

fn uint8array_size_sweep_bytes() -> Vec<usize> {
    let mut sizes = Vec::new();
    let mut bytes = UINT8ARRAY_SIZE_SWEEP_MIN_BYTES;
    while bytes <= UINT8ARRAY_SIZE_SWEEP_MAX_BYTES {
        sizes.push(bytes);
        bytes *= 2;
    }
    sizes
}

fn arc_compare_sizes() -> Vec<usize> {
    let mut sizes = Vec::new();
    let mut bytes = ARC_COMPARE_MIN_BYTES;

    while bytes <= ARC_COMPARE_MAX_BYTES {
        sizes.push(bytes);
        bytes *= 2;
    }

    sizes
}

pub(crate) fn push_record(
    records: &mut Vec<BenchRecord>,
    runtime: &str,
    generated_at_unix_ms: u128,
    benchmark: &str,
    column_kind: &'static str,
    column_value: usize,
    column_label: String,
    warmup: usize,
    stats: BenchStats,
) {
    records.push(BenchRecord {
        implementation: "tokio",
        runtime: runtime.to_string(),
        generated_at_unix_ms,
        benchmark: benchmark.to_string(),
        column_kind,
        column_value,
        column_label,
        iterations: ITERATIONS,
        warmup,
        stats,
    });
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn write_csv_report(records: &[BenchRecord]) -> io::Result<PathBuf> {
    let generated_at_unix_ms = records
        .first()
        .map_or(0, |record| record.generated_at_unix_ms);
    let output_path = PathBuf::from("results").join(format!("tokio-{}.csv", generated_at_unix_ms));

    create_dir_all("results")?;

    let mut csv = String::from(
        "implementation,runtime,generated_at_unix_ms,benchmark,column_kind,column_value,column_label,iterations,warmup,avg_ns,min_ns,p75_ns,p99_ns,max_ns\n",
    );

    for record in records {
        csv.push_str(
            &[
                csv_escape(record.implementation),
                csv_escape(&record.runtime),
                record.generated_at_unix_ms.to_string(),
                csv_escape(&record.benchmark),
                csv_escape(record.column_kind),
                record.column_value.to_string(),
                csv_escape(&record.column_label),
                record.iterations.to_string(),
                record.warmup.to_string(),
                record.stats.avg_ns.to_string(),
                record.stats.min_ns.to_string(),
                record.stats.p75_ns.to_string(),
                record.stats.p99_ns.to_string(),
                record.stats.max_ns.to_string(),
            ]
            .join(","),
        );
        csv.push('\n');
    }

    write(&output_path, csv)?;

    Ok(output_path)
}

fn unix_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

async fn run_bench_str(
    label: &str,
    payloads: Vec<String>,
    records: &mut Vec<BenchRecord>,
    runtime: &str,
    generated_at_unix_ms: u128,
) {
    assert!(
        payloads.len() == 4,
        "run_bench_str expects exactly 4 payload variants"
    );

    print_header(label, "batch");

    for &n in BATCH_SIZES {
        let warmup = warmup_iters(n);
        let mut samples: Vec<Duration> = Vec::with_capacity(ITERATIONS + warmup);

        let (req_tx, mut req_rx) =
            mpsc::channel::<(String, tokio::sync::oneshot::Sender<String>)>(n * 2);

        tokio::spawn(async move {
            while let Some((msg, reply_tx)) = req_rx.recv().await {
                // Clone on reply so the timed round trip materializes a fresh
                // payload in both directions, matching the TS transport more closely.
                let _ = reply_tx.send(msg.clone());
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();

            let mut handles = Vec::with_capacity(n);
            for j in 0..n {
                let payload_idx = (i + j) % payloads.len();
                let tx = req_tx.clone();
                // Cloning the String happens inside the timed path.
                let payload = payloads[payload_idx].clone();
                handles.push(tokio::spawn(async move {
                    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel::<String>();
                    tx.send((payload, reply_tx)).await.unwrap();
                    reply_rx.await.unwrap()
                }));
            }

            for task in join_all(handles).await {
                let _ = task.unwrap();
            }

            let elapsed = start.elapsed();
            if i >= warmup {
                samples.push(elapsed);
            }
        }

        let stats = summarize_samples(&mut samples);
        let column_label = format!("n={}", n);
        print_stats(&column_label, stats);
        push_record(
            records,
            runtime,
            generated_at_unix_ms,
            label,
            "batch",
            n,
            column_label,
            warmup,
            stats,
        );
    }
}

async fn run_bench_bytes(
    label: &str,
    payloads: Vec<Vec<u8>>,
    records: &mut Vec<BenchRecord>,
    runtime: &str,
    generated_at_unix_ms: u128,
) {
    assert!(
        payloads.len() == 4,
        "run_bench_bytes expects exactly 4 payload variants"
    );

    print_header(label, "batch");

    for &n in BATCH_SIZES {
        let warmup = warmup_iters(n);
        let mut samples: Vec<Duration> = Vec::with_capacity(ITERATIONS + warmup);

        let (req_tx, mut req_rx) =
            mpsc::channel::<(Vec<u8>, tokio::sync::oneshot::Sender<Vec<u8>>)>(n * 2);

        tokio::spawn(async move {
            while let Some((msg, reply_tx)) = req_rx.recv().await {
                // Clone on reply so the timed round trip materializes a fresh
                // payload in both directions, matching the TS transport more closely.
                let _ = reply_tx.send(msg.clone());
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();

            let mut handles = Vec::with_capacity(n);
            for j in 0..n {
                let payload_idx = (i + j) % payloads.len();
                let tx = req_tx.clone();
                // Cloning the Vec<u8> happens inside the timed path.
                let payload = payloads[payload_idx].clone();
                handles.push(tokio::spawn(async move {
                    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel::<Vec<u8>>();
                    tx.send((payload, reply_tx)).await.unwrap();
                    reply_rx.await.unwrap()
                }));
            }

            for task in join_all(handles).await {
                let _ = task.unwrap();
            }

            let elapsed = start.elapsed();
            if i >= warmup {
                samples.push(elapsed);
            }
        }

        let stats = summarize_samples(&mut samples);
        let column_label = format!("n={}", n);
        print_stats(&column_label, stats);
        push_record(
            records,
            runtime,
            generated_at_unix_ms,
            label,
            "batch",
            n,
            column_label,
            warmup,
            stats,
        );
    }
}

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

        let (req_tx, mut req_rx) = mpsc::channel::<(f64, tokio::sync::oneshot::Sender<f64>)>(n * 2);

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
                    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel::<f64>();
                    tx.send((payload, reply_tx)).await.unwrap();
                    reply_rx.await.unwrap()
                }));
            }

            for task in join_all(handles).await {
                let _ = task.unwrap();
            }

            let elapsed = start.elapsed();
            if i >= warmup {
                samples.push(elapsed);
            }
        }

        let stats = summarize_samples(&mut samples);
        let column_label = format!("n={}", n);
        print_stats(&column_label, stats);
        push_record(
            records,
            runtime,
            generated_at_unix_ms,
            label,
            "batch",
            n,
            column_label,
            warmup,
            stats,
        );
    }
}

async fn run_bench_bytes_size_sweep(
    records: &mut Vec<BenchRecord>,
    runtime: &str,
    generated_at_unix_ms: u128,
) {
    let benchmark_label = format!(
        "Uint8Array size sweep (batch={})",
        UINT8ARRAY_SIZE_SWEEP_BATCH
    );

    print_header(
        &format!(
            "Uint8Array size sweep (batch={}, {} -> {})",
            UINT8ARRAY_SIZE_SWEEP_BATCH,
            fmt_binary_bytes(UINT8ARRAY_SIZE_SWEEP_MIN_BYTES),
            fmt_binary_bytes(UINT8ARRAY_SIZE_SWEEP_MAX_BYTES),
        ),
        "size",
    );

    let warmup = warmup_iters(UINT8ARRAY_SIZE_SWEEP_BATCH);

    for bytes in uint8array_size_sweep_bytes() {
        let mut samples: Vec<Duration> = Vec::with_capacity(ITERATIONS + warmup);
        let payloads = make_byte_payloads(bytes);
        let (req_tx, mut req_rx) = mpsc::channel::<(Vec<u8>, tokio::sync::oneshot::Sender<Vec<u8>>)>(
            UINT8ARRAY_SIZE_SWEEP_BATCH * 2,
        );

        tokio::spawn(async move {
            while let Some((msg, reply_tx)) = req_rx.recv().await {
                // Clone on reply so the timed round trip materializes a fresh
                // payload in both directions, matching the TS transport more closely.
                let _ = reply_tx.send(msg.clone());
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();

            let mut handles = Vec::with_capacity(UINT8ARRAY_SIZE_SWEEP_BATCH);
            for j in 0..UINT8ARRAY_SIZE_SWEEP_BATCH {
                let payload_idx = (i + j) % payloads.len();
                let tx = req_tx.clone();
                // Cloning the Vec<u8> happens inside the timed path.
                let payload = payloads[payload_idx].clone();
                handles.push(tokio::spawn(async move {
                    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel::<Vec<u8>>();
                    tx.send((payload, reply_tx)).await.unwrap();
                    reply_rx.await.unwrap()
                }));
            }

            for task in join_all(handles).await {
                let _ = task.unwrap();
            }

            let elapsed = start.elapsed();
            if i >= warmup {
                samples.push(elapsed);
            }
        }

        let stats = summarize_samples(&mut samples);
        let column_label = fmt_binary_bytes(bytes);
        print_stats(&column_label, stats);
        push_record(
            records,
            runtime,
            generated_at_unix_ms,
            &benchmark_label,
            "size_bytes",
            bytes,
            column_label,
            warmup,
            stats,
        );
    }
}

async fn run_bench_arc_bytes_size_sweep(
    records: &mut Vec<BenchRecord>,
    runtime: &str,
    generated_at_unix_ms: u128,
) {
    print_header(
        &format!(
            "{} ({} -> {})",
            ARC_COMPARE_LABEL,
            fmt_binary_bytes(ARC_COMPARE_MIN_BYTES),
            fmt_binary_bytes(ARC_COMPARE_MAX_BYTES),
        ),
        "size",
    );

    let warmup = warmup_iters(ARC_COMPARE_BATCH_SIZE);

    for bytes in arc_compare_sizes() {
        let mut samples: Vec<Duration> = Vec::with_capacity(ITERATIONS + warmup);
        let payloads: Vec<Arc<Vec<u8>>> = vec![
            Arc::new(vec![0xAB; bytes]),
            Arc::new(vec![0xBC; bytes]),
            Arc::new(vec![0xCD; bytes]),
            Arc::new(vec![0xDE; bytes]),
        ];

        let (req_tx, mut req_rx) = mpsc::channel::<(Arc<Vec<u8>>, oneshot::Sender<Arc<Vec<u8>>>)>(
            ARC_COMPARE_BATCH_SIZE * 2,
        );

        tokio::spawn(async move {
            while let Some((msg, reply_tx)) = req_rx.recv().await {
                let _ = reply_tx.send(msg);
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();

            let mut handles = Vec::with_capacity(ARC_COMPARE_BATCH_SIZE);
            for j in 0..ARC_COMPARE_BATCH_SIZE {
                let payload_idx = (i + j) % payloads.len();
                let tx = req_tx.clone();
                // Arc::clone only bumps the refcount; it does not clone the bytes.
                let payload = Arc::clone(&payloads[payload_idx]);
                handles.push(tokio::spawn(async move {
                    let (reply_tx, reply_rx) = oneshot::channel::<Arc<Vec<u8>>>();
                    tx.send((payload, reply_tx)).await.unwrap();
                    reply_rx.await.unwrap()
                }));
            }

            for task in join_all(handles).await {
                let _ = task.unwrap();
            }

            let elapsed = start.elapsed();
            if i >= warmup {
                samples.push(elapsed);
            }
        }

        let stats = summarize_samples(&mut samples);
        let column_label = fmt_binary_bytes(bytes);
        print_stats(&column_label, stats);
        push_record(
            records,
            runtime,
            generated_at_unix_ms,
            ARC_COMPARE_LABEL,
            "size_bytes",
            bytes,
            column_label,
            warmup,
            stats,
        );
    }
}

#[tokio::main(worker_threads = 1)]
async fn main() -> io::Result<()> {
    let write_csv = env::args().skip(1).any(|arg| arg == "--csv");
    let generated_at_unix_ms = unix_time_millis();
    let mut records = Vec::new();

    println!("runtime: {}", RUNTIME_LABEL);
    println!("task: send payload -> worker echo -> return, join_all");
    println!(
        "(whole-batch latency; warmup n=1: {}, others: {})",
        WARMUP_N1, WARMUP
    );
    println!("(string/bytes use 4 payload variants rotated with index % 4)");
    println!("(Rust string/bytes clone on send and reply to mirror TS round-trip materialization)");
    println!("(the Arc<Vec<u8>> small-size sweep is a separate Rust upper-bound reference)");

    run_bench_f64(
        "number: f64 (8 bytes)",
        42.0,
        &mut records,
        RUNTIME_LABEL,
        generated_at_unix_ms,
    )
    .await;
    run_bench_str(
        "large string: 1MB (1048576 bytes)",
        vec![
            "x".repeat(PAYLOAD_BYTES),
            "y".repeat(PAYLOAD_BYTES),
            "z".repeat(PAYLOAD_BYTES),
            "w".repeat(PAYLOAD_BYTES),
        ],
        &mut records,
        RUNTIME_LABEL,
        generated_at_unix_ms,
    )
    .await;
    run_bench_bytes(
        "Uint8Array: 1MB (1048576 bytes)",
        make_byte_payloads(PAYLOAD_BYTES),
        &mut records,
        RUNTIME_LABEL,
        generated_at_unix_ms,
    )
    .await;
    run_bench_bytes_size_sweep(&mut records, RUNTIME_LABEL, generated_at_unix_ms).await;
    run_bench_arc_bytes_size_sweep(&mut records, RUNTIME_LABEL, generated_at_unix_ms).await;

    if write_csv {
        let output_path = write_csv_report(&records)?;
        println!("csv: {}", output_path.display());
    }

    println!();

    Ok(())
}
