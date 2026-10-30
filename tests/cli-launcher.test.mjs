import test from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdtemp, mkdir, copyFile, writeFile, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";

const run = promisify(execFile);

async function cli(t) {
  const root = await mkdtemp(resolve(tmpdir(), "pi-trace-cli-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(resolve(root, "package/scripts/lib"), { recursive: true });
  await mkdir(resolve(root, "caller workspace"));
  await writeFile(resolve(root, "package/package.json"), JSON.stringify({ scripts: { start: "node scripts/start.mjs" } }));
  const entry = resolve(root, "package/scripts/start.mjs");
  const log = resolve(root, "calls.jsonl");
  await copyFile(new URL("../scripts/start.mjs", import.meta.url), entry);
  await writeFile(resolve(root, "package/scripts/lib/launcher.mjs"), `
import { appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
export const packageRoot = fileURLToPath(new URL("../../", import.meta.url));
function record(call) { appendFileSync(process.env.PI_TRACE_TEST_LOG, JSON.stringify(call) + "\\n"); }
export function createLauncher() {
  return {
    async start(options) {
      record(["start", options]);
      return { url: "http://127.0.0.1:4317", closed: Promise.resolve({ code: 0 }) };
    },
    async dispose() { record(["dispose"]); },
  };
}
`);
  const cwd = resolve(root, "caller workspace");
  const env = { ...process.env, PI_TRACE_TEST_LOG: log };
  delete env.INIT_CWD;
  return {
    cwd,
    invoke: (args, inheritedEnv = {}, invocationCwd = cwd) => run(process.execPath, [entry, ...args], { cwd: invocationCwd, env: { ...env, ...inheritedEnv } }),
    npmInvoke: (args) => run("npm", ["--prefix", resolve(root, "package"), "start", "--", ...args], { cwd, env }),
    npmWrapperInvoke: async (args) => {
      const wrapper = resolve(root, "wrapper");
      await mkdir(wrapper);
      await writeFile(resolve(wrapper, "package.json"), JSON.stringify({ scripts: { start: "node wrapper.mjs" } }));
      await writeFile(resolve(wrapper, "wrapper.mjs"), `
import { spawnSync } from "node:child_process";
process.chdir(${JSON.stringify(cwd)});
const result = spawnSync(process.execPath, [${JSON.stringify(entry)}, ...process.argv.slice(2)], { stdio: "inherit" });
process.exitCode = result.status ?? 1;
`);
      return run("npm", ["start", "--", ...args], { cwd: wrapper, env });
    },
    packageRoot: resolve(root, "package"),
    calls: async () => (await readFile(log, "utf8")).trim().split("\n").map(JSON.parse),
  };
}

test("the installed CLI supplies the invocation cwd and one complete source argument", async (t) => {
  const fixture = await cli(t);
  const source = "recordings/trace with spaces;$(no-shell)";
  const result = await fixture.invoke(["serve", source]);
  assert.match(result.stdout, /Open http:\/\/127\.0\.0\.1:4317/);
  assert.deepEqual(await fixture.calls(), [["start", { source, cwd: fixture.cwd }], ["dispose"]]);
});

test("the CLI preserves no-source startup", async (t) => {
  const fixture = await cli(t);
  await fixture.invoke([]);
  assert.deepEqual(await fixture.calls(), [["start", { cwd: fixture.cwd }], ["dispose"]]);
});

test("npm start resolves source paths from the caller even when npm runs in its package directory", async (t) => {
  const fixture = await cli(t);
  const source = "recordings/trace with spaces";
  await fixture.npmInvoke(["serve", source]);
  assert.deepEqual(await fixture.calls(), [["start", { source, cwd: fixture.cwd }], ["dispose"]]);
});

test("a direct CLI call ignores inherited INIT_CWD", async (t) => {
  const fixture = await cli(t);
  const source = "recordings/direct call";
  await fixture.invoke(["serve", source], { INIT_CWD: resolve(fixture.cwd, "old caller") });
  assert.deepEqual(await fixture.calls(), [["start", { source, cwd: fixture.cwd }], ["dispose"]]);
});

test("inherited package start metadata cannot override a changed invocation cwd", async (t) => {
  const fixture = await cli(t);
  const source = "recordings/changed cwd";
  await fixture.invoke(["serve", source], {
    INIT_CWD: resolve(fixture.cwd, "old caller"),
    npm_lifecycle_event: "start",
    npm_package_json: resolve(fixture.packageRoot, "package.json"),
  });
  assert.deepEqual(await fixture.calls(), [["start", { source, cwd: fixture.cwd }], ["dispose"]]);
});

test("an npm wrapper that changes cwd resolves the source from that cwd", async (t) => {
  const fixture = await cli(t);
  const source = "recordings/wrapped call";
  await fixture.npmWrapperInvoke(["serve", source]);
  assert.deepEqual(await fixture.calls(), [["start", { source, cwd: fixture.cwd }], ["dispose"]]);
});

test("only this package's npm start may use INIT_CWD from its package directory", async (t) => {
  for (const [event, ownManifest] of [["test", true], ["start", false]]) {
    const fixture = await cli(t);
    const source = "recordings/package cwd";
    await fixture.invoke(["serve", source], {
      INIT_CWD: fixture.cwd,
      npm_lifecycle_event: event,
      npm_package_json: resolve(ownManifest ? fixture.packageRoot : fixture.cwd, "package.json"),
    }, fixture.packageRoot);
    assert.deepEqual(await fixture.calls(), [["start", { source, cwd: fixture.packageRoot }], ["dispose"]]);
  }
});

test("invalid CLI syntax fails without starting a service", async (t) => {
  for (const args of [["serve"], ["serve", ""], ["other"], ["serve", "first", "second"], ["watch"], ["watch", ""], ["watch", "a", "b"]]) {
    const fixture = await cli(t);
    await assert.rejects(fixture.invoke(args), (error) => {
      assert.equal(error.code, 1);
      assert.match(error.stderr, /Use pi-trace-rs or pi-trace-rs serve <directory>/);
      return true;
    });
    assert.deepEqual(await fixture.calls(), [["dispose"]]);
  }
});

test("watch CLI supplies mode and preserves the complete path and invocation cwd", async (t) => {
  const fixture = await cli(t);
  const source = "recordings/live trace with spaces;$(no-shell)";
  await fixture.invoke(["watch", source]);
  assert.deepEqual(await fixture.calls(), [["start", { source, cwd: fixture.cwd, mode: "watch" }], ["dispose"]]);
});

test("system CLI forwards repeated roots and disabled providers without shell parsing", async (t) => {
  const fixture = await cli(t);
  const roots = ["pi=first store", "pi=second store;$(no-shell)", "codex=path=with=equals"];
  await fixture.invoke(["system", "--root", roots[0], "--disable-provider", "claude", "--root", roots[1], "--root", roots[2], "--disable-provider", "opencode"]);
  assert.deepEqual(await fixture.calls(), [["start", {
    mode: "system", roots, disabledProviders: ["claude", "opencode"], cwd: fixture.cwd,
  }], ["dispose"]]);
});

test("bare system CLI and npm start preserve caller cwd and inherited settings", async (t) => {
  const direct = await cli(t);
  await direct.invoke(["system"]);
  assert.deepEqual(await direct.calls(), [["start", { mode: "system", roots: [], disabledProviders: [], cwd: direct.cwd }], ["dispose"]]);
  const npm = await cli(t);
  await npm.npmInvoke(["system", "--root", "codex=relative with spaces"]);
  assert.deepEqual(await npm.calls(), [["start", { mode: "system", roots: ["codex=relative with spaces"], disabledProviders: [], cwd: npm.cwd }], ["dispose"]]);
});

test("invalid system CLI options explain syntax without starting a service", async (t) => {
  for (const args of [
    ["system", "path"], ["system", "--root"], ["system", "--root", ""],
    ["system", "--disable-provider"], ["system", "--other", "codex"],
    ["system", "--root", "--disable-provider", "pi"],
    ["system", "--root", "codex=a", "--disable-provider"],
  ]) {
    const fixture = await cli(t);
    await assert.rejects(fixture.invoke(args), (error) => {
      assert.equal(error.code, 1);
      assert.match(error.stderr, /pi-trace-rs system.*--root.*--disable-provider/);
      return true;
    });
    assert.deepEqual(await fixture.calls(), [["dispose"]]);
  }
});
