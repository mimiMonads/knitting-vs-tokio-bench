from __future__ import annotations

import argparse
import csv
import math
from dataclasses import dataclass
from html import escape
from pathlib import Path
from typing import Iterable

from plot_style import (
    AXIS_HEX,
    DARK_AXES_HEX,
    DARK_BG_HEX,
    GRID_HEX,
    LEGEND_BG_HEX,
    LEGEND_EDGE_HEX,
    RUNTIME_COLORS,
    RUNTIME_LABELS,
    RUNTIME_ORDER,
    TEXT_HEX,
    TITLE_HEX,
)


BATCH_BENCHMARKS = (
    ("number_f64", "number f64 (8 bytes)"),
    ("large_string_1mb", "large string 1 MiB"),
    ("uint8array_1mb", "Uint8Array 1 MiB"),
)
SIZE_SWEEP_BENCHMARK = ("uint8array_size_sweep", "Uint8Array size sweep (batch=100)")


@dataclass(frozen=True)
class BenchRow:
    runtime_key: str
    runtime_label: str
    source_file: str
    generated_at_unix_ms: int
    benchmark_key: str
    benchmark_label: str
    column_kind: str
    column_value: int
    column_label: str
    avg_ns: float
    p99_ns: float


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
                        source_file=path.name,
                        generated_at_unix_ms=int(raw["generated_at_unix_ms"]),
                        benchmark_key=benchmark_key,
                        benchmark_label=benchmark_label,
                        column_kind=raw["column_kind"],
                        column_value=int(raw["column_value"]),
                        column_label=raw["column_label"],
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
        (row.benchmark_key, row.column_value, row.runtime_key): row for row in rows if row.column_kind == "batch"
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
        (row.benchmark_key, row.column_value, row.runtime_key): row for row in rows if row.column_kind == "batch"
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


def sweep_table(rows: list[BenchRow], runtimes: list[str]) -> str:
    sweep_rows = [row for row in rows if row.benchmark_key == SIZE_SWEEP_BENCHMARK[0]]
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


def svg_line_chart(
    width: int,
    height: int,
    body: list[str],
) -> str:
    return "\n".join(
        [
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
            f'<rect x="0" y="0" width="{width}" height="{height}" fill="{DARK_BG_HEX}" />',
            *body,
            "</svg>",
        ]
    )


def svg_text(x: float, y: float, text: str, fill: str = TEXT_HEX, size: int = 14, anchor: str = "start") -> str:
    return (
        f'<text x="{x:.2f}" y="{y:.2f}" fill="{fill}" font-size="{size}" '
        f'font-family="IBM Plex Sans, Segoe UI, sans-serif" text-anchor="{anchor}">{escape(text)}</text>'
    )


def svg_line(x1: float, y1: float, x2: float, y2: float, stroke: str, width: float = 1.0, dash: str | None = None) -> str:
    dash_attr = f' stroke-dasharray="{dash}"' if dash else ""
    return (
        f'<line x1="{x1:.2f}" y1="{y1:.2f}" x2="{x2:.2f}" y2="{y2:.2f}" '
        f'stroke="{stroke}" stroke-width="{width:.2f}"{dash_attr} />'
    )


def svg_rect(x: float, y: float, width: float, height: float, fill: str, stroke: str | None = None) -> str:
    stroke_attr = f' stroke="{stroke}"' if stroke else ""
    return (
        f'<rect x="{x:.2f}" y="{y:.2f}" width="{width:.2f}" height="{height:.2f}" '
        f'fill="{fill}"{stroke_attr} />'
    )


def svg_polyline(points: list[tuple[float, float]], stroke: str) -> str:
    rendered = " ".join(f"{x:.2f},{y:.2f}" for x, y in points)
    return f'<polyline points="{rendered}" fill="none" stroke="{stroke}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" />'


def svg_circle(x: float, y: float, radius: float, fill: str) -> str:
    return f'<circle cx="{x:.2f}" cy="{y:.2f}" r="{radius:.2f}" fill="{fill}" />'


def log_bounds(values: list[float]) -> tuple[float, float]:
    positive = [value for value in values if value > 0]
    minimum = min(positive)
    maximum = max(positive)
    lower = math.floor(math.log10(minimum))
    upper = math.ceil(math.log10(maximum))
    if lower == upper:
        upper += 1
    return lower, upper


def log_ticks(lower: float, upper: float) -> list[float]:
    return [10 ** exponent for exponent in range(int(lower), int(upper) + 1)]


def scale_log(value: float, lower: float, upper: float, size: float) -> float:
    return (math.log10(value) - lower) / (upper - lower) * size


def write_runtime_legend(body: list[str], runtimes: list[str], x: float, y: float) -> None:
    item_width = 100
    box_width = len(runtimes) * item_width + 20
    body.append(svg_rect(x - 10, y - 14, box_width, 36, LEGEND_BG_HEX, LEGEND_EDGE_HEX))

    cursor = x
    for runtime in runtimes:
        color = RUNTIME_COLORS[runtime]
        body.append(svg_line(cursor + 10, y, cursor + 34, y, color, 3))
        body.append(svg_circle(cursor + 22, y, 4, color))
        body.append(svg_text(cursor + 44, y + 5, RUNTIME_LABELS[runtime], TEXT_HEX, 14))
        cursor += item_width


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


def write_batch_chart(
    benchmark_label: str,
    series: dict[str, list[BenchRow]],
    runtimes: list[str],
    output_path: Path,
) -> None:
    all_values = [row.avg_ns for runtime_rows in series.values() for row in runtime_rows]
    if not all_values:
        return

    width = 840
    height = 620
    plot_left = 138
    plot_top = 180
    plot_right = width - 54
    plot_bottom = height - 118
    plot_width = plot_right - plot_left
    plot_height = plot_bottom - plot_top
    batch_positions = {1: plot_left, 10: plot_left + plot_width / 2, 100: plot_right}

    lower, upper = log_bounds(all_values)
    ticks = log_ticks(lower, upper)
    body = [
        svg_text(70, 60, "Batch Avg Latency (less is better)", TITLE_HEX, 27),
        svg_text(70, 92, benchmark_label, TEXT_HEX, 18),
        svg_text(70, 118, "log-scale y axis", TEXT_HEX, 14),
    ]
    write_runtime_legend(body, runtimes, 70, 148)
    body.append(
        svg_rect(
            plot_left - 26,
            plot_top - 24,
            plot_width + 52,
            plot_height + 48,
            DARK_AXES_HEX,
            AXIS_HEX,
        )
    )

    for tick in ticks:
        y = plot_bottom - scale_log(tick, lower, upper, plot_height)
        body.append(svg_line(plot_left, y, plot_right, y, GRID_HEX, 1, "4 6"))
        body.append(svg_text(plot_left - 12, y + 5, format_ns(tick), TEXT_HEX, 12, "end"))

    for batch, x in batch_positions.items():
        body.append(svg_line(x, plot_bottom, x, plot_bottom + 8, AXIS_HEX, 1.5))
        body.append(svg_text(x, plot_bottom + 28, f"n={batch}", TEXT_HEX, 12, "middle"))

    body.append(svg_line(plot_left, plot_top, plot_left, plot_bottom, AXIS_HEX, 1.5))
    body.append(svg_line(plot_left, plot_bottom, plot_right, plot_bottom, AXIS_HEX, 1.5))

    for runtime in runtimes:
        runtime_rows = series.get(runtime)
        if not runtime_rows:
            continue

        points = []
        for row in runtime_rows:
            x = batch_positions[row.column_value]
            y = plot_bottom - scale_log(row.avg_ns, lower, upper, plot_height)
            points.append((x, y))
        body.append(svg_polyline(points, RUNTIME_COLORS[runtime]))
        for x, y in points:
            body.append(svg_circle(x, y, 4.5, RUNTIME_COLORS[runtime]))

    body.append(svg_text(plot_right - 4, plot_bottom + 66, "batch size", TEXT_HEX, 13, "end"))
    body.append(svg_text(plot_left - 94, plot_top - 8, "avg latency", TEXT_HEX, 13))

    output_path.write_text(svg_line_chart(width, height, body), encoding="utf8")


def write_size_sweep_chart(rows: list[BenchRow], runtimes: list[str], output_path: Path) -> None:
    sweep_rows = [row for row in rows if row.benchmark_key == SIZE_SWEEP_BENCHMARK[0]]
    if not sweep_rows:
        return

    width = 1600
    height = 820
    left = 132
    top = 180
    right = width - 60
    bottom = height - 140
    plot_width = right - left
    plot_height = bottom - top

    sizes = sorted({row.column_value for row in sweep_rows})
    min_size = min(sizes)
    max_size = max(sizes)
    all_values = [row.avg_ns for row in sweep_rows]
    lower_y, upper_y = log_bounds(all_values)
    ticks_y = log_ticks(lower_y, upper_y)
    min_x = math.log2(min_size)
    max_x = math.log2(max_size)

    def scale_x(size: int) -> float:
        return left + (math.log2(size) - min_x) / (max_x - min_x) * plot_width

    body = [
        svg_text(70, 60, "Uint8Array Size Sweep (less is better)", TITLE_HEX, 28),
        svg_text(70, 92, "batch=100, log-scale x and y axes", TEXT_HEX, 16),
    ]
    write_runtime_legend(body, runtimes, 70, 132)
    body.append(svg_rect(left - 24, top - 24, plot_width + 48, plot_height + 52, DARK_AXES_HEX, AXIS_HEX))

    for tick in ticks_y:
        y = bottom - scale_log(tick, lower_y, upper_y, plot_height)
        body.append(svg_line(left, y, right, y, GRID_HEX, 1, "4 6"))
        body.append(svg_text(left - 12, y + 5, format_ns(tick), TEXT_HEX, 12, "end"))

    for index, size in enumerate(sizes):
        x = scale_x(size)
        body.append(svg_line(x, top, x, bottom, GRID_HEX, 1, "3 7"))
        label_y = bottom + 30 if index % 2 == 0 else bottom + 48
        body.append(svg_text(x, label_y, format_binary_bytes(size), TEXT_HEX, 11, "middle"))

    body.append(svg_line(left, top, left, bottom, AXIS_HEX, 1.5))
    body.append(svg_line(left, bottom, right, bottom, AXIS_HEX, 1.5))

    for runtime in runtimes:
        runtime_rows = sorted(
            [row for row in sweep_rows if row.runtime_key == runtime],
            key=lambda row: row.column_value,
        )
        if not runtime_rows:
            continue

        points = []
        for row in runtime_rows:
            x = scale_x(row.column_value)
            y = bottom - scale_log(row.avg_ns, lower_y, upper_y, plot_height)
            points.append((x, y))
        body.append(svg_polyline(points, RUNTIME_COLORS[runtime]))
        for x, y in points:
            body.append(svg_circle(x, y, 3.5, RUNTIME_COLORS[runtime]))

    body.append(svg_text(right - 4, bottom + 92, "payload size", TEXT_HEX, 13, "end"))
    body.append(svg_text(left - 100, top - 10, "avg latency", TEXT_HEX, 13))

    output_path.write_text(svg_line_chart(width, height, body), encoding="utf8")


def write_summary(
    output_path: Path,
    source_paths: dict[str, Path],
    batch_avg: str,
    batch_p99: str,
    ratios: str,
    sweep: str,
) -> None:
    lines = ["# Benchmark Summary", "", "## Sources", ""]
    for runtime in RUNTIME_ORDER:
        path = source_paths.get(runtime)
        if path is not None:
            lines.append(f"- {RUNTIME_LABELS[runtime]}: `{path.as_posix()}`")

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

    batch_avg = batch_table(rows, runtimes, "avg")
    batch_p99 = batch_table(rows, runtimes, "p99")
    ratios = ratio_table(rows, runtimes)
    sweep = sweep_table(rows, runtimes)

    old_batch_chart_path = out_dir / "batch_avg_log.svg"
    if old_batch_chart_path.exists():
        old_batch_chart_path.unlink()

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
    summary_path = out_dir / "summary.md"

    write_size_sweep_chart(rows, runtimes, sweep_chart_path)
    write_summary(summary_path, latest_files, batch_avg, batch_p99, ratios, sweep)

    print("\n== Batch Avg Latency (less is better) ==")
    print(batch_avg)
    print("\n== Batch P99 Latency (less is better) ==")
    print(batch_p99)
    print("\n== Avg Ratio Vs Tokio ==")
    print(ratios)
    print("\n== Uint8Array Size Sweep Avg Latency (less is better) ==")
    print(sweep)
    print(f"\nsummary: {summary_path.as_posix()}")
    for chart_path in batch_chart_paths:
        print(f"chart: {chart_path.as_posix()}")
    print(f"chart: {sweep_chart_path.as_posix()}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
