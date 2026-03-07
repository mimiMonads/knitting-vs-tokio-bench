import { spawn } from "node:child_process";
import { existsSync } from "node:fs";

const withCsv = process.argv.includes("--csv");
const reportPython = existsSync(".venv/bin/python") ? ".venv/bin/python" : "python3";

const steps = [
  {
    label: "tokio",
    command: "cargo",
    args: ["run", "--release", "--quiet", ...(withCsv ? ["--", "--csv"] : [])],
  },
  {
    label: "bun",
    command: "bun",
    args: ["run", "src/main.ts", ...(withCsv ? ["--csv"] : [])],
  },
  {
    label: "deno",
    command: "deno",
    args: ["run", "-A", "src/main.ts", ...(withCsv ? ["--csv"] : [])],
  },
  {
    label: "node",
    command: "node",
    args: ["src/main.ts", ...(withCsv ? ["--csv"] : [])],
  },
];

const runStep = (label, command, args) =>
  new Promise((resolve, reject) => {
    console.log(`\n== ${label} ==`);

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

for (const step of steps) {
  await runStep(step.label, step.command, step.args);
}

if (withCsv) {
  await runStep("report", reportPython, ["graphs/report.py"]);
}
