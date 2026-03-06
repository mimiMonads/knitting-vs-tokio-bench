use std::time::{Duration, Instant};

use futures::future::join_all;
use tokio::sync::mpsc;

const BATCH_SIZES: &[usize] = &[1, 10, 100];
const ITERATIONS: usize = 500;
const WARMUP: usize = 50;
const WARMUP_N1: usize = 200;
const PAYLOAD_BYTES: usize = 1024 * 1024;
const BYTE_FILL_VALUES: &[u8] = &[0xAB, 0xBC, 0xCD, 0xDE];
const UINT8ARRAY_SIZE_SWEEP_BATCH: usize = 100;
const UINT8ARRAY_SIZE_SWEEP_MIN_BYTES: usize = 8;
const UINT8ARRAY_SIZE_SWEEP_MAX_BYTES: usize = PAYLOAD_BYTES;
const LABEL_COLUMN_WIDTH: usize = 10;

fn warmup_iters(batch: usize) -> usize {
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

fn fmt_binary_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{} MiB", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        format!("{} KiB", bytes / 1024)
    } else {
        format!("{} B", bytes)
    }
}

fn print_header(label: &str, column_label: &str) {
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

fn print_stats(label: &str, samples: &mut [Duration]) {
    samples.sort();
    let len = samples.len();
    let avg = samples.iter().sum::<Duration>() / len as u32;
    let min = samples[0];
    let p75 = samples[len * 75 / 100];
    let p99 = samples[len * 99 / 100];
    let max = samples[len - 1];

    println!(
        "{:<width$} {:>12} {:>12} {:>12} {:>12} {:>12}",
        label,
        fmt_ns(avg.as_nanos() as f64),
        fmt_ns(min.as_nanos() as f64),
        fmt_ns(p75.as_nanos() as f64),
        fmt_ns(p99.as_nanos() as f64),
        fmt_ns(max.as_nanos() as f64),
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

async fn run_bench_str(label: &str, payloads: Vec<String>) {
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
                let _ = reply_tx.send(msg);
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();

            let mut handles = Vec::with_capacity(n);
            for j in 0..n {
                let payload_idx = (i + j) % payloads.len();
                let tx = req_tx.clone();
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

        print_stats(&format!("n={}", n), &mut samples);
    }
}

async fn run_bench_bytes(label: &str, payloads: Vec<Vec<u8>>) {
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
                let _ = reply_tx.send(msg);
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();

            let mut handles = Vec::with_capacity(n);
            for j in 0..n {
                let payload_idx = (i + j) % payloads.len();
                let tx = req_tx.clone();
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

        print_stats(&format!("n={}", n), &mut samples);
    }
}

async fn run_bench_f64(label: &str, payload: f64) {
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

        print_stats(&format!("n={}", n), &mut samples);
    }
}

async fn run_bench_bytes_size_sweep() {
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
                let _ = reply_tx.send(msg);
            }
        });

        for i in 0..(ITERATIONS + warmup) {
            let start = Instant::now();

            let mut handles = Vec::with_capacity(UINT8ARRAY_SIZE_SWEEP_BATCH);
            for j in 0..UINT8ARRAY_SIZE_SWEEP_BATCH {
                let payload_idx = (i + j) % payloads.len();
                let tx = req_tx.clone();
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

        print_stats(&fmt_binary_bytes(bytes), &mut samples);
    }
}

#[tokio::main(worker_threads = 1)]
async fn main() {
    println!("cpu: Ryzen 9 5950X ~4.55GHz");
    println!("runtime: tokio 1.x mpsc (worker_threads = 1)");
    println!("task: send payload -> worker echo -> return, join_all");
    println!(
        "(whole-batch latency; warmup n=1: {}, others: {})",
        WARMUP_N1, WARMUP
    );
    println!("(string/bytes use 4 payload variants rotated with index % 4)");

    run_bench_str(
        "large string: 1MB (1048576 bytes)",
        vec![
            "x".repeat(PAYLOAD_BYTES),
            "y".repeat(PAYLOAD_BYTES),
            "z".repeat(PAYLOAD_BYTES),
            "w".repeat(PAYLOAD_BYTES),
        ],
    )
    .await;
    run_bench_f64("number: f64 (8 bytes)", 42.0).await;
    run_bench_bytes(
        "Uint8Array: 1MB (1048576 bytes)",
        make_byte_payloads(PAYLOAD_BYTES),
    )
    .await;
    run_bench_bytes_size_sweep().await;

    println!();
}
