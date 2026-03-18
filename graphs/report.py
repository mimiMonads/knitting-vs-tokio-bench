from __future__ import annotations

import argparse
import csv
import os
import platform
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

import matplotlib.pyplot as plt
from matplotlib.ticker import FuncFormatter

from plot_style import (
    DARK_AXES_HEX,
    DARK_BG_HEX,
    RUNTIME_COLORS,
    RUNTIME_LABELS,
    RUNTIME_ORDER,
    TEXT_HEX,
    TITLE_HEX,
    apply_dark_style,
)


BATCH_BENCHMARKS = (
    ("number_f64", "number f64 (8 bytes)"),
    ("large_string_1mb", "large string 1 MiB"),
    ("uint8array_1mb", "Uint8Array 1 MiB"),
)
SIZE_SWEEP_BENCHMARK = ("uint8array_size_sweep", "Uint8Array size sweep (batch=100)")
ARC_COMPARE_SIZE_SWEEP_BENCHMARK = (
    "uint8array_arc_compare_size_sweep",
    "Uint8Array arc comparison size sweep (batch=100)",
)
METHODOLOGY_NOTES = (
    "The main string and byte benchmarks are intended to compare the same logical round trip on both sides: send payload, receive it in the worker, echo it back, receive it again on the caller, then wait for the whole batch.",
    "In `src/main.ts`, the `string` and `Uint8Array` paths go through knitting transport in both directions. That transport materializes a fresh payload on receive, so the round trip includes payload work on both the request side and the reply side.",
    "To keep the Tokio baseline fair, `src/main.rs` clones `String` and `Vec<u8>` on send and also clones again on the worker reply. The reply clone is intentional. Without it, Tokio would be measuring a cheaper return-path move while the JS runtimes were still paying for fresh payload materialization on the way back.",
    "The `Arc<Vec<u8>>` sweep is intentionally separate and is not the default apples-to-apples byte benchmark. It exists as an upper-bound shared-bytes reference for small payloads. `Arc::clone` only bumps a refcount, so it is expected to be cheaper than copying bytes.",
    'This means the default `string` and `Uint8Array` tables should be read as the fairer comparison, while the Arc section should be read as "how close does the normal transport get to shared ownership for small values?"',
)


@dataclass(frozen=True)
class BenchRow:
    runtime_key: str
    runtime_label: str
    generated_at_unix_ms: int
    benchmark_key: str
    benchmark_label: str
    column_kind: str
    column_value: int
    avg_ns: float
    p99_ns: float


def run_text(command: list[str]) -> str | None:
    if shutil.which(command[0]) is None:
        return None

    try:
        completed = subprocess.run(
            command,
            check=True,
            capture_output=True,
            text=True,
        )
    except (OSError, subprocess.SubprocessError):
        return None

    output = completed.stdout.strip()
    return output or None


def parse_colon_table(text: str) -> dict[str, str]:
    parsed: dict[str, str] = {}
    for line in text.splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        parsed[key.strip()] = value.strip()
    return parsed


def format_bytes_binary(value: int) -> str:
    gib = 1024**3
    mib = 1024**2
    kib = 1024
    if value >= gib:
        amount = value / gib
        return f"{amount:.1f} GiB" if amount % 1 else f"{int(amount)} GiB"
    if value >= mib:
        amount = value / mib
        return f"{amount:.1f} MiB" if amount % 1 else f"{int(amount)} MiB"
    if value >= kib:
        amount = value / kib
        return f"{amount:.1f} KiB" if amount % 1 else f"{int(amount)} KiB"
    return f"{value} B"


def read_os_pretty_name() -> str | None:
    os_release = Path("/etc/os-release")
    if not os_release.exists():
        return None

    for line in os_release.read_text(encoding="utf8").splitlines():
        if not line.startswith("PRETTY_NAME="):
            continue
        return line.split("=", 1)[1].strip().strip('"')

    return None


def read_meminfo_bytes(field: str) -> int | None:
    meminfo = Path("/proc/meminfo")
    if not meminfo.exists():
        return None

    for line in meminfo.read_text(encoding="utf8").splitlines():
        if not line.startswith(field):
            continue
        parts = line.split()
        if len(parts) < 2:
            return None
        try:
            kib = int(parts[1])
        except ValueError:
            return None
        return kib * 1024

    return None


def machine_specs() -> list[tuple[str, str]]:
    specs: list[tuple[str, str]] = []

    os_name = read_os_pretty_name()
    if os_name is None:
        os_name = platform.platform()
    specs.append(("OS", os_name))

    kernel = platform.release().strip()
    if kernel:
        specs.append(("Kernel", kernel))

    architecture = platform.machine().strip()
    if architecture:
        specs.append(("Architecture", architecture))

    lscpu_output = run_text(["lscpu"])
    lscpu_info = parse_colon_table(lscpu_output) if lscpu_output else {}

    cpu_model = lscpu_info.get("Model name") or platform.processor().strip()
    if cpu_model:
        specs.append(("CPU", cpu_model))

    logical_cpus = lscpu_info.get("CPU(s)")
    if logical_cpus is None:
        count = os.cpu_count()
        logical_cpus = str(count) if count is not None else None

    sockets = lscpu_info.get("Socket(s)")
    cores_per_socket = lscpu_info.get("Core(s) per socket")
    threads_per_core = lscpu_info.get("Thread(s) per core")
    topology_parts: list[str] = []
    if logical_cpus:
        topology_parts.append(f"{logical_cpus} logical CPUs")
    if sockets:
        topology_parts.append(f"{sockets} socket(s)")
    if cores_per_socket:
        topology_parts.append(f"{cores_per_socket} core(s)/socket")
    if threads_per_core:
        topology_parts.append(f"{threads_per_core} thread(s)/core")
    if topology_parts:
        specs.append(("Topology", ", ".join(topology_parts)))

    mem_total = read_meminfo_bytes("MemTotal:")
    if mem_total is not None:
        specs.append(("Memory", format_bytes_binary(mem_total)))

    swap_total = read_meminfo_bytes("SwapTotal:")
    if swap_total is not None:
        specs.append(("Swap", format_bytes_binary(swap_total)))

    return specs


def format_ns(ns: float) -> str:
    if ns >= 1_000_000:
        return f"{ns / 1_000_000:.2f} ms"
    if ns >= 1_000:
        return f"{ns / 1_000:.2f} us"
    return f"{ns:.2f} ns"


def format_ratio(value: float | None) -> str:
    if value is None:
        return "-"
    return f"{value:.2f}x"


def format_binary_bytes(value: int) -> str:
    if value >= 1024 * 1024:
        return f"{value // (1024 * 1024)} MiB"
    if value >= 1024:
        return f"{value // 1024} KiB"
    return f"{value} B"


def staggered_binary_bytes(value: int, index: int) -> str:
    label = format_binary_bytes(value)
    return label if index % 2 == 0 else f"\n{label}"


def short_runtime_key(row: dict[str, str]) -> str:
    runtime = row["runtime"].strip().lower()
    implementation = row["implementation"].strip().lower()
    if implementation == "tokio" or runtime.startswith("tokio"):
        return "tokio"
    for candidate in RUNTIME_ORDER:
        if runtime.startswith(candidate):
            return candidate
    return implementation or "unknown"


def normalize_benchmark(name: str) -> tuple[str, str]:
    lower = name.lower()
    if "arc comparison" in lower and "size sweep" in lower:
        return ARC_COMPARE_SIZE_SWEEP_BENCHMARK
    if "size sweep" in lower:
        return SIZE_SWEEP_BENCHMARK
    if "number" in lower and "f64" in lower:
        return BATCH_BENCHMARKS[0]
    if "large string" in lower:
        return BATCH_BENCHMARKS[1]
    if "uint8array" in lower:
        return BATCH_BENCHMARKS[2]
    slug = lower.replace(" ", "_")
    return slug, name


def detect_latest_files(results_dir: Path) -> dict[str, Path]:
    latest: dict[str, tuple[int, Path]] = {}

    for path in sorted(results_dir.glob("*.csv")):
        with path.open(newline="", encoding="utf8") as handle:
            reader = csv.DictReader(handle)
            first = next(reader, None)

        if not first:
            continue

        runtime_key = short_runtime_key(first)
        generated_at = int(first["generated_at_unix_ms"])
        previous = latest.get(runtime_key)
        if previous is None or generated_at > previous[0]:
            latest[runtime_key] = (generated_at, path)

    return {runtime: path for runtime, (_, path) in latest.items()}


def load_rows(csv_paths: Iterable[Path]) -> list[BenchRow]:
    rows: list[BenchRow] = []

    for path in csv_paths:
        with path.open(newline="", encoding="utf8") as handle:
            reader = csv.DictReader(handle)
            for raw in reader:
                runtime_key = short_runtime_key(raw)
                benchmark_key, benchmark_label = normalize_benchmark(raw["benchmark"])
                rows.append(
                    BenchRow(
                        runtime_key=runtime_key,
                        runtime_label=RUNTIME_LABELS.get(runtime_key, runtime_key),
                        generated_at_unix_ms=int(raw["generated_at_unix_ms"]),
                        benchmark_key=benchmark_key,
                        benchmark_label=benchmark_label,
                        column_kind=raw["column_kind"],
                        column_value=int(raw["column_value"]),
                        avg_ns=float(raw["avg_ns"]),
                        p99_ns=float(raw["p99_ns"]),
                    )
                )

    return rows


def available_runtimes(rows: list[BenchRow]) -> list[str]:
    present = {row.runtime_key for row in rows}
    return [runtime for runtime in RUNTIME_ORDER if runtime in present]


def build_table(headers: list[str], rows: list[list[str]]) -> str:
    widths = [len(header) for header in headers]
    for row in rows:
        for index, cell in enumerate(row):
            widths[index] = max(widths[index], len(cell))

    def render_row(row: list[str]) -> str:
        return " | ".join(cell.ljust(widths[index]) for index, cell in enumerate(row))

    separator = "-+-".join("-" * width for width in widths)
    lines = [render_row(headers), separator]
    lines.extend(render_row(row) for row in rows)
    return "\n".join(lines)


def batch_table(rows: list[BenchRow], runtimes: list[str], metric: str) -> str:
    lookup = {
        (row.benchmark_key, row.column_value, row.runtime_key): row
        for row in rows
        if row.column_kind == "batch"
    }
    headers = ["benchmark", "batch", *[RUNTIME_LABELS.get(runtime, runtime) for runtime in runtimes]]
    table_rows: list[list[str]] = []

    for benchmark_key, benchmark_label in BATCH_BENCHMARKS:
        for batch in (1, 10, 100):
            cells = [benchmark_label, f"n={batch}"]
            for runtime in runtimes:
                row = lookup.get((benchmark_key, batch, runtime))
                if row is None:
                    cells.append("-")
                else:
                    value = row.avg_ns if metric == "avg" else row.p99_ns
                    cells.append(format_ns(value))
            table_rows.append(cells)

    return build_table(headers, table_rows)


def ratio_table(rows: list[BenchRow], runtimes: list[str]) -> str:
    lookup = {
        (row.benchmark_key, row.column_value, row.runtime_key): row
        for row in rows
        if row.column_kind == "batch"
    }
    compare_runtimes = [runtime for runtime in runtimes if runtime != "tokio"]
    headers = ["benchmark", "batch", *[f"{RUNTIME_LABELS.get(runtime, runtime)}/tokio" for runtime in compare_runtimes]]
    table_rows: list[list[str]] = []

    for benchmark_key, benchmark_label in BATCH_BENCHMARKS:
        for batch in (1, 10, 100):
            tokio_row = lookup.get((benchmark_key, batch, "tokio"))
            cells = [benchmark_label, f"n={batch}"]
            for runtime in compare_runtimes:
                runtime_row = lookup.get((benchmark_key, batch, runtime))
                if tokio_row is None or runtime_row is None:
                    cells.append("-")
                else:
                    cells.append(format_ratio(runtime_row.avg_ns / tokio_row.avg_ns))
            table_rows.append(cells)

    return build_table(headers, table_rows)


def sweep_table(rows: list[BenchRow], runtimes: list[str], benchmark_key: str) -> str:
    sweep_rows = [row for row in rows if row.benchmark_key == benchmark_key]
    sizes = sorted({row.column_value for row in sweep_rows})
    lookup = {(row.column_value, row.runtime_key): row for row in sweep_rows}
    headers = ["size", *[RUNTIME_LABELS.get(runtime, runtime) for runtime in runtimes]]
    table_rows: list[list[str]] = []

    for size in sizes:
        cells = [format_binary_bytes(size)]
        for runtime in runtimes:
            row = lookup.get((size, runtime))
            cells.append("-" if row is None else format_ns(row.avg_ns))
        table_rows.append(cells)

    return build_table(headers, table_rows)


def sweep_ratio_table(rows: list[BenchRow], runtimes: list[str], benchmark_key: str) -> str:
    sweep_rows = [row for row in rows if row.benchmark_key == benchmark_key]
    sizes = sorted({row.column_value for row in sweep_rows})
    lookup = {(row.column_value, row.runtime_key): row for row in sweep_rows}
    compare_runtimes = [runtime for runtime in runtimes if runtime != "tokio"]
    headers = ["size", *[f"{RUNTIME_LABELS.get(runtime, runtime)}/tokio" for runtime in compare_runtimes]]
    table_rows: list[list[str]] = []

    for size in sizes:
        tokio_row = lookup.get((size, "tokio"))
        cells = [format_binary_bytes(size)]
        for runtime in compare_runtimes:
            runtime_row = lookup.get((size, runtime))
            if tokio_row is None or runtime_row is None:
                cells.append("-")
            else:
                cells.append(format_ratio(runtime_row.avg_ns / tokio_row.avg_ns))
        table_rows.append(cells)

    return build_table(headers, table_rows)


def batch_series(
    rows: list[BenchRow],
    benchmark_key: str,
    runtimes: list[str],
) -> dict[str, list[BenchRow]]:
    series: dict[str, list[BenchRow]] = {}
    for runtime in runtimes:
        runtime_rows = sorted(
            [
                row
                for row in rows
                if row.benchmark_key == benchmark_key
                and row.column_kind == "batch"
                and row.runtime_key == runtime
            ],
            key=lambda row: row.column_value,
        )
        if runtime_rows:
            series[runtime] = runtime_rows
    return series


def batch_chart_filename(benchmark_key: str) -> str:
    return f"batch_avg_{benchmark_key}_log.svg"


def configure_axes(ax: plt.Axes, runtimes: list[str], title: str, subtitle: str) -> None:
    ax.set_facecolor(DARK_AXES_HEX)
    ax.set_title(title, loc="left", fontsize=17, color=TITLE_HEX, pad=22)
    ax.text(0.0, 1.03, subtitle, transform=ax.transAxes, color=TEXT_HEX, fontsize=11)
    ax.grid(True, axis="y", which="major", linestyle="--", linewidth=0.9, alpha=0.9)
    ax.legend(
        loc="lower left",
        bbox_to_anchor=(0.0, 1.08),
        ncol=min(len(runtimes), 4),
        frameon=True,
        fontsize=10,
    )
    ax.yaxis.set_major_formatter(FuncFormatter(lambda value, _: format_ns(value)))


def write_batch_chart(
    benchmark_label: str,
    series: dict[str, list[BenchRow]],
    runtimes: list[str],
    output_path: Path,
) -> None:
    if not series:
        return

    apply_dark_style()
    fig, ax = plt.subplots(figsize=(8.8, 6.2), constrained_layout=True)
    fig.set_facecolor(DARK_BG_HEX)

    for runtime in runtimes:
        runtime_rows = series.get(runtime)
        if not runtime_rows:
            continue
        ax.plot(
            [row.column_value for row in runtime_rows],
            [row.avg_ns for row in runtime_rows],
            color=RUNTIME_COLORS[runtime],
            marker="o",
            linewidth=2.5,
            markersize=6,
            label=RUNTIME_LABELS[runtime],
        )

    configure_axes(
        ax,
        runtimes,
        "Batch Avg Latency (less is better)",
        benchmark_label,
    )
    ax.set_yscale("log")
    ax.set_xlabel("batch size")
    ax.set_ylabel("avg latency")
    ax.set_xticks([1, 10, 100], ["n=1", "n=10", "n=100"])
    ax.tick_params(axis="x", pad=8)
    fig.savefig(output_path, format="svg")
    plt.close(fig)


def write_size_sweep_chart(
    rows: list[BenchRow],
    runtimes: list[str],
    benchmark_key: str,
    title: str,
    subtitle: str,
    output_path: Path,
) -> None:
    sweep_rows = [row for row in rows if row.benchmark_key == benchmark_key]
    if not sweep_rows:
        return

    sizes = sorted({row.column_value for row in sweep_rows})

    apply_dark_style()
    fig_width = 11.0 if len(sizes) <= 8 else 15.5
    fig_height = 6.8 if len(sizes) <= 8 else 8.2
    fig, ax = plt.subplots(figsize=(fig_width, fig_height), constrained_layout=True)
    fig.set_facecolor(DARK_BG_HEX)

    for runtime in runtimes:
        runtime_rows = sorted(
            [row for row in sweep_rows if row.runtime_key == runtime],
            key=lambda row: row.column_value,
        )
        if not runtime_rows:
            continue
        ax.plot(
            [row.column_value for row in runtime_rows],
            [row.avg_ns for row in runtime_rows],
            color=RUNTIME_COLORS[runtime],
            marker="o",
            linewidth=2.3,
            markersize=4.8,
            label=RUNTIME_LABELS[runtime],
        )

    configure_axes(
        ax,
        runtimes,
        title,
        subtitle,
    )
    ax.set_xscale("log", base=2)
    ax.set_yscale("log")
    ax.set_xlabel("payload size")
    ax.set_ylabel("avg latency")
    ax.set_xticks(sizes, [staggered_binary_bytes(size, index) for index, size in enumerate(sizes)])
    ax.tick_params(axis="x", labelsize=9, pad=10)
    ax.grid(True, axis="x", which="major", linestyle=":", linewidth=0.7, alpha=0.45)
    fig.savefig(output_path, format="svg")
    plt.close(fig)


def write_summary(
    output_path: Path,
    source_paths: dict[str, Path],
    specs: list[tuple[str, str]],
    batch_avg: str,
    batch_p99: str,
    ratios: str,
    sweep: str,
    arc_compare_sweep: str,
    arc_compare_ratios: str,
) -> None:
    lines = ["# Benchmark Summary", "", "## Sources", ""]
    for runtime in RUNTIME_ORDER:
        path = source_paths.get(runtime)
        if path is not None:
            lines.append(f"- {RUNTIME_LABELS[runtime]}: `{path.as_posix()}`")

    if specs:
        lines.extend(["", "## Machine Specs", ""])
        for label, value in specs:
            lines.append(f"- {label}: {value}")

    lines.extend(["", "## Methodology Notes", ""])
    for note in METHODOLOGY_NOTES:
        lines.append(f"- {note}")

    lines.extend(
        [
            "",
            "## Batch Avg Latency (less is better)",
            "",
            "```text",
            batch_avg,
            "```",
            "",
            "## Batch P99 Latency (less is better)",
            "",
            "```text",
            batch_p99,
            "```",
            "",
            "## Avg Ratio Vs Tokio",
            "",
            "```text",
            ratios,
            "```",
            "",
            "## Uint8Array Size Sweep Avg Latency (less is better)",
            "",
            "```text",
            sweep,
            "```",
            "",
            "## Arc Comparison Size Sweep Avg Latency (less is better)",
            "",
            "Tokio uses `Arc<Vec<u8>>` here as a separate shared-bytes reference point, not the default apples-to-apples byte path.",
            "",
            "```text",
            arc_compare_sweep,
            "```",
            "",
            "## Arc Comparison Avg Ratio Vs Tokio",
            "",
            "```text",
            arc_compare_ratios,
            "```",
            "",
        ]
    )
    output_path.write_text("\n".join(lines), encoding="utf8")


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate comparison tables and charts from benchmark CSVs.")
    parser.add_argument("--results-dir", default="results")
    parser.add_argument("--out-dir", default="results/graphs")
    args = parser.parse_args()

    results_dir = Path(args.results_dir)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    latest_files = detect_latest_files(results_dir)
    if not latest_files:
        raise SystemExit("No CSV benchmark results found in results/. Run a --csv benchmark first.")

    rows = load_rows(latest_files.values())
    runtimes = available_runtimes(rows)
    if not runtimes:
        raise SystemExit("No supported runtime data found in CSV files.")

    specs = machine_specs()
    batch_avg = batch_table(rows, runtimes, "avg")
    batch_p99 = batch_table(rows, runtimes, "p99")
    ratios = ratio_table(rows, runtimes)
    sweep = sweep_table(rows, runtimes, SIZE_SWEEP_BENCHMARK[0])
    arc_compare_sweep = sweep_table(rows, runtimes, ARC_COMPARE_SIZE_SWEEP_BENCHMARK[0])
    arc_compare_ratios = sweep_ratio_table(rows, runtimes, ARC_COMPARE_SIZE_SWEEP_BENCHMARK[0])

    old_batch_chart_path = out_dir / "batch_avg_log.svg"
    old_batch_chart_path.unlink(missing_ok=True)

    batch_chart_paths: list[Path] = []
    for benchmark_key, benchmark_label in BATCH_BENCHMARKS:
        chart_path = out_dir / batch_chart_filename(benchmark_key)
        write_batch_chart(
            benchmark_label,
            batch_series(rows, benchmark_key, runtimes),
            runtimes,
            chart_path,
        )
        batch_chart_paths.append(chart_path)

    sweep_chart_path = out_dir / "uint8array_size_sweep_avg_log.svg"
    arc_compare_chart_path = out_dir / "uint8array_arc_compare_size_sweep_avg_log.svg"
    summary_path = out_dir / "summary.md"
    results_path = out_dir / "results.md"

    write_size_sweep_chart(
        rows,
        runtimes,
        SIZE_SWEEP_BENCHMARK[0],
        "Uint8Array Size Sweep (less is better)",
        "batch=100, log-scale x and y axes",
        sweep_chart_path,
    )
    write_size_sweep_chart(
        rows,
        runtimes,
        ARC_COMPARE_SIZE_SWEEP_BENCHMARK[0],
        "Arc Comparison Size Sweep (less is better)",
        "batch=100, tokio uses Arc<Vec<u8>> as a separate reference, log-scale x and y axes",
        arc_compare_chart_path,
    )
    write_summary(
        summary_path,
        latest_files,
        specs,
        batch_avg,
        batch_p99,
        ratios,
        sweep,
        arc_compare_sweep,
        arc_compare_ratios,
    )
    write_summary(
        results_path,
        latest_files,
        specs,
        batch_avg,
        batch_p99,
        ratios,
        sweep,
        arc_compare_sweep,
        arc_compare_ratios,
    )

    print("\n== Batch Avg Latency (less is better) ==")
    print(batch_avg)
    print("\n== Batch P99 Latency (less is better) ==")
    print(batch_p99)
    print("\n== Avg Ratio Vs Tokio ==")
    print(ratios)
    print("\n== Uint8Array Size Sweep Avg Latency (less is better) ==")
    print(sweep)
    print("\n== Arc Comparison Size Sweep Avg Latency (less is better) ==")
    print(arc_compare_sweep)
    print("\n== Arc Comparison Avg Ratio Vs Tokio ==")
    print(arc_compare_ratios)
    print(f"\nsummary: {summary_path.as_posix()}")
    print(f"results: {results_path.as_posix()}")
    for chart_path in batch_chart_paths:
        print(f"chart: {chart_path.as_posix()}")
    print(f"chart: {sweep_chart_path.as_posix()}")
    print(f"chart: {arc_compare_chart_path.as_posix()}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
