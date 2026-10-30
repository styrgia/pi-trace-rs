import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdtemp, mkdir, writeFile, readFile, rm, chmod, stat, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import extension from "../extensions/pi-trace.mjs";
import { createLauncher } from "../scripts/lib/launcher.mjs";

const fakeServer = `#!${process.execPath}
import { createServer } from "node:http";
import { writeFileSync } from "node:fs";
writeFileSync("server.pid", String(process.pid));
writeFileSync("invocation.json", JSON.stringify({ args: process.argv.slice(2), cwd: process.cwd() }));
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

function launcherFor(t, config, mode = "normal") {
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

test("source paths resolve against invocation cwd and remain one shell-free argument", async (t) => {
  const config = await fixture(t);
  const launcher = launcherFor(t, config);
  const cwd = resolve(config.root, "caller workspace");
  const source = "recordings/trace with spaces;$(no-shell)";
  await mkdir(cwd);
  const app = await launcher.start({ source, cwd });
  const invocation = JSON.parse(await readFile(resolve(config.root, "invocation.json"), "utf8"));
  assert.deepEqual(invocation, {
    args: [String(config.port), "normal", "serve", resolve(cwd, source)],
    cwd: config.root,
  });
  assert.equal(await launcher.start({ source: resolve(cwd, source) }), app);
  await assert.rejects(launcher.start(), /different source.*Stop Pi Trace/);
  await assert.rejects(launcher.start({ source: "another recording", cwd }), /different source.*Stop Pi Trace/);
  assert.equal((await fetch(`${app.url}/api/v1/health`)).status, 200);
  await launcher.stop();
  const empty = await launcher.start();
  assert.notEqual(empty, app);
  assert.deepEqual(JSON.parse(await readFile(resolve(config.root, "invocation.json"), "utf8")).args, [String(config.port), "normal"]);
  await assert.rejects(launcher.start({ source, cwd }), /different source.*Stop Pi Trace/);
});

test("in-flight starts share only the same resolved source and refuse a different source", async (t) => {
  const config = await fixture(t);
  const launcher = launcherFor(t, config);
  const first = launcher.start({ source: "trace with spaces", cwd: config.root });
  const same = launcher.start({ source: resolve(config.root, "trace with spaces") });
  await assert.rejects(launcher.start({ source: "another trace", cwd: config.root }), /different source.*Stop Pi Trace/);
  await assert.rejects(launcher.start(), /different source.*Stop Pi Trace/);
  assert.equal(await first, await same);
});

test("invalid source values are refused before spawning", async (t) => {
  const config = await fixture(t);
  const launcher = launcherFor(t, config);
  for (const source of ["", 3, {}]) {
    await assert.rejects(launcher.start({ source }), /recording directory/);
  }
  await assert.rejects(stat(resolve(config.root, "server.pid")), { code: "ENOENT" });
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

test("watch processes reuse only the same source and mode and forward one shell-free path", async (t) => {
  const config = await fixture(t);
  const launcher = launcherFor(t, config);
  const source = "live traces/with spaces;$(no-shell)";
  const first = launcher.start({ source, cwd: config.root, mode: "watch" });
  const same = launcher.start({ source: resolve(config.root, source), mode: "watch" });
  await assert.rejects(launcher.start({ source, cwd: config.root }), /different mode/);
  assert.equal(await first, await same);
  const app = await first;
  const pid = Number(await readFile(resolve(config.root, "server.pid"), "utf8"));
  assert.deepEqual(JSON.parse(await readFile(resolve(config.root, "invocation.json"), "utf8")).args, [String(config.port), "normal", "watch", resolve(config.root, source)]);
  await assert.rejects(launcher.start({ source, cwd: config.root, mode: "offline" }), /different mode/);
  await assert.rejects(launcher.start({ source: "another", cwd: config.root, mode: "watch" }), /different source/);
  assert.equal(Number(await readFile(resolve(config.root, "server.pid"), "utf8")), pid);
  assert.equal((await fetch(`${app.url}/api/v1/health`)).status, 200);
  await launcher.stop();
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
});

test("invalid watch mode or missing source is refused before spawning", async (t) => {
  const config = await fixture(t);
  const launcher = launcherFor(t, config);
  for (const options of [{ mode: "watch" }, { source: "a", mode: "unknown" }]) {
    await assert.rejects(launcher.start(options), /mode|recording directory/);
  }
  await assert.rejects(stat(resolve(config.root, "server.pid")), { code: "ENOENT" });
});

test("an owned offline process refuses watch without stopping or replacing it", async (t) => {
  const config = await fixture(t);
  const launcher = launcherFor(t, config);
  const source = "same trace";
  const app = await launcher.start({ source, cwd: config.root });
  const pid = Number(await readFile(resolve(config.root, "server.pid"), "utf8"));
  await assert.rejects(launcher.start({ source, cwd: config.root, mode: "watch" }), /different mode/);
  assert.equal(await launcher.start({ source, cwd: config.root }), app);
  assert.equal(Number(await readFile(resolve(config.root, "server.pid"), "utf8")), pid);
});

test("watch startup cancellation stops only the owned child", async (t) => {
  const config = await fixture(t);
  const launcher = launcherFor(t, config, "hang");
  const starting = launcher.start({ source: "trace with spaces", cwd: config.root, mode: "watch" });
  const rejection = assert.rejects(starting, /cancelled|exited before becoming ready/);
  const pid = Number(await waitForFile(resolve(config.root, "server.pid")));
  await launcher.dispose();
  await rejection;
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
});

async function systemFixture(t, env = {}, mode = "normal") {
  const config = await fixture(t);
  const home = resolve(config.root, "synthetic home");
  const cwd = resolve(config.root, "caller workspace");
  await mkdir(home);
  await mkdir(cwd);
  const launcher = launcherFor(t, { ...config, env, home }, mode);
  return {
    ...config, home, cwd, launcher,
    invocation: async () => JSON.parse(await readFile(resolve(config.root, "invocation.json"), "utf8")),
  };
}

test("explicit system mode forwards all default stores from an injected synthetic home", async (t) => {
  const config = await systemFixture(t);
  const app = await config.launcher.start({ mode: "system", cwd: config.cwd });
  assert.deepEqual(await config.invocation(), {
    cwd: config.root,
    args: [String(config.port), "normal", "system",
      "--root", `claude=${config.home}/.claude/projects`,
      "--root", `codex=${config.home}/.codex`,
      "--root", `opencode=${config.home}/.local/share/opencode`,
      "--root", `pi=${config.home}/.pi/agent/sessions`],
  });
  assert.equal((await fetch(`${app.url}/api/v1/health`)).status, 200);
  await assert.rejects(config.launcher.start(), /different mode/);
  assert.equal(await config.launcher.start({ mode: "system", cwd: config.cwd }), app);
});

test("inherited relative settings are frozen against the caller cwd before native startup", async (t) => {
  const env = {
    CODEX_HOME: "custom codex",
    PI_CODING_AGENT_DIR: "custom pi",
    CLAUDE_CONFIG_DIR: "custom claude",
    XDG_DATA_HOME: "custom data",
    OPENCODE_DB: "opencode channel.db",
    PI_SESSION_DIR: "ignored unsupported setting",
  };
  const config = await systemFixture(t, env);
  const app = await config.launcher.start({ mode: "system", cwd: config.cwd });
  assert.deepEqual((await config.invocation()).args, [String(config.port), "normal", "system",
    "--root", `claude=${config.cwd}/custom claude/projects`,
    "--root", `codex=${config.cwd}/custom codex`,
    "--root", `opencode=${config.cwd}/custom data/opencode/opencode channel.db`,
    "--root", `pi=${config.cwd}/custom pi/sessions`]);
  assert.equal(await config.launcher.start({ mode: "system", roots: [
    `pi=${config.cwd}/custom pi/sessions`, `opencode=${config.cwd}/custom data/opencode/opencode channel.db`,
    `codex=${config.cwd}/custom codex`, `claude=${config.cwd}/custom claude/projects`,
  ] }), app);
  env.CODEX_HOME = "different codex";
  await assert.rejects(config.launcher.start({ mode: "system", cwd: config.cwd }), /different provider configuration/);
  assert.equal((await fetch(`${app.url}/api/v1/health`)).status, 200);
});

test("Pi's custom session directory overrides its agent directory without appending sessions", async (t) => {
  const config = await systemFixture(t, { PI_CODING_AGENT_DIR: "unused agent", PI_CODING_AGENT_SESSION_DIR: "selected sessions" });
  await config.launcher.start({ mode: "system", cwd: config.cwd, disabledProviders: ["codex", "claude", "opencode"] });
  assert.deepEqual((await config.invocation()).args, [String(config.port), "normal", "system",
    "--disable-provider", "claude", "--disable-provider", "codex", "--disable-provider", "opencode",
    "--root", `pi=${config.cwd}/selected sessions`]);
});

test("system aliases and reordered multistore roots reuse one process and override defaults", async (t) => {
  const config = await systemFixture(t, { CODEX_HOME: "unused", OPENCODE_DB: ":memory:" });
  const store = resolve(config.cwd, "store with spaces;$(no-shell)");
  const alias = resolve(config.cwd, "store alias");
  await mkdir(store);
  await symlink(store, alias, "dir");
  const options = {
    mode: "system", cwd: config.cwd,
    roots: ["codex=store alias", "codex=store with spaces;$(no-shell)", "pi=second store", "pi=first store"],
    disabledProviders: ["opencode", "claude", "opencode"],
  };
  const first = config.launcher.start(options);
  const same = config.launcher.start({ mode: "system", roots: [
    `pi=${config.cwd}/first store`, `codex=${store}`, `pi=${config.cwd}/second store`,
  ], disabledProviders: ["claude", "opencode"] });
  const app = await first;
  assert.equal(await same, app);
  assert.deepEqual((await config.invocation()).args, [String(config.port), "normal", "system",
    "--disable-provider", "claude", "--root", `codex=${store}`, "--disable-provider", "opencode",
    "--root", `pi=${config.cwd}/first store`, "--root", `pi=${config.cwd}/second store`]);
  const pid = Number(await readFile(resolve(config.root, "server.pid"), "utf8"));
  await assert.rejects(config.launcher.start({ ...options, roots: ["codex=other store", "pi=first store"] }), /different provider configuration/);
  await assert.rejects(config.launcher.start({ ...options, disabledProviders: ["claude"], roots: ["codex=store alias", "opencode=selected.db", "pi=first store"] }), /different provider configuration/);
  await assert.rejects(config.launcher.start({ mode: "watch", source: "trace", cwd: config.cwd }), /different mode/);
  assert.equal(Number(await readFile(resolve(config.root, "server.pid"), "utf8")), pid);
  assert.equal((await fetch(`${app.url}/api/v1/health`)).status, 200);
  await config.launcher.dispose();
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
});

test("a missing root below a symlinked ancestor shares its canonical identity", async (t) => {
  const config = await systemFixture(t);
  await symlink(config.cwd, resolve(config.root, "caller alias"), "dir");
  const options = { mode: "system", disabledProviders: ["claude", "pi", "opencode"] };
  const app = await config.launcher.start({ ...options, roots: ["codex=missing store"], cwd: resolve(config.root, "caller alias") });
  assert.equal(await config.launcher.start({ ...options, roots: [`codex=${config.cwd}/missing store`] }), app);
  assert.ok((await config.invocation()).args.includes(`codex=${config.cwd}/missing store`));
});

test("OpenCode's absolute selected file excludes its parent directory and explicit roots win", async (t) => {
  const config = await systemFixture(t);
  const database = resolve(config.cwd, "exact selected.db");
  const env = { XDG_DATA_HOME: "unused data", OPENCODE_DB: database };
  const launcher = launcherFor(t, { ...config, env });
  await launcher.start({ mode: "system", cwd: config.cwd, disabledProviders: ["claude", "codex", "pi"] });
  assert.deepEqual((await config.invocation()).args, [String(config.port), "normal", "system",
    "--disable-provider", "claude", "--disable-provider", "codex", "--root", `opencode=${database}`, "--disable-provider", "pi"]);
  await launcher.stop();
  env.OPENCODE_DB = ":memory:";
  await launcher.start({ mode: "system", cwd: config.cwd, roots: ["opencode=override directory"], disabledProviders: ["claude", "codex", "pi"] });
  assert.ok((await config.invocation()).args.includes(`opencode=${config.cwd}/override directory`));
});

test("invalid system roots and conflicting provider options fail before spawning", async (t) => {
  const config = await systemFixture(t, { OPENCODE_DB: ":memory:" });
  const base = { mode: "system", cwd: config.cwd };
  for (const options of [
    { roots: ["unknown=a"] }, { disabledProviders: ["unknown"] },
    { roots: ["codex="] }, { roots: ["codex"] }, { roots: ["Codex=a"] },
    { roots: ["pi=a", "pi=b"], disabledProviders: ["pi"] },
    { roots: ["opencode=:memory:"] }, {},
    { roots: "codex=a" }, { disabledProviders: null }, { source: "recording" },
  ]) {
    await assert.rejects(config.launcher.start({ ...base, ...options }), /provider|--root|memory|arrays|recording directory/i);
  }
  await assert.rejects(config.launcher.start({ roots: ["codex=a"] }), /require system mode/);
  await assert.rejects(stat(resolve(config.root, "server.pid")), { code: "ENOENT" });
});

test("relative OpenCode database selection stays inside the data directory including aliases", async (t) => {
  const env = { XDG_DATA_HOME: "data", OPENCODE_DB: "../outside.db" };
  const config = await systemFixture(t, env);
  const directory = resolve(config.cwd, "data/opencode");
  const outside = resolve(config.cwd, "outside");
  await mkdir(directory, { recursive: true });
  await mkdir(outside);
  await symlink(outside, resolve(directory, "alias"), "dir");
  const options = { mode: "system", cwd: config.cwd, disabledProviders: ["pi", "codex", "claude"] };
  await assert.rejects(config.launcher.start(options), /Relative OPENCODE_DB.*inside/);
  env.OPENCODE_DB = "alias/selected.db";
  await assert.rejects(config.launcher.start(options), /Relative OPENCODE_DB.*inside/);
  await assert.rejects(stat(resolve(config.root, "server.pid")), { code: "ENOENT" });
  env.OPENCODE_DB = "nested/selected.db";
  await config.launcher.start(options);
  assert.ok((await config.invocation()).args.includes(`opencode=${directory}/nested/selected.db`));
  await config.launcher.stop();
  env.OPENCODE_DB = resolve(outside, "selected.db");
  await config.launcher.start(options);
  assert.ok((await config.invocation()).args.includes(`opencode=${outside}/selected.db`));
});

test("system startup conflicts and cancellation leave only the owned child stopped", async (t) => {
  const config = await systemFixture(t, {}, "hang");
  const options = { mode: "system", roots: ["codex=one"], cwd: config.cwd };
  const starting = config.launcher.start(options);
  const rejection = assert.rejects(starting, /cancelled|exited before becoming ready/);
  const pid = Number(await waitForFile(resolve(config.root, "server.pid")));
  await assert.rejects(config.launcher.start({ ...options, roots: ["codex=two"] }), /different provider configuration/);
  await assert.rejects(config.launcher.start(), /different mode/);
  process.kill(pid, 0);
  await config.launcher.dispose();
  await rejection;
  assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
  await assert.rejects(config.launcher.start(options), /shut down/);
});

test("immediate disposal during system configuration prevents startup", async (t) => {
  const config = await systemFixture(t);
  const rejected = assert.rejects(config.launcher.start({ mode: "system", cwd: config.cwd }), /shut down/);
  await config.launcher.dispose();
  await rejected;
  await assert.rejects(stat(resolve(config.root, "server.pid")), { code: "ENOENT" });
});

test("stop cancels system configuration before spawning and permits a later start", async (t) => {
  const config = await systemFixture(t);
  const options = { mode: "system", cwd: config.cwd };
  const rejected = assert.rejects(config.launcher.start(options), /cancelled/);
  await config.launcher.stop();
  await rejected;
  await assert.rejects(stat(resolve(config.root, "server.pid")), { code: "ENOENT" });
  const app = await config.launcher.start(options);
  assert.equal((await fetch(`${app.url}/api/v1/health`)).status, 200);
});

test("system root normalization reserves startup against an overlapping build", async (t) => {
  const config = await systemFixture(t);
  const starting = config.launcher.start({ mode: "system", cwd: config.cwd });
  await assert.rejects(config.launcher.build(), /start is in progress/);
  const app = await starting;
  assert.equal((await fetch(`${app.url}/api/v1/health`)).status, 200);
});

test("system port collision does not query, reuse, or stop an unrelated service", async (t) => {
  const config = await systemFixture(t);
  let requests = 0;
  const other = createServer((_request, response) => {
    requests++;
    response.end(JSON.stringify({ name: "pi-trace", status: "ok", stage: "bootstrap" }));
  });
  await new Promise((ready) => other.listen(config.port, "127.0.0.1", ready));
  t.after(() => new Promise((closed) => other.close(closed)));
  await assert.rejects(config.launcher.start({ mode: "system", cwd: config.cwd }), /exited before becoming ready/);
  await config.launcher.dispose();
  assert.equal(requests, 0);
  assert.equal((await fetch(`http://127.0.0.1:${config.port}`)).status, 200);
  assert.equal(requests, 1);
});
