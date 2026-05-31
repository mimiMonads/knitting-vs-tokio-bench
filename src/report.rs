pub const ITERATIONS: usize = 500;
pub const WARMUP: usize = 50;
pub const WARMUP_N1: usize = 200;
pub const BATCH_SIZES: &[usize] = &[1, 10, 100];

#[derive(Clone, Copy)]
pub struct BenchStats {
    pub avg_ns: f64,
    pub min_ns: f64,
    pub p75_ns: f64,
    pub p99_ns: f64,
    pub max_ns: f64,
}

pub struct BenchRecord {
    pub implementation: &'static str,
    pub runtime: String,
    pub generated_at_unix_ms: u128,
    pub benchmark: String,
    pub column_kind: &'static str,
    pub column_value: usize,
    pub column_label: String,
    pub iterations: usize,
    pub warmup: usize,
    pub stats: BenchStats,
}

pub fn warmup_iters(batch: usize) -> usize {
    if batch == 1 { WARMUP_N1 } else { WARMUP }
}

pub fn summarize_samples(samples: &mut [std::time::Duration]) -> BenchStats {
    samples.sort();
    let len = samples.len();
    let avg = samples.iter().sum::<std::time::Duration>() / len as u32;
    BenchStats {
        avg_ns: avg.as_nanos() as f64,
        min_ns: samples[0].as_nanos() as f64,
        p75_ns: samples[len * 75 / 100].as_nanos() as f64,
        p99_ns: samples[len * 99 / 100].as_nanos() as f64,
        max_ns: samples[len - 1].as_nanos() as f64,
    }
}

pub fn push_record(
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
