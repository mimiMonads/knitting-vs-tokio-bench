# tokio_ipc_bench

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

## How To Read It

Two of the big benchmark-shape problems have already been fixed.

- Dispatch is aligned. Rust now fans requests out concurrently with spawned tasks and waits with `join_all(...)`, which matches knitting creating all `pool.call.*(...)` promises and then awaiting `Promise.all(...)`.
- Timing is aligned. The TypeScript side no longer uses `mitata`; both implementations now use the same hand-rolled warmup, iteration, timing, and summary logic.

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

## Notes

- The TypeScript benchmark prints the detected runtime at startup, so the same entrypoint can be used under Bun, Deno, or Node.
- Rust should be run in `--release` mode for any meaningful comparison.
