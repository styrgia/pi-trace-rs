import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { createStores, providers } from './fixture-store.mjs';

const root = resolve(process.env.PI_TRACE_VERIFY_PACKAGE ?? process.cwd());
const binary = resolve(process.env.PI_TRACE_VERIFY_BINARY ?? resolve(root, 'target/release/pi-trace'));
const output = resolve(process.env.PI_TRACE_VERIFY_ARTIFACTS ?? '.audit/system-session/backlog');
const origin = 'http://127.0.0.1:4317';
await mkdir(output, { recursive: true });
const directory = await mkdtemp(resolve(tmpdir(), 'pi-trace historical backlog '));
const active = await createStores(resolve(directory, 'a active'));
const history = await createStores(resolve(directory, 'z history'));
const receipts = [];
const statuses = [];
let child, closed;
let stdout = '';
let result = { verdict: 'not_verified', directory };
async function json(path) {
  const response = await fetch(`${origin}${path}`, { signal: AbortSignal.timeout(10000) });
  assert.equal(response.status, 200, path);
  return response.json();
}
async function until(predicate, message, timeout = 20000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await predicate()) return; await delay(10); }
  throw new Error(message);
}
async function status() { const value = await json('/api/v1/system'); statuses.push({ at: Date.now(), ...value }); return value; }
try {
  for (const provider of providers) {
    await active.createSession(provider, 'active', { project: '/synthetic/active' });
    await active.appendUsage(provider, 'active');
  }
  const historyPerProvider = 2000;
  for (let i = 0; i < historyPerProvider; i++) {
    for (const provider of providers) {
      const session = await history.createSession(provider, `history-${i}`, { project: '/synthetic/history' });
      if (provider !== 'opencode') {
        const header = JSON.parse(await readFile(session.source, 'utf8'));
        header.syntheticPadding = 'x'.repeat(48 * 1024);
        await writeFile(session.source, `${JSON.stringify(header)}\n`);
      }
    }
  }
  const args = ['system', ...providers.flatMap((provider) => ['--root', `${provider}=${active.roots[provider]}`, '--root', `${provider}=${history.roots[provider]}`])];
  const started = Date.now();
  child = spawn(binary, args, { cwd: root, stdio: ['ignore', 'pipe', 'pipe'] });
  closed = new Promise((done) => child.once('exit', (code, signal) => done({ code, signal })));
  child.stdout.on('data', (bytes) => stdout += bytes);
  child.stderr.on('data', (bytes) => stdout += bytes);
  await until(() => stdout.includes(`Pi Trace bootstrap: ${origin}`), 'owned service did not start');
  let items;
  await until(async () => { items = (await json('/api/v1/sessions?project=%2Fsynthetic%2Factive')).items; return items.length === 4; }, 'all four active stores must progress');
  const committed = await Promise.all(providers.map((provider) => active.appendUsage(provider, 'active')));
  await Promise.all(items.map(async (item) => {
    let ws;
    await until(async () => {
      ws = await json(`/api/v1/sessions/${item.id}/workspace`);
      if (ws.state !== 'ready') return false;
      const view = await json(`/api/v1/sessions/${item.id}/snapshots/${ws.snapshotId}/overview`);
      return view.tokenAccounting.totalTokens.value === '260';
    }, `selected ${item.provider} did not progress`);
    const observedAt = Date.now();
    const catalog = await status();
    const commit = committed.find((value) => value.provider === item.provider);
    receipts.push({ provider: item.provider, committedAt: commit.committedAt, observedAt, latencyMs: observedAt - commit.committedAt, backlogIncomplete: catalog.stores.some((store) => !store.sweepComplete), retainedEntries: catalog.coverage.retainedEntries, readRecords: catalog.metrics.readRecords });
  }));
  assert.ok(receipts.every((receipt) => receipt.latencyMs <= 2000), 'each provider advances within two seconds');
  assert.ok(receipts.every((receipt) => receipt.backlogIncomplete), 'every provider advances before the historical sweep finishes');
  await until(async () => (await status()).stores.every((store) => store.sweepComplete), 'historical traversal did not finish');
  const final = await status();
  assert.equal(final.coverage.retainedEntries, historyPerProvider * 4 + 4);
  assert.equal(final.coverage.state, 'complete');
  assert.equal(final.metrics.loadedEntries, 4);
  assert.ok(final.metrics.readRecords < 100, 'unselected history was not semantically decoded');
  assert.ok(final.metrics.maxActiveParseJobs <= 2);
  assert.ok(final.metrics.databaseConnections <= 16);
  assert.ok(final.metrics.retainedBytes <= final.limits.retainedDecodedBytes);
  result = { verdict: 'verified', directory, historyPerProvider, catalogEntries: historyPerProvider * 4 + 4, syntheticHeaderPaddingBytes: 48 * 1024, completedSweepMs: Date.now() - started, receipts, final };
} catch (error) {
  result = { ...result, message: error.message, stack: error.stack, receipts, stdout };
  console.error(error);
  process.exitCode = 1;
} finally {
  if (child && child.exitCode === null) {
    child.kill('SIGTERM');
    const stopped = await Promise.race([closed, delay(5000, null, { ref: false })]);
    result.shutdown = stopped ?? { forced: true };
    if (!stopped) { child.kill('SIGKILL'); result.verdict = 'not_verified'; process.exitCode = 1; }
  }
  active.close(); history.close();
  await writeFile(resolve(output, 'status-samples.json'), JSON.stringify(statuses, null, 2) + '\n');
  await writeFile(resolve(output, 'result.json'), JSON.stringify(result, null, 2) + '\n');
  console.log(JSON.stringify({ verdict: result.verdict, output }));
}
