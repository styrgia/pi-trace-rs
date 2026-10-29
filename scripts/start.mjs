#!/usr/bin/env node
import { createLauncher } from "./lib/launcher.mjs";

const launcher = createLauncher({ onOutput: (text) => process.stdout.write(text) });
let interrupted = false;
const shutdown = () => { interrupted = true; void launcher.dispose(); };
process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);

try {
  const app = await launcher.start();
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
