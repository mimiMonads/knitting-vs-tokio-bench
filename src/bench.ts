import { nowNs } from "./runtime.ts";
import { printHeader, printStats } from "./format.ts";

export const BATCH_SIZES = [1, 10, 100] as const;
export const ITERATIONS = 500;
export const WARMUP = 50;
export const WARMUP_N1 = 200;

export type ColumnKind = "batch" | "size_bytes";

export type BenchStats = {
  avgNs: number;
  minNs: number;
  p75Ns: number;
  p99Ns: number;
  maxNs: number;
};

export type BenchRecord = {
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

export const warmupIters = (batch: number): number =>
  batch === 1 ? WARMUP_N1 : WARMUP;

export const summarizeSamples = (samples: number[]): BenchStats => {
  samples.sort((a, b) => a - b);
  const len = samples.length;
  return {
    avgNs: samples.reduce((sum, s) => sum + s, 0) / len,
    minNs: samples[0]!,
    p75Ns: samples[Math.floor((len * 75) / 100)]!,
    p99Ns: samples[Math.floor((len * 99) / 100)]!,
    maxNs: samples[len - 1]!,
  };
};

export const pushRecord = (
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
    ...stats,
  });
};

export const runBench = async (
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
    pushRecord(records, runtime, generatedAtUnixMs, label, "batch", n, columnLabel, warmup, stats);
  }
};
