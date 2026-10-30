import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readFile, unlink, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, relative, basename } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { zstdCompressSync } from 'node:zlib';
import { setTimeout as delay } from 'node:timers/promises';
import { createStores, providers, fabricatedBody } from './fixture-store.mjs';

const root = resolve(process.env.PI_TRACE_VERIFY_PACKAGE);
const output = resolve(process.env.PI_TRACE_VERIFY_ARTIFACTS ?? '.audit/system-session/package');
const origin = 'http://127.0.0.1:4317';
await mkdir(output, { recursive: true });
const directory = await mkdtemp(resolve(tmpdir(), 'pi-trace extracted verification '));
const caller = resolve(directory, 'unrelated caller cwd');
const nodeOnly = resolve(directory, 'node only PATH');
await Promise.all([mkdir(caller), mkdir(nodeOnly)]);
await symlink(process.execPath, resolve(nodeOnly, 'node'));
const env = { ...process.env, PATH: nodeOnly };
const stores = await createStores(resolve(directory, 'synthetic roots with spaces'));
const cliLog = [];
const http = [];
const children = new Set();
let launcher, otherLauncher, service;
let result = { verdict: 'not_verified', packageRoot: root, caller, nodeOnlyPath: nodeOnly };
function start(args) {
  const child = spawn(process.execPath, [resolve(root, 'scripts/start.mjs'), ...args], { cwd: caller, env, stdio: ['ignore', 'pipe', 'pipe'] });
  child.output = '';
  child.closed = new Promise((done) => child.once('exit', (code, signal) => { children.delete(child); done({ code, signal }); }));
  child.stdout.on('data', (bytes) => { child.output += bytes; cliLog.push(bytes.toString()); });
  child.stderr.on('data', (bytes) => { child.output += bytes; cliLog.push(bytes.toString()); });
  children.add(child);
  return child;
}
async function until(predicate, message, timeout = 15000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await predicate()) return; await delay(50); }
  throw new Error(message);
}
async function json(path, expected = 200) {
  const response = await fetch(`${origin}${path}`, { signal: AbortSignal.timeout(3000) });
  const text = await response.text();
  http.push({ at: Date.now(), path, status: response.status });
  assert.equal(response.status, expected, path);
  assert.ok(!text.includes(fabricatedBody));
  return JSON.parse(text);
}
async function overview(item, expected) {
  let ws;
  await until(async () => { ws = await json(`/api/v1/sessions/${item.id}/workspace`); return ws.state === 'ready'; }, 'packaged analysis did not become ready');
  const view = await json(`/api/v1/sessions/${item.id}/snapshots/${ws.snapshotId}/overview`);
  assert.deepEqual(view.tokenAccounting.totalTokens, { state: 'exact', value: String(expected) });
  assert.equal(view.source.provider, item.provider);
  return { ws, view };
}
async function stopChild(child) {
  if (child.exitCode === null) child.kill('SIGTERM');
  const stopped = await Promise.race([child.closed, delay(5000, null, { ref: false })]);
  if (!stopped) { child.kill('SIGKILL'); throw new Error('Packaged CLI did not stop its owned worker'); }
  assert.equal(stopped.code, 0, child.output);
  await until(async () => { try { await fetch(`${origin}/api/v1/health`, { signal: AbortSignal.timeout(250) }); return false; } catch { return true; } }, 'owned native service remains after CLI stopped', 2000);
  return stopped;
}
try {
  for (const provider of providers) {
    await stores.createSession(provider, 'active');
    await stores.appendUsage(provider, 'active');
  }
  const archived = await stores.createSession('codex', 'archive');
  await stores.appendUsage('codex', 'archive');
  const archivePath = resolve(stores.roots.codex, 'archived_sessions', `${basename(archived.source)}.zst`);
  await mkdir(resolve(stores.roots.codex, 'archived_sessions'), { recursive: true });
  await writeFile(archivePath, zstdCompressSync(await readFile(archived.source)));
  await unlink(archived.source);
  archived.source = archivePath;
  const roots = providers.map((provider) => `${provider}=${relative(caller, stores.roots[provider])}`);
  service = start(['system', ...roots.flatMap((value) => ['--root', value])]);
  await until(async () => {
    if (service.exitCode !== null) throw new Error(service.output);
    if (!service.output.includes(`Open ${origin}`)) return false;
    return (await json('/api/v1/health')).status === 'ok';
  }, 'packaged CLI failed to start');
  await until(async () => (await json('/api/v1/sessions')).items.length === 5, 'package did not discover five sessions');
  const items = (await json('/api/v1/sessions')).items;
  const observed = [];
  for (const provider of providers) {
    const item = items.find((entry) => entry.provider === provider && (provider !== 'codex' || entry.nativeId === archived.id));
    assert.ok(item);
    const { ws, view } = await overview(item, 130);
    if (provider === 'codex') assert.equal(view.source.kind, 'compressedJsonl');
    if (provider === 'opencode') assert.equal(view.source.kind, 'sqlite');
    observed.push({ provider, source: view.source.kind, sessionId: item.id, snapshotId: ws.snapshotId, total: '130' });
  }
  const before = await stores.fingerprint();
  const beforeSchema = await readFile(stores.database);
  await delay(1200);
  await json('/api/v1/system');
  assert.deepEqual(await stores.fingerprint(), before, 'extracted native reads leave static files, WAL, SHM and permissions unchanged');
  assert.deepEqual(await readFile(stores.database), beforeSchema);
  const conflict = start(['system', '--root', 'pi=some other root', '--disable-provider', 'codex', '--disable-provider', 'claude', '--disable-provider', 'opencode']);
  const conflicted = await Promise.race([conflict.closed, delay(5000, null, { ref: false })]);
  assert.ok(conflicted && conflicted.code !== 0, 'another CLI cannot attach to the owned service');
  assert.match(conflict.output, /Address already in use|already in use|os error 98/);
  assert.equal((await json('/api/v1/health')).status, 'ok', 'conflict does not stop the original service');
  const fresh = await stores.createSession('pi', 'new-session');
  await stores.appendUsage('pi', 'new-session');
  let newItem;
  await until(async () => { newItem = (await json('/api/v1/sessions')).items.find((entry) => entry.nativeId === fresh.id); return Boolean(newItem); }, 'package new-session discovery', 5000);
  await delay(2200);
  await overview(newItem, 130);
  await stores.appendUsage('pi', 'new-session');
  await until(async () => {
    const ws = await json(`/api/v1/sessions/${newItem.id}/workspace`);
    const view = await json(`/api/v1/sessions/${newItem.id}/snapshots/${ws.snapshotId}/overview`);
    return view.tokenAccounting.totalTokens.value === '260';
  }, 'packaged new session did not update', 5000);
  const cliShutdown = await stopChild(service);
  const { createLauncher } = await import(pathToFileURL(resolve(root, 'scripts/lib/launcher.mjs')).href);
  launcher = createLauncher({ root, env });
  const options = { mode: 'system', cwd: caller, roots };
  const first = await launcher.start(options);
  const alias = resolve(directory, 'provider root alias');
  await symlink(stores.roots.pi, alias);
  const equivalent = [...roots].reverse().map((value) => value.startsWith('pi=') ? `pi=${relative(caller, alias)}` : value);
  const second = await launcher.start({ ...options, roots: equivalent });
  assert.strictEqual(first, second, 'root order and canonical aliases reuse the same owned service');
  await assert.rejects(launcher.start({ ...options, roots: ['pi=different root'], disabledProviders: ['codex', 'claude', 'opencode'] }), /different provider configuration/);
  await assert.rejects(launcher.start({ mode: 'watch', source: caller, cwd: caller }), /different mode/);
  otherLauncher = createLauncher({ root, env, startupTimeout: 3000 });
  await assert.rejects(otherLauncher.start(options), /Address already in use|already in use|os error 98/);
  await otherLauncher.dispose();
  assert.equal((await json('/api/v1/health')).status, 'ok', 'disposal of a failed launcher preserves the other owner');
  await launcher.dispose();
  assert.equal((await first.closed).code, 0);
  const empty = start([]);
  await until(async () => empty.output.includes(`Open ${origin}`), 'no-source packaged startup');
  assert.equal((await json('/api/v1/workspace')).state, 'empty');
  await stopChild(empty);
  const recording = resolve(caller, 'collector recording');
  await mkdir(recording);
  await writeFile(resolve(recording, 'events.jsonl'), '{"type":"session_start","ts":0,"sessionId":"synthetic-package"}\n');
  for (const mode of ['serve', 'watch']) {
    const child = start([mode, 'collector recording']);
    await until(async () => child.output.includes(`Open ${origin}`), `packaged ${mode} startup`);
    assert.equal((await json('/api/v1/workspace')).state, 'ready');
    await stopChild(child);
  }
  const binaryPath = resolve(root, 'native', `${process.platform}-${process.arch}`, 'pi-trace');
  result = { verdict: 'verified', packageRoot: root, caller, nodeOnlyPath: nodeOnly, providers: observed, binarySha256: createHash('sha256').update(await readFile(binaryPath)).digest('hex'), cliShutdown, independentConflictRejected: true, aliasConfigurationReused: true, newSessionUpdate: '260', sourceImmutableDuringStaticRead: true, modes: ['empty', 'serve', 'watch', 'system'], providerOrBuildExecutablesOnPath: false };
} catch (error) {
  result = { ...result, message: error.message, stack: error.stack };
  console.error(error);
  process.exitCode = 1;
} finally {
  await otherLauncher?.dispose();
  await launcher?.dispose();
  for (const child of children) {
    child.kill('SIGTERM');
    const stopped = await Promise.race([child.closed, delay(5000, null, { ref: false })]);
    if (!stopped) { child.kill('SIGKILL'); process.exitCode = 1; result.verdict = 'not_verified'; }
  }
  stores.close();
  await writeFile(resolve(output, 'http.json'), JSON.stringify(http, null, 2) + '\n');
  await writeFile(resolve(output, 'cli-output.log'), cliLog.join(''));
  await writeFile(resolve(output, 'result.json'), JSON.stringify(result, null, 2) + '\n');
  console.log(JSON.stringify({ verdict: result.verdict, output }));
}
