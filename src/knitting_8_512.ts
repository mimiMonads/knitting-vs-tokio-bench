// Knitting benchmark for Arc comparison
// Usage: node --no-warnings --experimental-transform-types knitting_bench.ts

import { createPool, task, isMain } from "../knitting/knitting.ts";

const BATCH_SIZE = 100;
const ITERATIONS = 500;
const WARMUP = 50;

const sizes = [8, 16, 32, 64, 128, 256, 512];

const nowNs = (): bigint => {
  const hrtime = (globalThis as any).process?.hrtime?.bigint;
  if (hrtime) return hrtime();
  return BigInt(Math.round(globalThis.performance.now() * 1_000_000));
};

const fmtNs = (ns: number): string => {
  if (ns >= 1_000_000) return `${(ns / 1_000_000).toFixed(2)}ms`;
  if (ns >= 1_000) return `${(ns / 1_000).toFixed(0)}Âµs`;
  return `${ns.toFixed(0)}ns`;
};

export const echoBytes = task<Uint8Array, Uint8Array>({
  f: (value) => value,
});

let sink = 0;

if (isMain) {
  const pool = createPool({
    threads: 1,
    payload: {
      payloadInitialBytes: 16 * 1024,
      payloadMaxByteLength: 256 * 1024,
    },
  })({ echoBytes });

  console.log("knitting (node) Arc comparison benchmark");
  console.log(`batch=${BATCH_SIZE}, iterations=${ITERATIONS}, warmup=${WARMUP}\n`);
  console.log("size       avg        p99");
  console.log("-".repeat(30));

  try {
    for (const bytes of sizes) {
      const payloads = [
        new Uint8Array(bytes).fill(0xAB),
        new Uint8Array(bytes).fill(0xBC),
        new Uint8Array(bytes).fill(0xCD),
        new Uint8Array(bytes).fill(0xDE),
      ];

      const samples: number[] = [];

      for (let i = 0; i < ITERATIONS + WARMUP; i++) {
        const start = nowNs();

        const jobs = new Array(BATCH_SIZE);
        for (let j = 0; j < BATCH_SIZE; j++) {
          const payloadIdx = (i + j) % payloads.length;
          jobs[j] = pool.call.echoBytes(payloads[payloadIdx]!);
        }
        const values = await Promise.all(jobs);
        for (const value of values) sink ^= value.byteLength;

        const elapsedNs = Number(nowNs() - start);
        if (i >= WARMUP) samples.push(elapsedNs);
      }

      samples.sort((a, b) => a - b);
      const avg = samples.reduce((sum, x) => sum + x, 0) / samples.length;
      const p99 = samples[Math.floor(samples.length * 0.99)]!;

      console.log(`${bytes}B`.padEnd(10), fmtNs(avg).padStart(10), fmtNs(p99).padStart(10));
    }
  } finally {
    await pool.shutdown();
  }

  if (sink === Number.MIN_SAFE_INTEGER) {
    console.log("unreachable", sink);
  }
}