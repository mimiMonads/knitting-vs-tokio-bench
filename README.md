# knitting-vs-tokio-bench

Benchmarks a simple "echo path" in two worlds:

- Rust `tokio::sync::mpsc` (send → receive → reply)
- JS/TS [`@vixeny/knitting`](https://jsr.io/@vixeny/knitting) (main thread → worker → return)

This is not trying to crown a universal winner. It’s a small, repeatable workload meant to make the tradeoffs visible, especially once payload size starts to matter.

## What this benchmark measures

Whole-batch latency for three payload shapes:

- `f64`
- `String` / large UTF-8 text
- `Uint8Array` / raw bytes

All runtimes use the same reporting setup:

- batch sizes: `1`, `10`, `100`
- warmup: `200` iterations for `n=1`, `50` otherwise
- measured iterations: `500`
- per-batch timing
- sorted samples: `avg`, `min`, `p75`, `p99`, `max`

## Fairness and the one intentional asymmetry

Two major sources of skew are already handled:

- **Dispatch shape is aligned.** Rust fans out via spawned tasks and waits with `join_all(...)`, matching knitting creating all `pool.call.*(...)` promises and awaiting `Promise.all(...)`.
- **Runtime width is aligned.** Knitting uses `threads: 1`, and Rust uses `#[tokio::main(worker_threads = 1)]`, so sender fan-out can't spread across a bigger worker pool.

One asymmetry is kept on purpose: **memory management**.

### Allocation model

This benchmark measures "total cost of the system as designed", not "transport cost after normalizing allocation away". Large payloads have to be copied or shared somehow, and that choice is part of the cost.

For large string and byte payloads:

- Rust `String` / `Vec<u8>` pays `clone()` (heap allocation + memcpy) in the timed section.
- Knitting copies into a preallocated shared-memory region managed by its own allocator-like bookkeeping.

Avoiding general-purpose allocation in the hot path is part of what makes knitting interesting, so the benchmark keeps that cost in-bounds rather than hiding it.

If you want to isolate pure channel / IPC overhead instead, rework the Rust side to pre-clone payloads (or share behind `Arc`) so allocation isn't in the critical section.

## A rough cost model (how to read results)

For the payload-heavy echo cases, treat the benchmark as measuring two different "systems":

- **knitting:** shared-buffer copies + allocator-style region management (JS values still get materialized when a worker reads/returns them)
- **tokio:** clone-driven allocation + payload copies on the channel path

The exact low-level behavior depends on payload type and runtime, but the high-level point is stable: knitting is buying speed by replacing repeated general-purpose allocation with preallocated shared-memory management.

## Why knitting can be fast (and why it's not "physics-breaking")

A few concrete things knitting does that matter for this benchmark:

- **Fixed pool topology → simpler queues.** The pool knows its workers up front, and each host↔worker lane is effectively single‑producer/single‑consumer. That's cheaper than a fully general multi‑producer channel.
- **Low-garbage hot path.** Most transport work happens inside typed-array-backed buffers and reused task objects, reducing allocation churn and GC pressure (and references get cleared quickly after each call settles).
- **Two-tier payload path.** Small payloads encode inline in the per-call header slot (roughly ~0.5 KiB per in-flight call, with ~480 bytes usable for inline data); larger payloads spill into the shared payload buffer (SAB/GSAB).
- **Shared payload buffer + mini allocator.** Large payloads are copied into a preallocated `SharedArrayBuffer` and carved into 64‑byte‑aligned regions tracked by a small slot table/bitset (more complexity, less `malloc` in the hot path).
- **Primitives are "header-only".** Numbers/booleans/null/etc encode directly in header words (no payload buffer at all), keeping contention and copying low.
- **Optional "gc at idle boundaries".** When workers have `gc()` available (for example via Node's `--expose-gc`), knitting may trigger a GC before going into longer spin/park waits, nudging collections away from the hot loop.

None of this is free: it trades simplicity for careful memory layout, extra bookkeeping, and more "allocator-like" engineering. That trade is exactly what this repo is trying to make visible.

## Requirements

- Rust stable toolchain
- Bun `1.2+` (for Bun runs)
- Deno `2+` (for Deno runs)
- Node `23+` (for Node runs, and for `npm run bench:all`)

Node 23 currently prints an experimental warning because built-in type stripping is still marked experimental. The benchmark still runs.

## Install

### Rust

```bash
cargo build --release
```

### JS deps (pick one)

```bash
bun install
```

Or:

```bash
npm install
```

### Deno (optional)

If you want Deno to manage the npm dependency itself:

```bash
deno install
```

If you already ran `bun install` or `npm install`, Deno can also use the existing `node_modules` tree in this repo.

### Plotting (optional)

The report charts use `matplotlib` from a repo-local virtualenv:

```bash
python3 -m venv .venv
.venv/bin/pip install matplotlib
```

## Run

### Quick start (bench + CSV + charts)

```bash
npm run bench:all:csv
```

This runs Tokio, Bun, Deno, and Node sequentially, writes timestamped CSVs into `results/`, and then generates the report under `results/graphs/`.

### Run every runtime (console tables only)

```bash
npm run bench:all
```

### Single-runtime runs

Rust:

```bash
npm run bench:rust
```

Bun:

```bash
bun run src/main.ts
```

Deno:

```bash
npm run bench:deno
```

Node:

```bash
npm run bench:node
```

### CSV output mode

Add `--csv` to any benchmark command to keep the console tables and also write a timestamped CSV into `results/`.

Examples:

```bash
cargo run --release --quiet -- --csv
bun run src/main.ts --csv
deno run -A src/main.ts --csv
node src/main.ts --csv
```

Or use the packaged scripts:

```bash
npm run bench:all:csv
npm run bench:rust:csv
bun run bench:bun:csv
npm run bench:deno:csv
npm run bench:node:csv
```

`npm run bench:all:csv` also generates:

- `results/graphs/summary.md`
- `results/graphs/batch_avg_number_f64_log.svg`
- `results/graphs/batch_avg_large_string_1mb_log.svg`
- `results/graphs/batch_avg_uint8array_1mb_log.svg`
- `results/graphs/uint8array_size_sweep_avg_log.svg`

If you already have CSV files and only want to rebuild the tables / charts:

```bash
npm run bench:report
```

## Notes

- The TypeScript benchmark prints the detected runtime at startup, so the same entrypoint can be used under Bun, Deno, or Node.
- Rust should be run in `--release` mode for any meaningful comparison.
- The report step uses the latest CSV per runtime found in `results/`.
- `bench:report` expects `matplotlib` to be installed in `.venv/`.
