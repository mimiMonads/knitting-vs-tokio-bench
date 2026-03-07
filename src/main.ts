import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { createPool, isMain, task } from "@vixeny/knitting";

const BATCH_SIZES = [1, 10, 100] as const;
const ITERATIONS = 500;
const WARMUP = 50;
const WARMUP_N1 = 200;
const PAYLOAD_BYTES = 1024 * 1024;
const PAYLOAD_INITIAL_BYTES = 16 * 1024 * 1024;
const PAYLOAD_MAX_BYTES = 256 * 1024 * 1024;
const BYTE_FILL_VALUES = [0xAB, 0xBC, 0xCD, 0xDE] as const;
const UINT8ARRAY_SIZE_SWEEP_BATCH = 100;
const UINT8ARRAY_SIZE_SWEEP_MIN_BYTES = 8;
const UINT8ARRAY_SIZE_SWEEP_MAX_BYTES = PAYLOAD_BYTES;
const LABEL_COLUMN_WIDTH = 10;

type ColumnKind = "batch" | "size_bytes";

type BenchStats = {
  avgNs: number;
  minNs: number;
  p75Ns: number;
  p99Ns: number;
  maxNs: number;
};

type BenchRecord = {
  implementation: "knitting";
  runtime: string;
  generatedAtUnixMs: number;
  benchmark: string;
  columnKind: ColumnKind;
  columnValue: number;
  columnLabel: string;
  iterations: number;
  warmup: number;
  avgNs: number;
  minNs: number;
  p75Ns: number;
  p99Ns: number;
  maxNs: number;
};

type CliOptions = {
  csv: boolean;
};

const warmupIters = (batch: number) => (batch === 1 ? WARMUP_N1 : WARMUP);
const uint8ArraySizeSweepBytes = (() => {
  const sizes: number[] = [];
  for (
    let bytes = UINT8ARRAY_SIZE_SWEEP_MIN_BYTES;
    bytes <= UINT8ARRAY_SIZE_SWEEP_MAX_BYTES;
    bytes *= 2
  ) {
    sizes.push(bytes);
  }
  return sizes;
})();
const runtimeGlobals = globalThis as typeof globalThis & {
  Bun?: { version: string };
  Deno?: { args: string[]; version: { deno: string } };
  process?: {
    argv?: string[];
    versions?: { node?: string };
    hrtime?: { bigint?: () => bigint };
  };
};

const cliArgs = (): string[] => {
  if (runtimeGlobals.Deno) return runtimeGlobals.Deno.args;
  return runtimeGlobals.process?.argv?.slice(2) ?? [];
};

const parseCliOptions = (): CliOptions => ({
  csv: cliArgs().includes("--csv"),
});

const fmtNs = (ns: number): string => {
  if (ns >= 1_000_000) return `${(ns / 1_000_000).toFixed(2)} ms`;
  if (ns >= 1_000) return `${(ns / 1_000).toFixed(2)} \u00B5s`;
  return `${ns.toFixed(2)} ns`;
};

const fmtBinaryBytes = (bytes: number): string => {
  if (bytes >= 1024 * 1024) return `${bytes / (1024 * 1024)} MiB`;
  if (bytes >= 1024) return `${bytes / 1024} KiB`;
  return `${bytes} B`;
};

const printHeader = (label: string, columnLabel = "batch"): void => {
  console.log(`\n--- ${label} ---`);
  console.log(
    `${columnLabel.padEnd(LABEL_COLUMN_WIDTH)} ${"avg".padStart(12)} ${"min".padStart(12)} ${"p75".padStart(12)} ${"p99".padStart(12)} ${"max".padStart(12)}`,
  );
  console.log("-".repeat(70));
};

const summarizeSamples = (samples: number[]): BenchStats => {
  samples.sort((a, b) => a - b);

  const len = samples.length;
  return {
    avgNs: samples.reduce((sum, sample) => sum + sample, 0) / len,
    minNs: samples[0]!,
    p75Ns: samples[Math.floor((len * 75) / 100)]!,
    p99Ns: samples[Math.floor((len * 99) / 100)]!,
    maxNs: samples[len - 1]!,
  };
};

const printStats = (label: string, stats: BenchStats): void => {
  console.log(
    `${label.padEnd(LABEL_COLUMN_WIDTH)} ${fmtNs(stats.avgNs).padStart(12)} ${fmtNs(stats.minNs).padStart(12)} ${fmtNs(stats.p75Ns).padStart(12)} ${fmtNs(stats.p99Ns).padStart(12)} ${fmtNs(stats.maxNs).padStart(12)}`,
  );
};

const makeBytePayloads = (bytes: number): Uint8Array[] =>
  BYTE_FILL_VALUES.map((fillValue) => new Uint8Array(bytes).fill(fillValue));

const makeEchoBytesBatch = (
  n: number,
  payloads: readonly Uint8Array[],
  echoBytes: (value: Uint8Array) => Promise<Uint8Array>,
): (() => Promise<void>) => {
  let turn = 0;

  return async () => {
    const jobs = new Array<Promise<Uint8Array>>(n);
    for (let j = 0; j < n; j++) {
      const index = (turn + j) % payloads.length;
      jobs[j] = echoBytes(payloads[index]!);
    }
    const values = await Promise.all(jobs);
    for (const value of values) sink ^= value.byteLength;
    turn++;
  };
};

const runtimeName = (): string => {
  if (runtimeGlobals.Bun) return `bun ${runtimeGlobals.Bun.version}`;
  if (runtimeGlobals.Deno) return `deno ${runtimeGlobals.Deno.version.deno}`;
  if (runtimeGlobals.process?.versions?.node) {
    return `node ${runtimeGlobals.process.versions.node}`;
  }
  return "unknown";
};

const runtimeId = (): string => {
  if (runtimeGlobals.Bun) return "bun";
  if (runtimeGlobals.Deno) return "deno";
  if (runtimeGlobals.process?.versions?.node) return "node";
  return "unknown";
};

const nowNs = (): bigint => {
  const hrtime = runtimeGlobals.process?.hrtime?.bigint;
  if (hrtime) return hrtime();
  return BigInt(Math.round(globalThis.performance.now() * 1_000_000));
};

const pushRecord = (
  records: BenchRecord[],
  runtime: string,
  generatedAtUnixMs: number,
  benchmark: string,
  columnKind: ColumnKind,
  columnValue: number,
  columnLabel: string,
  warmup: number,
  stats: BenchStats,
): void => {
  records.push({
    implementation: "knitting",
    runtime,
    generatedAtUnixMs,
    benchmark,
    columnKind,
    columnValue,
    columnLabel,
    iterations: ITERATIONS,
    warmup,
    avgNs: stats.avgNs,
    minNs: stats.minNs,
    p75Ns: stats.p75Ns,
    p99Ns: stats.p99Ns,
    maxNs: stats.maxNs,
  });
};

const csvEscape = (value: string | number): string => {
  const stringValue = String(value);
  if (/[",\n]/.test(stringValue)) {
    return `"${stringValue.replaceAll('"', '""')}"`;
  }
  return stringValue;
};

const serializeCsv = (records: readonly BenchRecord[]): string => {
  const header = [
    "implementation",
    "runtime",
    "generated_at_unix_ms",
    "benchmark",
    "column_kind",
    "column_value",
    "column_label",
    "iterations",
    "warmup",
    "avg_ns",
    "min_ns",
    "p75_ns",
    "p99_ns",
    "max_ns",
  ].join(",");

  const lines = records.map((record) =>
    [
      record.implementation,
      record.runtime,
      record.generatedAtUnixMs,
      record.benchmark,
      record.columnKind,
      record.columnValue,
      record.columnLabel,
      record.iterations,
      record.warmup,
      record.avgNs,
      record.minNs,
      record.p75Ns,
      record.p99Ns,
      record.maxNs,
    ]
      .map(csvEscape)
      .join(","),
  );

  return `${header}\n${lines.join("\n")}\n`;
};

const writeCsvReport = async (records: readonly BenchRecord[]): Promise<void> => {
  if (records.length === 0) return;

  await mkdir("results", { recursive: true });

  const outputPath = join(
    "results",
    `knitting-${runtimeId()}-${records[0]!.generatedAtUnixMs}.csv`,
  );

  await writeFile(outputPath, serializeCsv(records), "utf8");
  console.log(`csv: ${outputPath}`);
};

const runBench = async (
  label: string,
  makeRunBatch: (n: number) => () => Promise<void>,
  records: BenchRecord[],
  runtime: string,
  generatedAtUnixMs: number,
): Promise<void> => {
  printHeader(label);

  for (const n of BATCH_SIZES) {
    const warmup = warmupIters(n);
    const samples: number[] = [];
    const runBatch = makeRunBatch(n);

    for (let i = 0; i < ITERATIONS + warmup; i++) {
      const start = nowNs();
      await runBatch();
      const elapsedNs = Number(nowNs() - start);
      if (i >= warmup) samples.push(elapsedNs);
    }

    const stats = summarizeSamples(samples);
    const columnLabel = `n=${n}`;
    printStats(columnLabel, stats);
    pushRecord(
      records,
      runtime,
      generatedAtUnixMs,
      label,
      "batch",
      n,
      columnLabel,
      warmup,
      stats,
    );
  }
};

export const echoString = task<string, string>({
  f: (value) => value,
});

export const echoBytes = task<Uint8Array, Uint8Array>({
  f: (value) => value,
});

export const echoF64 = task<number, number>({
  f: (value) => value,
});

let sink = 0;

if (isMain) {
  const options = parseCliOptions();
  const runtime = runtimeName();
  const generatedAtUnixMs = Date.now();
  const records: BenchRecord[] = [];
  const pool = createPool({
    threads: 1,
    payload: {
      payloadInitialBytes: PAYLOAD_INITIAL_BYTES,
      payloadMaxByteLength: PAYLOAD_MAX_BYTES,
    },
  })({ echoString, echoBytes, echoF64 });

  const stringPayloads = [
    "x".repeat(PAYLOAD_BYTES),
    "y".repeat(PAYLOAD_BYTES),
    "z".repeat(PAYLOAD_BYTES),
    "w".repeat(PAYLOAD_BYTES),
  ];

  const bytePayloads = makeBytePayloads(PAYLOAD_BYTES);

  console.log(`runtime: ${runtime}`);
  console.log("task: send payload -> worker echo -> return, join_all");
  console.log(
    `(whole-batch latency; warmup n=1: ${WARMUP_N1}, others: ${WARMUP})`,
  );
  console.log("(string/bytes use 4 payload variants rotated with index % 4)");

  try {
    await runBench(
      "knitting number f64 (8 bytes)",
      (n) => {
        return async () => {
          const jobs = Array.from({ length: n }, () => pool.call.echoF64(42));
          const values = await Promise.all(jobs);
          for (const value of values) sink ^= value | 0;
        };
      },
      records,
      runtime,
      generatedAtUnixMs,
    );

    await runBench(
      `knitting large string (${PAYLOAD_BYTES} bytes)`,
      (n) => {
        let turn = 0;

        return async () => {
          const jobs = new Array<Promise<string>>(n);
          for (let j = 0; j < n; j++) {
            const index = (turn + j) % stringPayloads.length;
            jobs[j] = pool.call.echoString(stringPayloads[index]!);
          }
          const values = await Promise.all(jobs);
          for (const value of values) sink ^= value.length;
          turn++;
        };
      },
      records,
      runtime,
      generatedAtUnixMs,
    );

    await runBench(
      `knitting Uint8Array (${PAYLOAD_BYTES} bytes)`,
      (n) =>
        makeEchoBytesBatch(
          n,
          bytePayloads,
          (value) => pool.call.echoBytes(value),
        ),
      records,
      runtime,
      generatedAtUnixMs,
    );

    printHeader(
      `knitting Uint8Array size sweep (batch=${UINT8ARRAY_SIZE_SWEEP_BATCH}, ${fmtBinaryBytes(UINT8ARRAY_SIZE_SWEEP_MIN_BYTES)} -> ${fmtBinaryBytes(UINT8ARRAY_SIZE_SWEEP_MAX_BYTES)})`,
      "size",
    );

    const sizeSweepWarmup = warmupIters(UINT8ARRAY_SIZE_SWEEP_BATCH);
    for (const bytes of uint8ArraySizeSweepBytes) {
      const samples: number[] = [];
      const runBatch = makeEchoBytesBatch(
        UINT8ARRAY_SIZE_SWEEP_BATCH,
        makeBytePayloads(bytes),
        (value) => pool.call.echoBytes(value),
      );

      for (let i = 0; i < ITERATIONS + sizeSweepWarmup; i++) {
        const start = nowNs();
        await runBatch();
        const elapsedNs = Number(nowNs() - start);
        if (i >= sizeSweepWarmup) samples.push(elapsedNs);
      }

      const stats = summarizeSamples(samples);
      const columnLabel = fmtBinaryBytes(bytes);
      printStats(columnLabel, stats);
      pushRecord(
        records,
        runtime,
        generatedAtUnixMs,
        `knitting Uint8Array size sweep (batch=${UINT8ARRAY_SIZE_SWEEP_BATCH})`,
        "size_bytes",
        bytes,
        columnLabel,
        sizeSweepWarmup,
        stats,
      );
    }
  } finally {
    await pool.shutdown();
  }

  if (options.csv) {
    await writeCsvReport(records);
  }

  if (sink === Number.MIN_SAFE_INTEGER) {
    console.log("unreachable", sink);
  }
}
