import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdtemp, mkdir, writeFile, readFile, rm, chmod, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import extension from "../extensions/pi-trace.mjs";
import { createLauncher } from "../scripts/lib/launcher.mjs";

const fakeServer = `#!${process.execPath}
import { createServer } from "node:http";
import { writeFileSync } from "node:fs";
writeFileSync("server.pid", String(process.pid));
const port = Number(process.argv[2]);
if (process.argv[3] === "hang") {
  process.on("SIGTERM", () => {});
  setInterval(() => {}, 1000);
} else {
  const server = createServer((_request, response) => {
    response.setHeader("content-type", "application/json");
    response.end(JSON.stringify({ name: "pi-trace", status: "ok", stage: "bootstrap" }));
  });
  server.listen(port, "127.0.0.1", () => {
    process.stdout.write("Pi Trace bootstrap: http://127.0.0.1:" + port + "\\n");
  });
}
`;

async function freePort() {
  const server = createServer();
  await new Promise((resolveListen) => server.listen(0, "127.0.0.1", resolveListen));
  const port = server.address().port;
  await new Promise((resolveClose) => server.close(resolveClose));
  return port;
}

async function fixture(t) {
  const root = await mkdtemp(resolve(tmpdir(), "pi-trace-launcher-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(resolve(root, "web/dist"), { recursive: true });
  await writeFile(resolve(root, "web/dist/index.html"), "<p>Bootstrap</p>");
  await writeFile(resolve(root, "server.mjs"), fakeServer);
  const port = await freePort();
  return { root, port };
}

async function waitForFile(path) {
  for (let attempt = 0; attempt < 100; attempt++) {
    try { return await readFile(path, "utf8"); } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    await delay(10);
  }
  throw new Error(`Timed out waiting for ${path}`);
}

function launcherFor(t, config, mode = "serve") {
  const launcher = createLauncher({
    ...config,
    executable: process.execPath,
    args: [resolve(config.root, "server.mjs"), String(config.port), mode],
    startupTimeout: 2000,
    shutdownTimeout: 50,
  });
  t.after(() => launcher.dispose());
  return launcher;
}

test("extension registration does not build, launch a process, or open a browser", async () => {
  const commands = new Map();
  const events = new Map();
  extension({
    registerCommand: (name, command) => commands.set(name, command),
    on: (name, handler) => events.set(name, handler),
    exec: () => assert.fail("Registration must not execute a process"),
  });
  assert.deepEqual([...commands.keys()], ["pi-trace"]);
  assert.deepEqual([...events.keys()], ["session_shutdown"]);
  const notifications = [];
  await commands.get("pi-trace").handler("unknown", { ui: { notify: (...args) => notifications.push(args) } });
  assert.match(notifications[0][0], /Use \/pi-trace/);
  await events.get("session_shutdown")();
  await events.get("session_shutdown")();
});

test("start requires built assets and never starts a build", async (t) => {
  const { root } = await fixture(t);
  const launcher = createLauncher({ root });
  t.after(() => launcher.dispose());
  await assert.rejects(launcher.start(), /not built.*\/pi-trace build/);
  await assert.rejects(stat(resolve(root, "target")), { code: "ENOENT" });
});

test("concurrent and repeated starts share one owned server, and stop permits a fresh start", async (t) => {
  const config = await fixture(t);
  const launcher = launcherFor(t, config);
  const [first, second] = await Promise.all([launcher.start(), launcher.start()]);
  assert.equal(first, second);
  assert.equal(await launcher.start(), first);
  const pid = Number(await readFile(resolve(config.root, "server.pid"), "utf8"));
  assert.equal((await fetch(`${first.url}/api/v1/health`)).status, 200);
  await launcher.stop();
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
  const next = await launcher.start();
  assert.notEqual(next, first);
  assert.notEqual(Number(await readFile(resolve(config.root, "server.pid"), "utf8")), pid);
});

test("a port collision never attaches to, queries, or kills another server", async (t) => {
  const config = await fixture(t);
  let requests = 0;
  const other = createServer((_request, response) => {
    requests++;
    response.end(JSON.stringify({ name: "pi-trace", status: "ok", stage: "bootstrap" }));
  });
  await new Promise((resolveListen) => other.listen(config.port, "127.0.0.1", resolveListen));
  t.after(() => new Promise((resolveClose) => other.close(resolveClose)));
  const launcher = launcherFor(t, config);
  await assert.rejects(launcher.start(), /exited before becoming ready/);
  await launcher.stop();
  assert.equal(requests, 0);
  assert.equal((await fetch(`http://127.0.0.1:${config.port}`)).status, 200);
  assert.equal(requests, 1);
});

test("shutdown cancels startup and kills an owned child that ignores SIGTERM", async (t) => {
  const config = await fixture(t);
  const launcher = launcherFor(t, config, "hang");
  const pending = launcher.start();
  const rejected = assert.rejects(pending, /cancelled/);
  const pid = Number(await waitForFile(resolve(config.root, "server.pid")));
  await launcher.dispose();
  await rejected;
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
  await assert.rejects(launcher.start(), /shut down/);
});

test("readiness timeout stops the owned process instead of leaving it in the background", async (t) => {
  const config = await fixture(t);
  const launcher = createLauncher({
    ...config,
    executable: process.execPath,
    args: [resolve(config.root, "server.mjs"), String(config.port), "hang"],
    startupTimeout: 200,
    shutdownTimeout: 50,
  });
  t.after(() => launcher.dispose());
  await assert.rejects(launcher.start(), /did not become ready/);
  const pid = Number(await readFile(resolve(config.root, "server.pid"), "utf8"));
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
});

test("building a running server is refused without stopping it", async (t) => {
  const launcher = launcherFor(t, await fixture(t));
  const app = await launcher.start();
  await assert.rejects(launcher.build(), /Stop Pi Trace/);
  assert.equal((await fetch(`${app.url}/api/v1/health`)).status, 200);
});

async function buildTools(t, root, hang = false) {
  const bin = resolve(root, "tools");
  await mkdir(bin);
  for (const name of ["npm", "cargo"]) {
    const source = `#!${process.execPath}
import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
const tool = basename(process.argv[1]);
appendFileSync("commands.jsonl", JSON.stringify({ tool, args: process.argv.slice(2), cwd: process.cwd() }) + "\\n");
if (${hang} && tool === "npm") {
  writeFileSync("build.pid", String(process.pid));
  process.on("SIGTERM", () => {});
  setInterval(() => {}, 1000);
} else if (tool === "cargo") {
  mkdirSync(resolve(process.env.CARGO_TARGET_DIR, "release"), { recursive: true });
  writeFileSync(resolve(process.env.CARGO_TARGET_DIR, "release/pi-trace"), "built Rust binary");
}
`;
    await writeFile(resolve(bin, name), source);
    await chmod(resolve(bin, name), 0o755);
  }
  const previous = process.env.PATH;
  process.env.PATH = `${bin}:${previous}`;
  t.after(() => { process.env.PATH = previous; });
}

test("an explicit source build installs web development tools and bundles the Rust executable", async (t) => {
  const { root } = await fixture(t);
  await writeFile(resolve(root, "web/package-lock.json"), "{}");
  await buildTools(t, root);
  const launcher = createLauncher({ root });
  t.after(() => launcher.dispose());
  await launcher.build();
  const commands = (await readFile(resolve(root, "commands.jsonl"), "utf8")).trim().split("\n").map(JSON.parse);
  assert.deepEqual(commands, [
    { tool: "npm", args: ["--prefix", "web", "ci", "--include=dev"], cwd: root },
    { tool: "npm", args: ["--prefix", "web", "run", "build"], cwd: root },
    { tool: "cargo", args: ["build", "--release", "--locked", "-p", "pi-trace"], cwd: root },
  ]);
  const native = resolve(root, "native", `${process.platform}-${process.arch}`, "pi-trace");
  assert.equal(await readFile(native, "utf8"), "built Rust binary");
  assert.equal((await stat(native)).mode & 0o111, 0o111);
});

test("a packaged build uses bundled web assets without installing web tools", async (t) => {
  const { root } = await fixture(t);
  await buildTools(t, root);
  const launcher = createLauncher({ root });
  t.after(() => launcher.dispose());
  await launcher.build();
  const commands = (await readFile(resolve(root, "commands.jsonl"), "utf8")).trim().split("\n").map(JSON.parse);
  assert.deepEqual(commands.map(({ tool }) => tool), ["cargo"]);
});

test("building fails before running commands when neither web sources nor bundled assets exist", async (t) => {
  const { root } = await fixture(t);
  await rm(resolve(root, "web"), { recursive: true });
  const launcher = createLauncher({ root });
  t.after(() => launcher.dispose());
  await assert.rejects(launcher.build(), /web sources with a lockfile or bundled/);
});

test("shutdown cancels an explicit build and prevents later build steps", async (t) => {
  const { root } = await fixture(t);
  await writeFile(resolve(root, "web/package-lock.json"), "{}");
  await buildTools(t, root, true);
  const launcher = createLauncher({ root, shutdownTimeout: 50 });
  t.after(() => launcher.dispose());
  const rejected = assert.rejects(launcher.build(), /cancelled/);
  const pid = Number(await waitForFile(resolve(root, "build.pid")));
  await launcher.dispose();
  await rejected;
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
  assert.equal((await readFile(resolve(root, "commands.jsonl"), "utf8")).trim().split("\n").length, 1);
});

test("start prefers the bundled executable in the common package", async (t) => {
  const config = await fixture(t);
  const native = resolve(config.root, "native", `${process.platform}-${process.arch}`);
  await mkdir(native, { recursive: true });
  await writeFile(resolve(native, "pi-trace"), fakeServer);
  await chmod(resolve(native, "pi-trace"), 0o755);
  const launcher = createLauncher({ ...config, args: [String(config.port)], startupTimeout: 2000 });
  t.after(() => launcher.dispose());
  const app = await launcher.start();
  assert.equal((await fetch(`${app.url}/api/v1/health`)).status, 200);
});

test("start falls back to a locally compiled executable when no platform binary is bundled", async (t) => {
  const config = await fixture(t);
  const target = resolve(config.root, "target/release");
  await mkdir(target, { recursive: true });
  await writeFile(resolve(target, "pi-trace"), fakeServer);
  await chmod(resolve(target, "pi-trace"), 0o755);
  const launcher = createLauncher({ ...config, args: [String(config.port)], startupTimeout: 2000 });
  t.after(() => launcher.dispose());
  const app = await launcher.start();
  assert.equal((await fetch(`${app.url}/api/v1/health`)).status, 200);
});
