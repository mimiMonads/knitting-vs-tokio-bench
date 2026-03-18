import { createPool, isMain, task } from "@vixeny/knitting";

const TARGET_DURATION_MS = 1_500;
const F64_BATCH = 256;
const BYTE_BATCH = 32;
const PAYLOAD_BYTES = 256 * 1024;
const BYTE_FILL_VALUES = [0xAB, 0xBC, 0xCD, 0xDE] as const;

const runtimeGlobals = globalThis as typeof globalThis & {
  Bun?: { version: string };
  Deno?: { version: { deno: string } };
  process?: { versions?: { node?: string } };
};

const runtimeName = (): string => {
  if (runtimeGlobals.Bun) return `bun ${runtimeGlobals.Bun.version}`;
  if (runtimeGlobals.Deno) return `deno ${runtimeGlobals.Deno.version.deno}`;
  if (runtimeGlobals.process?.versions?.node) {
    return `node ${runtimeGlobals.process.versions.node}`;
  }
  return "unknown";
};

const makeBytePayloads = (bytes: number): Uint8Array[] =>
  BYTE_FILL_VALUES.map((fillValue) => new Uint8Array(bytes).fill(fillValue));

export const echoBytes = task<Uint8Array, Uint8Array>({
  f: (value) => value,
});

export const echoF64 = task<number, number>({
  f: (value) => value,
});

let sink = 0;

if (isMain) {
  const payloads = makeBytePayloads(PAYLOAD_BYTES);
  const pool = createPool({
    threads: 1,
    payload: {
    },
  })({ echoBytes, echoF64 });

  console.log(`resource benchmark: ${runtimeName()}`);
  console.log(
    `target_duration_ms=${TARGET_DURATION_MS} f64_batch=${F64_BATCH} bytes_batch=${BYTE_BATCH} payload_bytes=${PAYLOAD_BYTES}`,
  );

  let completedBatches = 0;
  let f64Messages = 0;
  let byteMessages = 0;
  let turn = 0;
  const startedAt = performance.now();

  try {
    while (performance.now() - startedAt < TARGET_DURATION_MS) {
      const numberJobs = new Array<Promise<number>>(F64_BATCH);
      for (let j = 0; j < F64_BATCH; j++) {
        numberJobs[j] = pool.call.echoF64(42 + ((turn + j) & 255));
      }

      const byteJobs = new Array<Promise<Uint8Array>>(BYTE_BATCH);
      for (let j = 0; j < BYTE_BATCH; j++) {
        const index = (turn + j) % payloads.length;
        byteJobs[j] = pool.call.echoBytes(payloads[index]!);
      }

      const [numbers, bytes] = await Promise.all([
        Promise.all(numberJobs),
        Promise.all(byteJobs),
      ]);

      for (const value of numbers) sink ^= value | 0;
      for (const value of bytes) sink ^= value.byteLength;

      completedBatches++;
      f64Messages += F64_BATCH;
      byteMessages += BYTE_BATCH;
      turn++;
    }
  } finally {
    await pool.shutdown();
  }

  console.log(`completed_batches=${completedBatches}`);
  console.log(`f64_messages=${f64Messages}`);
  console.log(`byte_messages=${byteMessages}`);
  console.log(`payload_bytes=${PAYLOAD_BYTES}`);
  console.log(`sink=${sink}`);
}
