export const LABEL_COLUMN_WIDTH = 10;

export const fmtNs = (ns: number): string => {
  if (ns >= 1_000_000) return `${(ns / 1_000_000).toFixed(2)} ms`;
  if (ns >= 1_000) return `${(ns / 1_000).toFixed(2)} µs`;
  return `${ns.toFixed(2)} ns`;
};

export const fmtBinaryBytes = (bytes: number): string => {
  if (bytes >= 1024 * 1024) return `${bytes / (1024 * 1024)} MiB`;
  if (bytes >= 1024) return `${bytes / 1024} KiB`;
  return `${bytes} B`;
};

export const printHeader = (label: string, columnLabel = "batch"): void => {
  console.log(`\n--- ${label} ---`);
  console.log(
    `${columnLabel.padEnd(LABEL_COLUMN_WIDTH)} ${"avg".padStart(12)} ${"min".padStart(12)} ${"p75".padStart(12)} ${"p99".padStart(12)} ${"max".padStart(12)}`,
  );
  console.log("-".repeat(70));
};

export const printStats = (label: string, stats: { avgNs: number; minNs: number; p75Ns: number; p99Ns: number; maxNs: number }): void => {
  console.log(
    `${label.padEnd(LABEL_COLUMN_WIDTH)} ${fmtNs(stats.avgNs).padStart(12)} ${fmtNs(stats.minNs).padStart(12)} ${fmtNs(stats.p75Ns).padStart(12)} ${fmtNs(stats.p99Ns).padStart(12)} ${fmtNs(stats.maxNs).padStart(12)}`,
  );
};
