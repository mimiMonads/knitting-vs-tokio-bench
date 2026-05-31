export const runtimeGlobals = globalThis as typeof globalThis & {
  Bun?: { version: string };
  Deno?: { args: string[]; version: { deno: string } };
  process?: {
    argv?: string[];
    versions?: { node?: string };
    hrtime?: { bigint?: () => bigint };
  };
};

export const runtimeName = (): string => {
  if (runtimeGlobals.Bun) return `bun ${runtimeGlobals.Bun.version}`;
  if (runtimeGlobals.Deno) return `deno ${runtimeGlobals.Deno.version.deno}`;
  if (runtimeGlobals.process?.versions?.node) return `node ${runtimeGlobals.process.versions.node}`;
  return "unknown";
};

export const runtimeId = (): string => {
  if (runtimeGlobals.Bun) return "bun";
  if (runtimeGlobals.Deno) return "deno";
  if (runtimeGlobals.process?.versions?.node) return "node";
  return "unknown";
};

export const nowNs = (): bigint => {
  const hrtime = runtimeGlobals.process?.hrtime?.bigint;
  if (hrtime) return hrtime();
  return BigInt(Math.round(globalThis.performance.now() * 1_000_000));
};

export const cliArgs = (): string[] => {
  if (runtimeGlobals.Deno) return runtimeGlobals.Deno.args;
  return runtimeGlobals.process?.argv?.slice(2) ?? [];
};
