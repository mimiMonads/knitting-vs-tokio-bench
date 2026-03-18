import { spawn } from "node:child_process";
import { mkdir, open, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const DEFAULT_RUNS = 1;
const TIME_FORMAT = [
  "elapsed_seconds=%e",
  "cpu_percent=%P",
  "max_rss_kb=%M",
  "voluntary_context_switches=%w",
].join("\\n");

const parseCliOptions = () => {
  const args = process.argv.slice(2);
  const runsIndex = args.indexOf("--runs");

  if (runsIndex === -1) {
    return { runs: DEFAULT_RUNS };
  }

  const rawValue = args[runsIndex + 1];
  const runs = Number.parseInt(rawValue ?? "", 10);
  if (!Number.isInteger(runs) || runs < 1) {
    throw new Error(`invalid --runs value: ${rawValue ?? "missing"}`);
  }

  return { runs };
};

const formatCpu = (percent) => `${percent.toFixed(1)}%`;
const formatRss = (kb) => `${(kb / 1024).toFixed(1)} MiB`;

const parseKeyValueFile = (text) => {
  const parsed = {};
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    const [key, value] = line.split("=", 2);
    if (!key || value === undefined) continue;
    parsed[key.trim()] = value.trim();
  }
  return parsed;
};

const parseMeasuredRun = async (timePath) => {
  const values = parseKeyValueFile(await readFile(timePath, "utf8"));
  const elapsedSeconds = Number(values.elapsed_seconds);
  const cpuPercent = Number((values.cpu_percent ?? "").replace("%", ""));
  const maxRssKb = Number(values.max_rss_kb);
  const voluntaryContextSwitches = Number(values.voluntary_context_switches);

  if (
    Number.isNaN(elapsedSeconds) ||
    Number.isNaN(cpuPercent) ||
    Number.isNaN(maxRssKb) ||
    Number.isNaN(voluntaryContextSwitches)
  ) {
    throw new Error(`failed to parse timing output: ${timePath}`);
  }

  return {
    elapsedSeconds,
    cpuPercent,
    maxRssKb,
    voluntaryContextSwitches,
  };
};

const summarizeValues = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  const median =
    sorted.length % 2 === 0
      ? (sorted[middle - 1] + sorted[middle]) / 2
      : sorted[middle];

  return {
    min: sorted[0],
    median,
    avg: values.reduce((sum, value) => sum + value, 0) / values.length,
    max: sorted[sorted.length - 1],
  };
};

const runCommand = async (label, command, args) => {
  await new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      stdio: "inherit",
    });

    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (code === 0) {
        resolve();
        return;
      }

      reject(
        new Error(
          signal
            ? `${label} terminated with signal ${signal}`
            : `${label} exited with code ${code ?? "unknown"}`,
        ),
      );
    });
  });
};

const steps = [
  {
    runtime: "tokio",
    command: join("target", "release", "resource_quick"),
    args: [],
  },
  {
    runtime: "bun",
    command: "bun",
    args: ["run", "src/resource_quick.ts"],
  },
  {
    runtime: "deno",
    command: "deno",
    args: ["run", "-A", "src/resource_quick.ts"],
  },
  {
    runtime: "node",
    command: "node",
    args: ["src/resource_quick.ts"],
  },
];

const runMeasuredStep = async (step, run, generatedAtUnixMs, outputDir) => {
  const timePath = join(outputDir, `${generatedAtUnixMs}-${step.runtime}-run${run}.time.txt`);
  const logPath = join(outputDir, `${generatedAtUnixMs}-${step.runtime}-run${run}.log.txt`);
  const logHandle = await open(logPath, "w");

  console.log(`\n== ${step.runtime} run ${run} ==`);

  try {
    await new Promise((resolve, reject) => {
      const child = spawn(
        "/usr/bin/time",
        ["-f", TIME_FORMAT, "-o", timePath, step.command, ...step.args],
        {
          stdio: ["ignore", logHandle.fd, logHandle.fd],
        },
      );

      child.on("error", reject);
      child.on("exit", (code, signal) => {
        if (code === 0) {
          resolve();
          return;
        }

        reject(
          new Error(
            signal
              ? `${step.runtime} terminated with signal ${signal}`
              : `${step.runtime} exited with code ${code ?? "unknown"}`,
          ),
        );
      });
    });
  } finally {
    await logHandle.close();
  }

  const measured = await parseMeasuredRun(timePath);
  console.log(
    `elapsed=${measured.elapsedSeconds.toFixed(2)}s cpu=${formatCpu(measured.cpuPercent)} rss=${formatRss(measured.maxRssKb)} voluntary_cs=${measured.voluntaryContextSwitches}`,
  );

  return {
    runtime: step.runtime,
    run,
    command: [step.command, ...step.args].join(" "),
    ...measured,
    timePath,
    logPath,
  };
};

const main = async () => {
  const options = parseCliOptions();
  const generatedAtUnixMs = Date.now();
  const outputDir = join("results", "resources");
  const results = [];

  await mkdir(outputDir, { recursive: true });
  console.log("preparing tokio release binary");
  await runCommand("tokio build", "cargo", [
    "build",
    "--release",
    "--quiet",
    "--bin",
    "resource_quick",
  ]);

  for (const step of steps) {
    for (let run = 1; run <= options.runs; run++) {
      results.push(await runMeasuredStep(step, run, generatedAtUnixMs, outputDir));
    }
  }

  const summary = steps.map((step) => {
    const runtimeResults = results.filter((result) => result.runtime === step.runtime);
    return {
      runtime: step.runtime,
      runs: runtimeResults.length,
      elapsedSeconds: summarizeValues(runtimeResults.map((result) => result.elapsedSeconds)),
      cpuPercent: summarizeValues(runtimeResults.map((result) => result.cpuPercent)),
      maxRssKb: summarizeValues(runtimeResults.map((result) => result.maxRssKb)),
      voluntaryContextSwitches: summarizeValues(
        runtimeResults.map((result) => result.voluntaryContextSwitches),
      ),
    };
  });

  const summaryPath = join(outputDir, `${generatedAtUnixMs}-summary.json`);
  await writeFile(
    summaryPath,
    JSON.stringify(
      {
        benchmark: "resource_quick",
        generatedAtUnixMs,
        runs: options.runs,
        results,
        summary,
      },
      null,
      2,
    ),
    "utf8",
  );

  console.log("\nsummary");
  for (const item of summary) {
    console.log(
      `${item.runtime.padEnd(6)} elapsed=${item.elapsedSeconds.avg.toFixed(2)}s cpu=${formatCpu(item.cpuPercent.avg)} rss=${formatRss(item.maxRssKb.avg)} voluntary_cs=${item.voluntaryContextSwitches.avg.toFixed(0)}`,
    );
  }
  console.log(`json: ${summaryPath}`);
};

await main();
