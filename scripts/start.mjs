#!/usr/bin/env node
import { resolve } from "node:path";
import { createLauncher, packageRoot } from "./lib/launcher.mjs";

const launcher = createLauncher({ onOutput: (text) => process.stdout.write(text) });
let interrupted = false;
const shutdown = () => { interrupted = true; void launcher.dispose(); };
process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);

try {
  const args = process.argv.slice(2);
  let options;
  const usage = "Use pi-trace-rs or pi-trace-rs serve <directory>, pi-trace-rs watch <directory>, or pi-trace-rs system [--root <provider>=<path>] [--disable-provider <provider>]. Providers: codex, pi, claude, opencode.";
  if (args[0] === "system") {
    options = { mode: "system", roots: [], disabledProviders: [] };
    for (let index = 1; index < args.length; index += 2) {
      const option = args[index];
      const value = args[index + 1];
      if (!["--root", "--disable-provider"].includes(option) || !value || value.startsWith("--")) throw new Error(usage);
      options[option === "--root" ? "roots" : "disabledProviders"].push(value);
    }
  } else {
    if (args.length !== 0 && (args.length !== 2 || !["serve", "watch"].includes(args[0]) || args[1].length === 0)) throw new Error(usage);
    options = { source: args[1], ...(args[0] === "watch" ? { mode: "watch" } : {}) };
  }
  const invocationCwd = process.cwd();
  const isPackageStart = process.env.npm_lifecycle_event === "start"
    && process.env.npm_package_json !== undefined
    && resolve(process.env.npm_package_json) === resolve(packageRoot, "package.json")
    && resolve(invocationCwd) === resolve(packageRoot);
  const cwd = isPackageStart ? process.env.INIT_CWD ?? invocationCwd : invocationCwd;
  const app = await launcher.start({ ...options, cwd });
  process.stdout.write(`Open ${app.url}. Stop with Ctrl+C.\n`);
  const result = await app.closed;
  if (!interrupted && result.code !== 0) {
    process.stderr.write(`Pi Trace exited (${result.signal ?? result.code}).\n`);
    process.exitCode = 1;
  }
} catch (error) {
  if (!interrupted) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
} finally {
  await launcher.dispose();
}
