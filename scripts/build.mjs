import { createLauncher } from "./lib/launcher.mjs";

const launcher = createLauncher({ onOutput: (text) => process.stdout.write(text) });
let interrupted = false;
const shutdown = () => { interrupted = true; void launcher.dispose(); };
process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);

try {
  await launcher.build();
} catch (error) {
  if (!interrupted) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
} finally {
  await launcher.dispose();
}
