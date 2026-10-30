import { spawn } from "node:child_process";
import { access, chmod, copyFile, mkdir } from "node:fs/promises";
import { constants } from "node:fs";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { homedir } from "node:os";
import { resolveSystemConfig } from "./system-config.mjs";

export const packageRoot = fileURLToPath(new URL("../../", import.meta.url));

export function createLauncher({
  root = packageRoot,
  executable,
  args = [],
  port = 4317,
  startupTimeout = 10_000,
  shutdownTimeout = 1500,
  onOutput = () => {},
  env = process.env,
  home = homedir(),
} = {}) {
  const url = `http://127.0.0.1:${port}`;
  let activity;
  let owned;
  let running;
  let stopping;
  let disposed = false;
  let stopRevision = 0;
  const configuring = new Set();

  function spawnOwned(command, commandArgs, onStdout = () => {}) {
    const child = spawn(command, commandArgs, {
      cwd: root,
      stdio: ["ignore", "pipe", "pipe"],
      shell: false,
      detached: process.platform === "linux",
      env: { ...env, CARGO_TARGET_DIR: resolve(root, "target") },
    });
    const handle = { child, output: "", ended: false, error: undefined };
    handle.closed = new Promise((resolveExit) => {
      child.once("error", (error) => { handle.error = error; });
      child.once("close", (code, signal) => {
        handle.ended = true;
        if (owned === handle) owned = undefined;
        if (running?.handle === handle) running = undefined;
        resolveExit({ code, signal, error: handle.error, output: handle.output });
      });
    });
    const collect = (chunk) => {
      handle.output = (handle.output + chunk).slice(-65_536);
      onOutput(chunk.toString());
    };
    child.stdout.on("data", (chunk) => { collect(chunk); onStdout(chunk.toString()); });
    child.stderr.on("data", collect);
    owned = handle;
    return handle;
  }

  async function terminate(handle) {
    if (handle.ended) return;
    function send(signal) {
      if (handle.ended || !handle.child.pid) return;
      try {
        if (process.platform === "linux") process.kill(-handle.child.pid, signal);
        else handle.child.kill(signal);
      } catch (error) {
        if (error.code !== "ESRCH") throw error;
      }
    }
    send("SIGTERM");
    if (await Promise.race([handle.closed.then(() => true), delay(shutdownTimeout, false, { ref: false })])) return;
    send("SIGKILL");
    if (!await Promise.race([handle.closed.then(() => true), delay(shutdownTimeout, false, { ref: false })])) {
      throw new Error("The owned Pi Trace process did not exit after SIGKILL.");
    }
  }

  function checkCancelled(signal) {
    if (signal.aborted) throw new Error("Pi Trace operation cancelled.");
  }

  async function exclusive(kind, operation, source = null, mode = "offline", configuration = null) {
    if (disposed) throw new Error("The Pi Trace launcher has shut down.");
    if (stopping) await stopping;
    if (disposed) throw new Error("The Pi Trace launcher has shut down.");
    if (kind === "build" && configuring.size > 0) throw new Error("Pi Trace start is in progress.");
    if (activity) {
      if (activity.kind === kind) {
        if (kind === "start" && activity.mode !== mode) throw modeConflict();
        if (kind === "start" && activity.source !== source) throw sourceConflict();
        if (kind === "start" && activity.configuration !== configuration) throw configurationConflict();
        return activity.promise;
      }
      throw new Error(`Pi Trace ${activity.kind} is in progress.`);
    }
    const current = { kind, source, mode, configuration, controller: new AbortController() };
    activity = current;
    current.promise = operation(current.controller.signal).finally(() => {
      if (activity === current) activity = undefined;
    });
    return current.promise;
  }

  async function build() {
    return exclusive("build", async (signal) => {
      if (running) throw new Error("Stop Pi Trace before building it.");
      let hasWebSource = true;
      try {
        await access(resolve(root, "web/package-lock.json"));
      } catch (error) {
        if (error.code !== "ENOENT") throw error;
        hasWebSource = false;
        try {
          await access(resolve(root, "web/dist/index.html"));
        } catch {
          throw new Error("Pi Trace needs web sources with a lockfile or bundled web/dist assets to build.");
        }
      }
      const commands = hasWebSource ? [
        ["npm", ["--prefix", "web", "ci", "--include=dev"]],
        ["npm", ["--prefix", "web", "run", "build"]],
      ] : [];
      commands.push(["cargo", ["build", "--release", "--locked", "-p", "pi-trace"]]);
      for (const [command, commandArgs] of commands) {
        checkCancelled(signal);
        const result = await spawnOwned(command, commandArgs).closed;
        checkCancelled(signal);
        if (result.error) throw new Error(`Cannot run ${command}: ${result.error.message}`);
        if (result.code !== 0) {
          throw new Error(`${command} ${commandArgs.join(" ")} failed (${result.signal ?? result.code}).\n${result.output}`);
        }
      }
      checkCancelled(signal);
      const name = process.platform === "win32" ? "pi-trace.exe" : "pi-trace";
      const directory = resolve(root, "native", `${process.platform}-${process.arch}`);
      await mkdir(directory, { recursive: true });
      await copyFile(resolve(root, "target/release", name), resolve(directory, name));
      await chmod(resolve(directory, name), 0o755);
    });
  }

  function sourceConflict() {
    return new Error("Pi Trace is already starting or running with a different source. Stop Pi Trace before selecting another recording.");
  }

  function modeConflict() {
    return new Error("Pi Trace is already starting or running in a different mode. Stop Pi Trace before switching between offline, watch, and system.");
  }

  function configurationConflict() {
    return new Error("Pi Trace is already starting or running with a different provider configuration. Stop Pi Trace before changing system roots or disabled providers.");
  }

  async function start({ source: selectedSource, cwd = process.cwd(), mode = "offline", roots, disabledProviders } = {}) {
    if (disposed) throw new Error("The Pi Trace launcher has shut down.");
    if (!["offline", "watch", "system"].includes(mode)) throw new Error("Pi Trace mode must be offline, watch, or system.");
    if (mode === "system" && selectedSource != null) throw new Error("System mode accepts provider roots, not a recording directory.");
    if (mode !== "system" && (roots !== undefined || disabledProviders !== undefined)) throw new Error("Provider roots and disabled providers require system mode.");
    if (mode === "watch" && selectedSource == null) throw new Error("Pi Trace needs a recording directory after watch.");
    if (selectedSource != null && (typeof selectedSource !== "string" || selectedSource.length === 0)) {
      throw new Error("Pi Trace needs a recording directory after serve or watch.");
    }
    const source = selectedSource == null ? null : resolve(cwd, selectedSource);
    let system = null;
    if (mode === "system") {
      const revision = stopRevision;
      const pending = resolveSystemConfig({ roots, disabledProviders, cwd, env, home });
      configuring.add(pending);
      try {
        system = await pending;
      } finally {
        configuring.delete(pending);
      }
      if (disposed) throw new Error("The Pi Trace launcher has shut down.");
      if (revision !== stopRevision) throw new Error("Pi Trace operation cancelled.");
    }
    const configuration = system?.identity ?? null;
    return exclusive("start", async (signal) => {
      if (running) {
        if (running.mode !== mode) throw modeConflict();
        if (running.source !== source) throw sourceConflict();
        if (running.configuration !== configuration) throw configurationConflict();
        return running.app;
      }
      let command = executable;
      try {
        if (!command) {
          const name = process.platform === "win32" ? "pi-trace.exe" : "pi-trace";
          const bundled = resolve(root, "native", `${process.platform}-${process.arch}`, name);
          try {
            await access(bundled, constants.X_OK);
            command = bundled;
          } catch (error) {
            if (error.code !== "ENOENT" && error.code !== "EACCES") throw error;
            command = resolve(root, "target/release", name);
          }
        }
        await access(command, constants.X_OK);
        await access(resolve(root, "web/dist/index.html"));
      } catch {
        throw new Error("Pi Trace is not built. Run /pi-trace build in Pi or npm run build in the package directory.");
      }
      checkCancelled(signal);
      let stdout = "";
      const commandArgs = system ? [...args, ...system.args] : source === null ? args : [...args, mode === "watch" ? "watch" : "serve", source];
      const handle = spawnOwned(command, commandArgs, (chunk) => { stdout = (stdout + chunk).slice(-4096); });
      const deadline = Date.now() + startupTimeout;
      try {
        while (Date.now() < deadline) {
          checkCancelled(signal);
          if (handle.ended) {
            const result = await handle.closed;
            throw new Error(`Pi Trace exited before becoming ready (${result.signal ?? result.code}).\n${result.error?.message ?? result.output}`);
          }
          if (stdout.includes(`Pi Trace bootstrap: ${url}`)) {
            try {
              const response = await fetch(`${url}/api/v1/health`, {
                signal: AbortSignal.any([signal, AbortSignal.timeout(500)]),
                redirect: "error",
              });
              const health = await response.json();
              if (response.ok && health?.name === "pi-trace" && health.status === "ok" && health.stage === "bootstrap") {
                checkCancelled(signal);
                if (handle.ended) continue;
                const app = { url, closed: handle.closed };
                running = { handle, app, source, mode, configuration };
                return app;
              }
            } catch {
              checkCancelled(signal);
            }
          }
          await delay(25);
        }
        throw new Error(`Pi Trace did not become ready at ${url} within ${startupTimeout}ms.`);
      } catch (error) {
        await terminate(handle);
        throw error;
      }
    }, source, mode, configuration);
  }

  function stop() {
    if (stopping) return stopping;
    stopRevision++;
    const pendingConfiguration = [...configuring];
    const pending = activity;
    pending?.controller.abort();
    stopping = (async () => {
      if (owned) await terminate(owned);
      await Promise.allSettled(pendingConfiguration);
      await pending?.promise.catch(() => {});
    })().finally(() => { stopping = undefined; });
    return stopping;
  }

  async function dispose() {
    disposed = true;
    await stop();
  }

  return { build, start, stop, dispose };
}
