use crate::report::BenchStats;

pub const LABEL_COLUMN_WIDTH: usize = 10;

pub fn fmt_ns(ns: f64) -> String {
    if ns >= 1_000_000.0 {
        format!("{:.2} ms", ns / 1_000_000.0)
    } else if ns >= 1_000.0 {
        format!("{:.2} \u{00B5}s", ns / 1_000.0)
    } else {
        format!("{:.2} ns", ns)
    }
}

pub fn fmt_binary_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{} MiB", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        format!("{} KiB", bytes / 1024)
    } else {
        format!("{} B", bytes)
    }
}

pub fn print_header(label: &str, column_label: &str) {
    println!("\n--- {} ---", label);
    println!(
        "{:<width$} {:>12} {:>12} {:>12} {:>12} {:>12}",
        column_label, "avg", "min", "p75", "p99", "max",
        width = LABEL_COLUMN_WIDTH,
    );
    println!("{}", "-".repeat(70));
}

pub fn print_stats(label: &str, stats: BenchStats) {
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
