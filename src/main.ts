import { createPool, isMain, task } from "knitting";
import { ProcessSharedBuffer } from "knitting/process-shared-buffer";

import { ITERATIONS, WARMUP_N1, WARMUP, warmupIters, summarizeSamples, pushRecord, runBench } from "./bench.ts";
import type { BenchRecord } from "./bench.ts";
import { writeCsvReport } from "./csv.ts";
import { fmtBinaryBytes, printHeader, printStats } from "./format.ts";
import { makeBytePayloads, makePsbPayloads, powerOfTwoBytes } from "./payloads.ts";
import { runtimeName, nowNs, cliArgs } from "./runtime.ts";

const PAYLOAD_BYTES = 1024 * 1024;
const UINT8ARRAY_SIZE_SWEEP_BATCH = 100;
const UINT8ARRAY_SIZE_SWEEP_MIN_BYTES = 8;
const UINT8ARRAY_SIZE_SWEEP_MAX_BYTES = PAYLOAD_BYTES;
const ARC_COMPARE_SIZE_SWEEP_BATCH = 100;
const ARC_COMPARE_SIZE_SWEEP_MIN_BYTES = 8;
const ARC_COMPARE_SIZE_SWEEP_MAX_BYTES = 512;
const ARC_COMPARE_PAYLOAD_INITIAL_BYTES = 16 * 1024;
const ARC_COMPARE_PAYLOAD_MAX_BYTES = 256 * 1024;

// ── Tasks ────────────────────────────────────────────────────────────────────
// Every task is an echo: the worker receives a value and returns it unchanged.
// The cost being measured is the round trip (main -> worker -> main), not any
// computation. Three payload shapes exercise three different transport paths:
//
//   echoF64                 — a primitive; encodes inline in the call header,
//                             so this is pure scheduling overhead, nothing copied.
//   echoBytes               — 1 MiB Uint8Array; spills into the shared payload
//                             buffer and is copied in both directions.
//   echoProcessSharedBuffer — 1 MiB in OS shared memory; only the fd reference
//                             (~40 bytes) travels, so zero bytes are copied.

export const echoF64 = task({
  f: (value: number): number => value,
});

export const echoBytes = task({
  f: (value: Uint8Array): Uint8Array => value,
});

export const echoProcessSharedBuffer = task({
  f: (value: ProcessSharedBuffer): ProcessSharedBuffer => value,
});

// ── Main ─────────────────────────────────────────────────────────────────────

let sink = 0;

if (isMain) {
  const csv = cliArgs().includes("--csv");
  const runtime = runtimeName();
  const generatedAtUnixMs = Date.now();
  const records: BenchRecord[] = [];

  const pool = createPool({ threads: 1  ,})({
    echoF64,
    echoBytes,
    echoProcessSharedBuffer,
  });

  const bytePayloads = makeBytePayloads(PAYLOAD_BYTES);
  const psbPayloads = makePsbPayloads(PAYLOAD_BYTES);
  const uint8ArraySweepSizes = powerOfTwoBytes(UINT8ARRAY_SIZE_SWEEP_MIN_BYTES, UINT8ARRAY_SIZE_SWEEP_MAX_BYTES);
  const arcCompareSweepSizes = powerOfTwoBytes(ARC_COMPARE_SIZE_SWEEP_MIN_BYTES, ARC_COMPARE_SIZE_SWEEP_MAX_BYTES);

  console.log(`runtime: ${runtime}`);
  console.log("task: send payload -> worker echo -> return, join_all");
  console.log(`(whole-batch latency; warmup n=1: ${WARMUP_N1}, others: ${WARMUP})`);
  console.log("(bytes rotate 4 payload variants per batch)");
  console.log("(ProcessSharedBuffer passes the fd reference only — zero bytes copied)");

  try {
    // ── f64: pure scheduling overhead, nothing copied ──
    await runBench(
      "knitting f64 (8 bytes)",
      (n) => async () => {
        const jobs = Array.from({ length: n }, () => pool.call.echoF64(42));
        const values = await Promise.all(jobs);
        for (const value of values) sink ^= value | 0;
      },
      records, runtime, generatedAtUnixMs,
    );

    // ── Uint8Array copy (1 MiB): payload copied into the shared buffer both ways ──
    await runBench(
      `knitting Uint8Array (${fmtBinaryBytes(PAYLOAD_BYTES)})`,
      (n) => {
        let turn = 0;
        return async () => {
          const jobs = new Array<Promise<Uint8Array>>(n);
          for (let j = 0; j < n; j++) jobs[j] = pool.call.echoBytes(bytePayloads[(turn + j) % bytePayloads.length]!);
          const values = await Promise.all(jobs);
          for (const value of values) sink ^= value.byteLength;
          turn++;
        };
      },
      records, runtime, generatedAtUnixMs,
    );

    // ── ProcessSharedBuffer: zero-copy, only fd reference travels ──
    await runBench(
      `knitting ProcessSharedBuffer (${fmtBinaryBytes(PAYLOAD_BYTES)})`,
      (n) => {
        let turn = 0;
        return async () => {
          const jobs = new Array<Promise<ProcessSharedBuffer>>(n);
          for (let j = 0; j < n; j++) jobs[j] = pool.call.echoProcessSharedBuffer(psbPayloads[(turn + j) % 4]!);
          const values = await Promise.all(jobs);
          for (const value of values) sink ^= value.byteLength;
          turn++;
        };
      },
      records, runtime, generatedAtUnixMs,
    );

    // ── Uint8Array size sweep: shows how copy cost scales with payload size ──
    printHeader(
      `knitting Uint8Array size sweep (batch=${UINT8ARRAY_SIZE_SWEEP_BATCH}, ${fmtBinaryBytes(UINT8ARRAY_SIZE_SWEEP_MIN_BYTES)} -> ${fmtBinaryBytes(UINT8ARRAY_SIZE_SWEEP_MAX_BYTES)})`,
      "size",
    );
    const sweepWarmup = warmupIters(UINT8ARRAY_SIZE_SWEEP_BATCH);
    for (const bytes of uint8ArraySweepSizes) {
      const payloads = makeBytePayloads(bytes);
      const samples: number[] = [];
      let turn = 0;
      for (let i = 0; i < ITERATIONS + sweepWarmup; i++) {
        const start = nowNs();
        const jobs = new Array<Promise<Uint8Array>>(UINT8ARRAY_SIZE_SWEEP_BATCH);
        for (let j = 0; j < UINT8ARRAY_SIZE_SWEEP_BATCH; j++) jobs[j] = pool.call.echoBytes(payloads[(turn + j) % payloads.length]!);
        const values = await Promise.all(jobs);
        for (const value of values) sink ^= value.byteLength;
        const elapsedNs = Number(nowNs() - start);
        if (i >= sweepWarmup) samples.push(elapsedNs);
        turn++;
      }
      const stats = summarizeSamples(samples);
      const columnLabel = fmtBinaryBytes(bytes);
      printStats(columnLabel, stats);
      pushRecord(records, runtime, generatedAtUnixMs, `knitting Uint8Array size sweep (batch=${UINT8ARRAY_SIZE_SWEEP_BATCH})`, "size_bytes", bytes, columnLabel, sweepWarmup, stats);
    }

    // ── Arc comparison sweep: smaller buffer pool to match the Rust Arc sweep range ──
    const arcComparePool = createPool({
      threads: 1,
      payload: {
        payloadInitialBytes: ARC_COMPARE_PAYLOAD_INITIAL_BYTES,
        payloadMaxByteLength: ARC_COMPARE_PAYLOAD_MAX_BYTES,
      },
    })({ echoBytes });

    try {
      printHeader(
        `Uint8Array arc comparison size sweep (batch=${ARC_COMPARE_SIZE_SWEEP_BATCH}, ${fmtBinaryBytes(ARC_COMPARE_SIZE_SWEEP_MIN_BYTES)} -> ${fmtBinaryBytes(ARC_COMPARE_SIZE_SWEEP_MAX_BYTES)})`,
        "size",
      );
      const arcWarmup = warmupIters(ARC_COMPARE_SIZE_SWEEP_BATCH);
      for (const bytes of arcCompareSweepSizes) {
        const payloads = makeBytePayloads(bytes);
        const samples: number[] = [];
        let turn = 0;
        for (let i = 0; i < ITERATIONS + arcWarmup; i++) {
          const start = nowNs();
          const jobs = new Array<Promise<Uint8Array>>(ARC_COMPARE_SIZE_SWEEP_BATCH);
          for (let j = 0; j < ARC_COMPARE_SIZE_SWEEP_BATCH; j++) jobs[j] = arcComparePool.call.echoBytes(payloads[(turn + j) % payloads.length]!);
          const values = await Promise.all(jobs);
          for (const value of values) sink ^= value.byteLength;
          const elapsedNs = Number(nowNs() - start);
          if (i >= arcWarmup) samples.push(elapsedNs);
          turn++;
        }
        const stats = summarizeSamples(samples);
        const columnLabel = fmtBinaryBytes(bytes);
        printStats(columnLabel, stats);
        pushRecord(records, runtime, generatedAtUnixMs, `Uint8Array arc comparison size sweep (batch=${ARC_COMPARE_SIZE_SWEEP_BATCH})`, "size_bytes", bytes, columnLabel, arcWarmup, stats);
      }
    } finally {
      await arcComparePool.shutdown();
    }
  } finally {
    await pool.shutdown();
  }

  if (csv) await writeCsvReport(records);

  if (sink === Number.MIN_SAFE_INTEGER) console.log("unreachable", sink);
}
