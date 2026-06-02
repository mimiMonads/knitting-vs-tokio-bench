# tokio-bench vs knitting

measures a simple echo path in two scenarios:
- Rust `tokio::sync::mpsc` (send -> receive -> respond)
- JS/TS [`knitting`](https://www.npmjs.com/package/knitting) (worker -> return -> main thread)

This is not a competition to declare a winner. It is a small, repetitive task designed to show trade-offs, particularly when payload size is a factor.

## This benchmark's measurements

Three payload forms and whole-batch latency:
- `f64`: nothing to copy, just scheduling overhead
- `Uint8Array` / raw bytes (1 MiB) – two-way copying
- `ProcessSharedBuffer` / `Arc<Vec<u8>>` (1 MiB) — **zero-copy**: only a refcount bump (Tokio) or a fd reference (knitting) passes over the channel

Tokio's Arc upper bound is also provided in a targeted `Uint8Array` `8 B → 512 B` sweep that matches the small-payload region.

The reporting setup is identical across all runtimes:
- Batch sizes: 1, 10, 100
- Warmup: `200` iterations if `n=1`; otherwise `50`
- Measured iterations: 500
- The timing of each batch
- Sorted samples: avg, min, p75, p99, and max

## Results

Average whole-batch latency of one run (`knitting 0.1.51`, `tokio 1.x`, `bun 1.3.11`, `deno 2.7.4`, `node 24.15.0`). Pay attention to the *shape* of the numbers, not the exact numbers, since these are one machine on one day. To replicate, run `npm run bench:all:csv` again.

**Pure scheduling overhead: `f64`**

| runtime | n=1 | n=10 | n=100 |
| --- | --- | --- | --- |
| tokio | 4.6 µs | 9.6 µs | 52.8 µs |
| knitting (bun) | 8.8 µs | 14.9 µs | 69.6 µs |
| knitting (node) | 9.1 µs | 16.2 µs | 53.8 µs |
| knitting (deno) | 12.8 µs | 15.4 µs | 57.1 µs |

The worker round trip is in the same order of magnitude.

**`Uint8Array` 1 MiB — complete copy in both directions**

| runtime | n=1 | n=10 | n=100 |
| --- | --- | --- | --- |
| tokio | 105 µs | 4.29 ms | 37.7 ms |
| knitting (bun) | 550 µs | 4.14 ms | 31.3 ms |
| knitting (node) | 1.33 ms | 4.59 ms | 47.6 ms |
| knitting (deno) | 657 µs | 6.00 ms | 55.2 ms |

The shared-buffer copy amortises better under load than per-message `Vec` cloning, so Tokio wins the single call (no SAB encode/decode), but by `n=10` knitting/bun pulls level and is ~17% ahead at `n=100`.

**Zero-copy 1 MiB – `Arc<Vec<u8>>` (tokio) vs. `ProcessSharedBuffer` (knitting)**

| runtime | n=1 | n=10 | n=100 |
| --- | --- | --- | --- |
| tokio | 4.6 µs | 10.2 µs | 68.9 µs |
| knitting (node) | 20.4 µs | 20.5 µs | 81.6 µs |
| knitting (deno) | 20.7 µs | 20.4 µs | 108 µs |
| knitting (bun) | 9.0 µs | 29.1 µs | 130 µs |

Tokio's `Arc` refcount bump is still the least expensive handoff, with neither side copying — but the margin narrowed sharply in knitting `0.1.51`. At `n=100`, Tokio (~69 µs) is only ~1.2x ahead of knitting/node (~82 µs) and ~1.9x ahead of knitting/bun (~130 µs), versus the ~3–4x seen in earlier knitting releases. Although encoding the shared-memory fd metadata through the SAB transport still costs more than a pointer move, both remain in the tens to hundreds of microseconds, which is far less than the millisecond copy path mentioned above.

The two are equivalent for modest scheduling-bound work; on a pure zero-copy handoff, knitting's preallocated shared buffer wins under batching on the 1 MiB copy path. The least expensive option is still Tokio's `Arc`.

## Equity

Three skew sources are addressed:

- **The dispatch shape is aligned.** Rust fans out via launched tasks and waits with `join_all(...)`, matching knitting producing all `pool.call.*(...)` promises and awaiting `Promise.all(...)`.
- **Threads aligned.** Rust uses `#[tokio::main(worker_threads = 1)]`; Knitting uses `threads: 1`.
- **Zero-copy.** `ProcessSharedBuffer` (fd reference only) vs. `Arc<Vec<u8>>` (refcount bump only). The difference is only channel and scheduling cost, since both benchmarks have 1 MiB and there is no data movement in any direction.

Copy path memory management is a deliberate imbalance.

In the timed part, the Rust `Vec<u8>` benchmark rewards `clone()` (heap allocation + memcpy). Knitting copies into a preallocated `SharedArrayBuffer` with accounting similar to that of an allocator. Since knitting involves avoiding general-purpose allocation in the hot path, the benchmark keeps this difference in-bounds rather than concealing it.

### The little arc sweep (8 B → 512 B)

This sweep is kept apart and capped at `512 B`. After that, it is no longer an apples-to-apples transport comparison because you are comparing copying against a shared-reference handoff. Instead of reading it as the default byte-path result, read it as a small-size scheduling lower-bound for Rust. The dedicated `Arc<Vec<u8>>` vs. `ProcessSharedBuffer` bench is the honest 1 MiB zero-copy comparison.

## A crude cost model (how to understand results)

Consider the benchmark as assessing two distinct "systems" for the payload-heavy echo cases:

- **Knitting:** Allocator-style area management combined with shared-buffer copies (JS values still materialise when a worker reads or returns them).
- **Tokio:** Clone-driven allocation plus payload copies on the channel path.

The high-level point is stable: knitting purchases speed by substituting preallocated shared-memory management for recurrent general-purpose allocation.

## Why knitting can be quick

For this benchmark, knitting matters in a few specific ways:

- **Simpler queues due to fixed pool structure.** Each host-worker lane is essentially single-producer/single-consumer, and the pool is aware of its employees up front. Compared to a completely general multi-producer channel, that is less expensive.
- The majority of transport work takes place inside reused task objects and buffers backed by typed arrays, which lowers GC burden and allocation churn.
- **The idle policy is spin-then-park, with GC incorporated into the dead time.** When a worker runs out of work, it first initiates a GC pass, busy-spins with `Atomics.pause` for a budget of `spinMicroseconds`, and only parks on `Atomics.wait` once that budget has passed (see `worker/timers.ts`). The spin budget allows a rapidly coming next call to be picked up mid-spin, avoiding the futex wakeup cost that otherwise dominates single-call delay, and collecting *while idle* keeps GC pauses out of the active task route (steadier tails).
- Larger payloads overflow into the shared payload buffer (SAB/GSAB), while smaller payloads encode inline in the per-call header slot (~480 bytes useable).
- Large payloads are copied into a preallocated `SharedArrayBuffer` divided into 64-byte-aligned chunks that are tracked by a tiny slot table/bitset.
- **Primitives are "header-only".** There is no payload buffer at all; numbers, booleans, null, etc. are encoded directly in header words. Knitting can transmit an OS-level shared-memory fd reference across the channel for substantial data that persists beyond a single call. **`ProcessSharedBuffer` is completely zero-copy.** The backing memory is never copied; only the fd metadata (~40 bytes) passes across the transport.

All of this is not free; it sacrifices simplicity in favour of more "allocator-like" engineering and meticulous memory layout. This repository is specifically attempting to make that deal visible.

## Requirements

- Rust stable toolchain
- Bun `1+`
- Deno `2+`
- Node `22+`

## Install

### Rust

```bash
cargo build --release
```

### JS deps

```bash
npm install
```

### Deno (optional)

```bash
deno install
```

### Plotting (optional)

```bash
python3 -m venv .venv
.venv/bin/pip install matplotlib
```

## Run

### Quick start (bench + CSV + charts)

```bash
npm run bench:all:csv
```

Runs Tokio, Bun, Deno, and Node sequentially, writes timestamped CSVs into `results/`, and generates charts under `results/graphs/`.

### All runtimes (console tables only)

```bash
npm run bench:all
```

### Quick resource benchmark

```bash
npm run bench:resources:quick
```

Runs a short fixed-duration workload under GNU `/usr/bin/time` and records elapsed seconds, CPU%, max RSS, and voluntary context switches for each runtime.

### Single-runtime runs

```bash
npm run bench:rust          # Tokio
bun run src/main.ts         # Bun
npm run bench:deno          # Deno
npm run bench:node          # Node
```

### CSV output

Add `--csv` to any run to write a timestamped file into `results/`:

```bash
cargo run --release --quiet -- --csv
bun run src/main.ts --csv
deno run -A src/main.ts --csv
node src/main.ts --csv
```

Or use the packaged scripts:

```bash
npm run bench:rust:csv
npm run bench:bun:csv
npm run bench:deno:csv
npm run bench:node:csv
```

If you already have CSVs and only want to rebuild charts:

```bash
npm run bench:report
```

## Notes

- The TypeScript entrypoint detects the runtime at startup, so the same file runs under Bun, Deno, and Node.
- Always run Rust in `--release` mode for any meaningful comparison.
- The report step uses the latest CSV per runtime found in `results/` and expects `matplotlib` in `.venv/`.
