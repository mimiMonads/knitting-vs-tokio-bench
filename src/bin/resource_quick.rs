use std::{
    io,
    time::{Duration, Instant},
};

use futures::future::join_all;
use tokio::sync::{mpsc, oneshot};

const TARGET_DURATION: Duration = Duration::from_millis(1_500);
const F64_BATCH: usize = 256;
const BYTE_BATCH: usize = 32;
const PAYLOAD_BYTES: usize = 256 * 1024;
const BYTE_FILL_VALUES: &[u8] = &[0xAB, 0xBC, 0xCD, 0xDE];
const RUNTIME_LABEL: &str = "tokio 1.x mpsc (worker_threads = 1)";

fn make_byte_payloads(bytes: usize) -> Vec<Vec<u8>> {
    BYTE_FILL_VALUES
        .iter()
        .map(|fill_value| vec![*fill_value; bytes])
        .collect()
}

#[tokio::main(worker_threads = 1)]
async fn main() -> io::Result<()> {
    let (f64_tx, mut f64_rx) = mpsc::channel::<(f64, oneshot::Sender<f64>)>(F64_BATCH * 2);
    tokio::spawn(async move {
        while let Some((msg, reply_tx)) = f64_rx.recv().await {
            let _ = reply_tx.send(msg);
        }
    });

    let (bytes_tx, mut bytes_rx) =
        mpsc::channel::<(Vec<u8>, oneshot::Sender<Vec<u8>>)>(BYTE_BATCH * 2);
    tokio::spawn(async move {
        while let Some((msg, reply_tx)) = bytes_rx.recv().await {
            let _ = reply_tx.send(msg.clone());
        }
    });

    let payloads = make_byte_payloads(PAYLOAD_BYTES);
    let started_at = Instant::now();
    let mut completed_batches = 0usize;
    let mut f64_messages = 0usize;
    let mut byte_messages = 0usize;
    let mut sink = 0usize;
    let mut turn = 0usize;

    println!("resource benchmark: {}", RUNTIME_LABEL);
    println!(
        "target_duration_ms={} f64_batch={} bytes_batch={} payload_bytes={}",
        TARGET_DURATION.as_millis(),
        F64_BATCH,
        BYTE_BATCH,
        PAYLOAD_BYTES,
    );

    while started_at.elapsed() < TARGET_DURATION {
        let mut number_handles = Vec::with_capacity(F64_BATCH);
        for j in 0..F64_BATCH {
            let tx = f64_tx.clone();
            let value = 42.0 + ((turn + j) & 255) as f64;
            number_handles.push(tokio::spawn(async move {
                let (reply_tx, reply_rx) = oneshot::channel::<f64>();
                tx.send((value, reply_tx)).await.unwrap();
                reply_rx.await.unwrap()
            }));
        }

        let mut byte_handles = Vec::with_capacity(BYTE_BATCH);
        for j in 0..BYTE_BATCH {
            let tx = bytes_tx.clone();
            let payload = payloads[(turn + j) % payloads.len()].clone();
            byte_handles.push(tokio::spawn(async move {
                let (reply_tx, reply_rx) = oneshot::channel::<Vec<u8>>();
                tx.send((payload, reply_tx)).await.unwrap();
                reply_rx.await.unwrap()
            }));
        }

        for task in join_all(number_handles).await {
            sink ^= task.unwrap() as usize;
        }

        for task in join_all(byte_handles).await {
            sink ^= task.unwrap().len();
        }

        completed_batches += 1;
        f64_messages += F64_BATCH;
        byte_messages += BYTE_BATCH;
        turn += 1;
    }

    println!("completed_batches={}", completed_batches);
    println!("f64_messages={}", f64_messages);
    println!("byte_messages={}", byte_messages);
    println!("payload_bytes={}", PAYLOAD_BYTES);
    println!("sink={}", sink);

    Ok(())
}
