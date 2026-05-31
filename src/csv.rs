use std::{
    fs::{create_dir_all, write},
    io,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use crate::report::BenchRecord;

pub fn unix_time_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub fn write_csv_report(records: &[BenchRecord]) -> io::Result<PathBuf> {
    let generated_at_unix_ms = records.first().map_or(0, |r| r.generated_at_unix_ms);
    let output_path = PathBuf::from("results").join(format!("tokio-{}.csv", generated_at_unix_ms));
    create_dir_all("results")?;

    let mut csv = String::from(
        "implementation,runtime,generated_at_unix_ms,benchmark,column_kind,column_value,column_label,iterations,warmup,avg_ns,min_ns,p75_ns,p99_ns,max_ns\n",
    );
    for r in records {
        csv.push_str(&[
            csv_escape(r.implementation),
            csv_escape(&r.runtime),
            r.generated_at_unix_ms.to_string(),
            csv_escape(&r.benchmark),
            csv_escape(r.column_kind),
            r.column_value.to_string(),
            csv_escape(&r.column_label),
            r.iterations.to_string(),
            r.warmup.to_string(),
            r.stats.avg_ns.to_string(),
            r.stats.min_ns.to_string(),
            r.stats.p75_ns.to_string(),
            r.stats.p99_ns.to_string(),
            r.stats.max_ns.to_string(),
        ].join(","));
        csv.push('\n');
    }

    write(&output_path, csv)?;
    Ok(output_path)
}
