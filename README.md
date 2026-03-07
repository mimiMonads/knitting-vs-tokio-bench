# knitting-vs-tokio-bench

This repo compares a Rust `tokio::sync::mpsc` echo path against a JavaScript/TypeScript [`@vixeny/knitting`](https://jsr.io/@vixeny/knitting) echo path.

The point is not to claim that one abstraction is universally better than the other. The point is to measure what each design actually costs on the same simple workload, especially once payload size starts to matter.

The benchmark measures whole-batch latency for three payload shapes:

- `f64`
- `String` / large UTF-8 text
- `Uint8Array` / raw bytes

Both sides use the same reporting shape:

- fixed batch sizes: `1`, `10`, `100`
- fixed warmup: `200` iterations for `n=1`, `50` otherwise
- fixed measured iterations: `500`
- per-batch timing
- sorted samples with `avg`, `min`, `p75`, `p99`, `max`

## Sample Results

Example output from one local run on the machine hard-coded in the benchmark header (`Ryzen 9 5950X ~4.55GHz`), with Tokio pinned to `worker_threads = 1` and knitting configured with `threads: 1`:

```text
$ cargo run --release --quiet
cpu: Ryzen 9 5950X ~4.55GHz
runtime: tokio 1.x mpsc (worker_threads = 1)
task: send payload -> worker echo -> return, join_all
(whole-batch latency; warmup n=1: 200, others: 50)
(string/bytes use 4 payload variants rotated with index % 4)

--- large string: 1MB (1048576 bytes) ---
batch             avg          min          p75          p99          max
----------------------------------------------------------------------
n=1          74.53 µs     47.02 µs     89.33 µs    138.22 µs    672.62 µs
n=10          2.80 ms      1.55 ms      3.16 ms      4.54 ms      5.47 ms
n=100        62.88 ms     46.97 ms     67.06 ms     76.92 ms     94.66 ms

--- number: f64 (8 bytes) ---
batch             avg          min          p75          p99          max
----------------------------------------------------------------------
n=1           8.22 µs      6.62 µs      8.03 µs     13.60 µs    150.75 µs
n=10          9.14 µs      6.40 µs      9.27 µs     10.77 µs    112.05 µs
n=100        43.61 µs     37.46 µs     41.47 µs    234.96 µs    299.49 µs

--- Uint8Array: 1MB (1048576 bytes) ---
batch             avg          min          p75          p99          max
----------------------------------------------------------------------
n=1         248.63 µs     49.65 µs    214.41 µs      2.49 ms      5.51 ms
n=10          4.96 ms      2.58 ms      5.32 ms      8.40 ms     12.07 ms
n=100        65.84 ms     49.63 ms     70.64 ms     77.37 ms     81.08 ms

$ bun run src/main.ts
runtime: bun 1.2.20
task: send payload -> worker echo -> return, join_all
(whole-batch latency; warmup n=1: 200, others: 50)
(string/bytes use 4 payload variants rotated with index % 4)

--- knitting large string (1048576 bytes) ---
batch             avg          min          p75          p99          max
----------------------------------------------------------------------
n=1           2.17 ms    568.08 µs      2.04 ms     12.87 ms     20.88 ms
n=10          7.87 ms      5.14 ms      8.59 ms     14.08 ms     16.69 ms
n=100        65.61 ms     57.76 ms     67.90 ms     87.63 ms     98.38 ms

--- knitting number f64 (8 bytes) ---
batch             avg          min          p75          p99          max
----------------------------------------------------------------------
n=1           4.37 µs      2.09 µs      4.21 µs     16.34 µs     69.44 µs
n=10          9.41 µs      6.08 µs     10.31 µs     20.40 µs     66.67 µs
n=100        49.62 µs     36.18 µs     51.70 µs     94.05 µs    175.59 µs

--- knitting Uint8Array (1048576 bytes) ---
batch             avg          min          p75          p99          max
----------------------------------------------------------------------
n=1           1.38 ms    501.05 µs      1.76 ms      3.49 ms      3.85 ms
n=10          6.47 ms      4.68 ms      7.16 ms      9.47 ms     10.19 ms
n=100        58.98 ms     44.50 ms     61.07 ms     76.16 ms     84.59 ms
```

Treat those numbers as a concrete scale reference, not a universal truth. If you run this on another CPU, memory subsystem, runtime version, or under a different system load, the absolute numbers will move.

## How To Read It

Three of the big benchmark-shape problems have already been fixed.

- Dispatch is aligned. Rust now fans requests out concurrently with spawned tasks and waits with `join_all(...)`, which matches knitting creating all `pool.call.*(...)` promises and then awaiting `Promise.all(...)`.
- Timing is aligned. The TypeScript side no longer uses `mitata`; both implementations now use the same hand-rolled warmup, iteration, timing, and summary logic.
- Runtime width is aligned. Knitting uses `threads: 1`, and the Rust benchmark uses `#[tokio::main(worker_threads = 1)]` so the sender fan-out cannot spread across a larger Tokio worker pool.

That leaves one important asymmetry, and it is intentional: memory management.

## Allocation Model

This benchmark is framed as "total cost of the system as designed", not "transport cost after normalizing allocation away".

So for large string and byte payloads, this difference is part of the result:

- Rust `String` / `Vec<u8>` paths pay `clone()`, which means heap allocation plus memcpy in the timed section.
- Knitting pays shared-memory copies into a preallocated region managed by its own allocator-like bookkeeping.

That is not something this README is trying to hide or explain away. Avoiding `malloc` in the hot path is part of what makes knitting interesting, so the benchmark should say that plainly.

If the goal changes to isolating pure channel / IPC overhead, then the benchmark should be reworked to pre-clone payloads or share them behind `Arc` on the Rust side.

## What Knitting Is Buying

Knitting is faster on these payload-heavy paths partly because it does more work up front in the runtime design:

- pre-allocates shared memory
- tracks sectors / regions inside that shared buffer
- reuses that space instead of going through the general-purpose allocator on every call

That is a real architectural win, but it is also real engineering complexity. In practice, it means taking on allocator-style concerns such as reuse rules, fragmentation behavior, reclamation timing, and making sure live regions are never treated as free.

Tokio is taking the opposite tradeoff. The Rust path is simpler: clone the owned value, send it through the channel, and let Rust ownership plus the system allocator handle memory management. That is easier to reason about, but it also means paying allocation cost in the hot path.

So the intended takeaway is not "knitting IPC is faster than tokio channels" in the abstract.

It is: for this workload, knitting's architecture of pre-allocated shared memory plus custom allocator-style region management outperforms tokio's clone-through-channel approach. That framing matters because the benchmark is showing the payoff of a deliberate design trade: more memory-management complexity in exchange for lower hot-path allocation cost.

## Rough Cost Model

For the payload-heavy echo cases, the asymmetry should be treated as part of the story:

- knitting: shared-buffer copies plus allocator-style region management, with JS values still being materialized when the worker reads or returns them
- tokio: clone-driven allocation and payload copies on the channel path

The exact low-level behavior depends on payload type and runtime, so this README keeps that part qualitative. The high-level point is stable: knitting is buying speed by replacing repeated general-purpose allocation with preallocated shared-memory management.

## Requirements

- Rust stable toolchain
- Bun `1.2+`
- Deno `2+`
- Node `23+` for direct `.ts` execution

Node 23 currently prints an experimental warning because built-in type stripping is still marked experimental. The benchmark still runs.

## Install

### Rust

```bash
cargo build --release
```

### Bun

```bash
bun install
```

### Node

```bash
npm install
```

### Deno

If you want Deno to manage the npm dependency itself:

```bash
deno install
```

If you already ran `bun install` or `npm install`, Deno can also use the existing `node_modules` tree in this repo.

## Run

### Run every runtime

```bash
npm run bench:all
```

This runs Tokio, Bun, Deno, and Node sequentially with the normal console tables.

### Rust benchmark

```bash
cargo run --release --quiet
```

Or:

```bash
npm run bench:rust
```

### Bun benchmark

```bash
bun run src/main.ts
```

Or:

```bash
bun run bench:bun
```

### Deno benchmark

```bash
deno run -A src/main.ts
```

Or:

```bash
npm run bench:deno
```

### Node benchmark

```bash
node src/main.ts
```

Or:

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
