import { createPool, isMain, task } from "@vixeny/knitting";

const BATCH_SIZES = [1, 10, 100] as const;
const ITERATIONS = 500;
const WARMUP = 50;
const WARMUP_N1 = 200;
const PAYLOAD_BYTES = 1024 * 1024;
const PAYLOAD_INITIAL_BYTES = 16 * 1024 * 1024;
const PAYLOAD_MAX_BYTES = 256 * 1024 * 1024;

const warmupIters = (batch: number) => (batch === 1 ? WARMUP_N1 : WARMUP);
const runtimeGlobals = globalThis as typeof globalThis & {
  Bun?: { version: string };
  Deno?: { version: { deno: string } };
  process?: {
    versions?: { node?: string };
    hrtime?: { bigint?: () => bigint };
  };
};

const fmtNs = (ns: number): string => {
  if (ns >= 1_000_000) return `${(ns / 1_000_000).toFixed(2)} ms`;
  if (ns >= 1_000) return `${(ns / 1_000).toFixed(2)} \u00B5s`;
  return `${ns.toFixed(2)} ns`;
};

const printHeader = (label: string): void => {
  console.log(`\n--- ${label} ---`);
  console.log(
    `${"batch".padEnd(8)} ${"avg".padStart(12)} ${"min".padStart(12)} ${"p75".padStart(12)} ${"p99".padStart(12)} ${"max".padStart(12)}`,
  );
  console.log("-".repeat(70));
};

const printStats = (n: number, samples: number[]): void => {
  samples.sort((a, b) => a - b);

  const len = samples.length;
  const avg = samples.reduce((sum, sample) => sum + sample, 0) / len;
  const min = samples[0]!;
  const p75 = samples[Math.floor((len * 75) / 100)]!;
  const p99 = samples[Math.floor((len * 99) / 100)]!;
  const max = samples[len - 1]!;

  console.log(
    `${`n=${n}`.padEnd(8)} ${fmtNs(avg).padStart(12)} ${fmtNs(min).padStart(12)} ${fmtNs(p75).padStart(12)} ${fmtNs(p99).padStart(12)} ${fmtNs(max).padStart(12)}`,
  );
};

const runtimeName = (): string => {
  if (runtimeGlobals.Bun) return `bun ${runtimeGlobals.Bun.version}`;
  if (runtimeGlobals.Deno) return `deno ${runtimeGlobals.Deno.version.deno}`;
  if (runtimeGlobals.process?.versions?.node) {
    return `node ${runtimeGlobals.process.versions.node}`;
  }
  return "unknown";
};

const nowNs = (): bigint => {
  const hrtime = runtimeGlobals.process?.hrtime?.bigint;
  if (hrtime) return hrtime();
  return BigInt(Math.round(globalThis.performance.now() * 1_000_000));
};

const runBench = async (
  label: string,
  makeRunBatch: (n: number) => () => Promise<void>,
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

    printStats(n, samples);
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

  const bytePayloads = [
    new Uint8Array(PAYLOAD_BYTES).fill(0xAB),
    new Uint8Array(PAYLOAD_BYTES).fill(0xBC),
    new Uint8Array(PAYLOAD_BYTES).fill(0xCD),
    new Uint8Array(PAYLOAD_BYTES).fill(0xDE),
  ];

  console.log(`runtime: ${runtimeName()}`);
  console.log("task: send payload -> worker echo -> return, join_all");
  console.log(
    `(whole-batch latency; warmup n=1: ${WARMUP_N1}, others: ${WARMUP})`,
  );
  console.log("(string/bytes use 4 payload variants rotated with index % 4)");

  try {
    await runBench(`knitting large string (${PAYLOAD_BYTES} bytes)`, (n) => {
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
    });

    await runBench("knitting number f64 (8 bytes)", (n) => {
      return async () => {
        const jobs = Array.from({ length: n }, () => pool.call.echoF64(42));
        const values = await Promise.all(jobs);
        for (const value of values) sink ^= value | 0;
      };
    });

    await runBench(`knitting Uint8Array (${PAYLOAD_BYTES} bytes)`, (n) => {
      let turn = 0;

      return async () => {
        const jobs = new Array<Promise<Uint8Array>>(n);
        for (let j = 0; j < n; j++) {
          const index = (turn + j) % bytePayloads.length;
          jobs[j] = pool.call.echoBytes(bytePayloads[index]!);
        }
        const values = await Promise.all(jobs);
        for (const value of values) sink ^= value.byteLength;
        turn++;
      };
    });
  } finally {
    await pool.shutdown();
  }

  if (sink === Number.MIN_SAFE_INTEGER) {
    console.log("unreachable", sink);
  }
}
