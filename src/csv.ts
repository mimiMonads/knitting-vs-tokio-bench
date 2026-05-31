import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { runtimeId } from "./runtime.ts";
import type { BenchRecord } from "./bench.ts";

const csvEscape = (value: string | number): string => {
  const s = String(value);
  return /[",\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s;
};

const serializeCsv = (records: readonly BenchRecord[]): string => {
  const header = [
    "implementation", "runtime", "generated_at_unix_ms", "benchmark",
    "column_kind", "column_value", "column_label", "iterations", "warmup",
    "avg_ns", "min_ns", "p75_ns", "p99_ns", "max_ns",
  ].join(",");

  const lines = records.map((r) =>
    [
      r.implementation, r.runtime, r.generatedAtUnixMs, r.benchmark,
      r.columnKind, r.columnValue, r.columnLabel, r.iterations, r.warmup,
      r.avgNs, r.minNs, r.p75Ns, r.p99Ns, r.maxNs,
    ].map(csvEscape).join(",")
  );

  return `${header}\n${lines.join("\n")}\n`;
};

export const writeCsvReport = async (records: readonly BenchRecord[]): Promise<void> => {
  if (records.length === 0) return;
  await mkdir("results", { recursive: true });
  const outputPath = join("results", `knitting-${runtimeId()}-${records[0]!.generatedAtUnixMs}.csv`);
  await writeFile(outputPath, serializeCsv(records), "utf8");
  console.log(`csv: ${outputPath}`);
};
