use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use futures::future::join_all;
use tokio::sync::{mpsc, oneshot};

use crate::{
    fmt_binary_bytes, print_header, print_stats, push_record, summarize_samples, warmup_iters,
    BenchRecord, ITERATIONS,
};

const ARC_COMPARE_BATCH_SIZE: usize = 100;
const ARC_COMPARE_MIN_BYTES: usize = 8;
const ARC_COMPARE_MAX_BYTES: usize = 512;
const ARC_COMPARE_LABEL: &str = "Uint8Array arc comparison size sweep (batch=100)";

fn arc_compare_sizes() -> Vec<usize> {
    let mut sizes = Vec::new();
    let mut bytes = ARC_COMPARE_MIN_BYTES;

    while bytes <= ARC_COMPARE_MAX_BYTES {
        sizes.push(bytes);
        bytes *= 2;
    }

    sizes
}

pub(crate) async fn run_bench_arc_bytes_size_sweep(
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
