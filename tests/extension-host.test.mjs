import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, copyFile, writeFile, readFile, stat, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { setTimeout as delay } from "node:timers/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

async function host(t, { live = false, hang = false } = {}) {
  const root = await mkdtemp(resolve(tmpdir(), "pi-trace-extension-host-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(resolve(root, "extensions"));
  await mkdir(resolve(root, "scripts/lib"), { recursive: true });
  await copyFile(new URL("../extensions/pi-trace.mjs", import.meta.url), resolve(root, "extensions/pi-trace.mjs"));
  const launcherPath = resolve(root, "scripts/lib/launcher.mjs");
  await writeFile(launcherPath, `
export const calls = [];
export const state = { failure: null, instances: 0 };
export function createLauncher() {
  state.instances++;
  return {
    async build() { calls.push(["build"]); },
    async start(options) {
      calls.push(["start", options]);
      if (state.failure) throw new Error(state.failure);
      return { url: "http://127.0.0.1:4317" };
    },
    async stop() { calls.push(["stop"]); },
    async dispose() { calls.push(["dispose"]); },
  };
}
`);
  if (live) {
    const reservation = createServer();
    await new Promise((ready) => reservation.listen(0, "127.0.0.1", ready));
    const port = reservation.address().port;
    await new Promise((closed) => reservation.close(closed));
    await mkdir(resolve(root, "web/dist"), { recursive: true });
    await writeFile(resolve(root, "web/dist/index.html"), "<p>Synthetic app</p>");
    await writeFile(resolve(root, "server.mjs"), `
import { createServer } from "node:http";
import { writeFileSync } from "node:fs";
writeFileSync("server.pid", String(process.pid));
writeFileSync("invocation.json", JSON.stringify(process.argv.slice(2)));
if (${hang}) {
  process.on("SIGTERM", () => {});
  setInterval(() => {}, 1000);
} else {
  createServer((_request, response) => {
    response.setHeader("content-type", "application/json");
    response.end(JSON.stringify({ name: "pi-trace", status: "ok", stage: "bootstrap" }));
  }).listen(${port}, "127.0.0.1", () => process.stdout.write("Pi Trace bootstrap: http://127.0.0.1:${port}\\n"));
}
`);
    await writeFile(launcherPath, `
import { createLauncher as actualLauncher } from ${JSON.stringify(new URL("../scripts/lib/launcher.mjs", import.meta.url).href)};
export function createLauncher() {
  return actualLauncher({
    root: ${JSON.stringify(root)}, home: ${JSON.stringify(resolve(root, "synthetic home"))}, env: {},
    executable: ${JSON.stringify(process.execPath)}, args: [${JSON.stringify(resolve(root, "server.mjs"))}],
    port: ${port}, startupTimeout: 2000, shutdownTimeout: 50,
  });
}
`);
  }
  const { calls, state } = await import(pathToFileURL(launcherPath));
  const { default: extension } = await import(pathToFileURL(resolve(root, "extensions/pi-trace.mjs")));
  const commands = new Map();
  const events = new Map();
  const browser = [];
  const notifications = [];
  extension({
    registerCommand: (name, command) => commands.set(name, command),
    on: (name, handler) => events.set(name, handler),
    exec: async (...args) => { browser.push(args); return { code: 0 }; },
  });
  const ctx = { cwd: resolve(root, "Pi session workspace"), ui: { notify: (...args) => notifications.push(args) } };
  if (live) t.after(() => events.get("session_shutdown")());
  return { root, calls, state, commands, events, browser, notifications, ctx, run: (args) => commands.get("pi-trace").handler(args, ctx) };
}

test("extension discovery is passive and registers only its command and shutdown handler", async (t) => {
  const fixture = await host(t);
  assert.deepEqual([...fixture.commands.keys()], ["pi-trace"]);
  assert.deepEqual([...fixture.events.keys()], ["session_shutdown"]);
  assert.deepEqual(fixture.calls, []);
  assert.deepEqual(fixture.browser, []);
  assert.equal(fixture.state.instances, 1);
  await fixture.run("unknown");
  await fixture.run("serve");
  assert.equal(fixture.notifications.length, 2);
  assert.ok(fixture.notifications.every(([message]) => message.includes("/pi-trace serve <directory>")));
  assert.deepEqual(fixture.calls, []);
});

test("the Pi command passes the complete serve remainder and session cwd to its launcher", async (t) => {
  const fixture = await host(t);
  await fixture.run("  serve recordings/trace with spaces;$(no-shell)");
  assert.deepEqual(fixture.calls, [["start", {
    source: "recordings/trace with spaces;$(no-shell)",
    cwd: fixture.ctx.cwd,
  }]]);
  assert.match(fixture.notifications[0][0], /http:\/\/127\.0\.0\.1:4317/);
  if (process.platform === "linux" && process.env.PI_TRACE_NO_OPEN !== "1") {
    assert.deepEqual(fixture.browser, [["xdg-open", ["http://127.0.0.1:4317"], { timeout: 5000, maxBuffer: 16_384 }]]);
  } else {
    assert.deepEqual(fixture.browser, []);
  }
});

for (const source of [" leading space", "trailing space ", "interior space", " leading and trailing "]) {
  test(`the Pi command preserves the path ${JSON.stringify(source)}`, async (t) => {
    const fixture = await host(t);
    await fixture.run(`serve ${source}`);
    assert.deepEqual(fixture.calls, [["start", { source, cwd: fixture.ctx.cwd }]]);
  });
}

test("build, empty start, stop and session shutdown use the same launcher instance", async (t) => {
  const fixture = await host(t);
  await fixture.run("  build  ");
  assert.deepEqual(fixture.calls, [["build"]]);
  assert.deepEqual(fixture.browser, []);
  await fixture.run("  ");
  await fixture.run("  stop  ");
  await fixture.events.get("session_shutdown")();
  await fixture.events.get("session_shutdown")();
  assert.deepEqual(fixture.calls, [
    ["build"],
    ["start", { source: undefined, cwd: fixture.ctx.cwd }],
    ["stop"],
    ["dispose"],
    ["dispose"],
  ]);
  assert.ok(fixture.notifications.some(([message]) => message.includes("owned by this session has stopped")));
  assert.equal(fixture.state.instances, 1);
});

test("a launcher refusal is visible and does not open a browser", async (t) => {
  const fixture = await host(t);
  fixture.state.failure = "Stop Pi Trace before selecting another recording.";
  await fixture.run("serve other trace");
  assert.deepEqual(fixture.notifications, [[fixture.state.failure, "error"]]);
  assert.deepEqual(fixture.browser, []);
});

for (const source of ["live trace with spaces;$(no-shell)", " leading space", "trailing space "]) {
  test(`watch Pi command preserves full path ${JSON.stringify(source)} and stays passive`, async (t) => {
    const fixture = await host(t);
    assert.deepEqual(fixture.calls, []);
    await fixture.run(`watch ${source}`);
    assert.deepEqual(fixture.calls, [["start", { source, cwd: fixture.ctx.cwd, mode: "watch" }]]);
    await fixture.events.get("session_shutdown")();
    assert.deepEqual(fixture.calls.at(-1), ["dispose"]);
    assert.deepEqual([...fixture.events.keys()], ["session_shutdown"]);
  });
}

test("bare Pi system inherits configuration through the existing session launcher", async (t) => {
  const fixture = await host(t);
  assert.deepEqual(fixture.calls, []);
  await fixture.run("  system  ");
  assert.deepEqual(fixture.calls, [["start", { mode: "system", cwd: fixture.ctx.cwd }]]);
  assert.match(fixture.notifications[0][0], /http:\/\/127\.0\.0\.1:4317/);
  await fixture.events.get("session_shutdown")();
  assert.deepEqual(fixture.calls.at(-1), ["dispose"]);
  assert.deepEqual([...fixture.events.keys()], ["session_shutdown"]);
  assert.equal(fixture.state.instances, 1);
});

test("Pi system rejects arguments with terminal CLI guidance and never opens a browser", async (t) => {
  const fixture = await host(t);
  for (const command of ["system --root codex=a path", "system --disable-provider pi", " system arbitrary path ", "system\n--root pi=x"]) {
    await fixture.run(command);
    assert.match(fixture.notifications.at(-1)[0], /without arguments.*pi-trace-rs system --root/);
  }
  assert.deepEqual(fixture.calls, []);
  assert.deepEqual(fixture.browser, []);
});

test("Pi system startup conflicts stay visible and do not open a browser", async (t) => {
  const fixture = await host(t);
  fixture.state.failure = "Pi Trace is already running in a different mode. Stop Pi Trace before switching.";
  await fixture.run("system");
  assert.deepEqual(fixture.notifications, [[fixture.state.failure, "error"]]);
  assert.deepEqual(fixture.browser, []);
  await fixture.events.get("session_shutdown")();
  assert.deepEqual(fixture.calls, [["start", { mode: "system", cwd: fixture.ctx.cwd }], ["dispose"]]);
});

test("Pi shutdown disposes a real owned system child while mode conflicts preserve it", async (t) => {
  const fixture = await host(t, { live: true });
  await assert.rejects(stat(resolve(fixture.root, "server.pid")), { code: "ENOENT" });
  await fixture.run("system");
  const pid = Number(await readFile(resolve(fixture.root, "server.pid"), "utf8"));
  const args = JSON.parse(await readFile(resolve(fixture.root, "invocation.json"), "utf8"));
  assert.equal(args[0], "system");
  assert.ok(args.includes(`codex=${fixture.root}/synthetic home/.codex`));
  const browserOpens = fixture.browser.length;
  await fixture.run("serve external trace");
  assert.match(fixture.notifications.at(-1)[0], /different mode/);
  assert.equal(fixture.notifications.at(-1)[1], "error");
  assert.equal(fixture.browser.length, browserOpens);
  process.kill(pid, 0);
  await fixture.run("system");
  assert.equal(Number(await readFile(resolve(fixture.root, "server.pid"), "utf8")), pid);
  await fixture.events.get("session_shutdown")();
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
});

test("Pi shutdown cancels a real starting system child without stranded workers", async (t) => {
  const fixture = await host(t, { live: true, hang: true });
  const starting = fixture.run("system");
  let pid;
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      pid = Number(await readFile(resolve(fixture.root, "server.pid"), "utf8"));
      break;
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    await delay(10);
  }
  assert.ok(pid, "the synthetic child reached startup");
  await fixture.events.get("session_shutdown")();
  await starting;
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
  assert.match(fixture.notifications.at(-1)[0], /cancelled|exited before becoming ready/);
  assert.deepEqual(fixture.browser, []);
});
