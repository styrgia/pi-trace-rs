import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, writeFile, access } from 'node:fs/promises';
import { tmpdir, cpus, totalmem, platform, release, loadavg } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { createStores, providers, fabricatedBody } from './fixture-store.mjs';

const packageRoot = resolve(process.env.PI_TRACE_VERIFY_PACKAGE ?? process.cwd());
const output = resolve(process.env.PI_TRACE_VERIFY_ARTIFACTS ?? '.audit/system-session-run');
const binary = resolve(process.env.PI_TRACE_VERIFY_BINARY ?? resolve(packageRoot, 'target/release/pi-trace'));
const modulePath = process.env.PI_TRACE_PLAYWRIGHT_MODULE;
const { chromium } = await import(modulePath ? pathToFileURL(resolve(modulePath)).href : 'playwright');
const duration = Number(process.env.PI_TRACE_VERIFY_DURATION_MS ?? 30000);
const historical = Number(process.env.PI_TRACE_VERIFY_HISTORY_PER_PROVIDER ?? 249);
const origin = 'http://127.0.0.1:4317';
await mkdir(output, { recursive: true });
await access(binary);
const sourceDirectory = await mkdtemp(resolve(tmpdir(), 'pi-trace-system stores '));
const stores = await createStores(sourceDirectory);
const errors = [];
const requests = [];
const responses = [];
const commits = [];
const discoveries = [];
const browserObservations = [];
const statusSamples = [];
let service;
let browser;
let closed;
let serviceOutput = '';
let requestFailures = 0;
let serviceDone = false;
let verifiedReport;
const machine = { platform: platform(), release: release(), cpu: cpus()[0]?.model, cores: cpus().length, totalMemory: totalmem(), load: loadavg(), node: process.version, startedAt: new Date().toISOString(), binarySha256: createHash('sha256').update(await readFile(binary)).digest('hex') };

function coherent(frame, session) {
  assert.equal(frame.session, session, 'rendered session matches selected provider');
  assert.equal(frame.snapshot, frame.rowSnapshot, 'row belongs to published snapshot');
  assert.equal(frame.snapshot, frame.detailSnapshot, 'inspector belongs to published snapshot');
  assert.equal(frame.row, frame.detail, 'row and inspector totals agree');
  assert.equal(frame.rowState, frame.detailState, 'row and inspector coverage agree');
}
const canary = { session: 'session-a', snapshot: 'snapshot-a', rowSnapshot: 'snapshot-a', detailSnapshot: 'snapshot-a', row: '130', detail: '130', rowState: 'exact', detailState: 'exact' };
coherent(canary, 'session-a');
for (const mutation of [{ session: 'session-b' }, { detail: '131' }, { detailSnapshot: 'snapshot-b' }, { rowState: 'partial' }]) {
  assert.throws(() => coherent({ ...canary, ...mutation }, 'session-a'), 'known wrong frame must fail the measurement instrument');
}

async function json(path, options = {}) {
  const response = await fetch(`${origin}${path}`, { ...options, signal: AbortSignal.timeout(3000) });
  const text = await response.text();
  if (!response.ok) { requestFailures++; throw new Error(`${path}: ${response.status} ${text}`); }
  assert.ok(Buffer.byteLength(text) <= 1024 * 1024, 'bounded HTTP response');
  assert.ok(!text.includes(fabricatedBody), 'metadata never contains fabricated bodies');
  return JSON.parse(text);
}
async function until(predicate, message, milliseconds = 15000) {
  const deadline = Date.now() + milliseconds;
  while (Date.now() < deadline) { if (await predicate()) return; await delay(50); }
  throw new Error(message);
}
async function status() { const value = await json('/api/v1/system'); statusSamples.push({ at: Date.now(), ...value }); return value; }
async function catalog(provider) {
  const entries = [];
  let cursor = null;
  do {
    const query = new URLSearchParams({ provider, limit: '100' });
    if (cursor) query.set('cursor', cursor);
    const page = await json(`/api/v1/sessions?${query}`);
    entries.push(...page.items); cursor = page.nextCursor;
  } while (cursor);
  return entries;
}
function processSample(pid) {
  try {
    const raw = execFileSync('cat', [`/proc/${pid}/stat`], { encoding: 'utf8' });
    const fields = raw.slice(raw.lastIndexOf(')') + 2).trim().split(/\s+/);
    const ticks = Number(execFileSync('getconf', ['CLK_TCK'], { encoding: 'utf8' }));
    const pageBytes = Number(execFileSync('getconf', ['PAGESIZE'], { encoding: 'utf8' }));
    return { at: Date.now(), cpuMs: (Number(fields[11]) + Number(fields[12])) * 1000 / ticks, rssBytes: Number(fields[21]) * pageBytes };
  } catch { return null; }
}

try {
  for (let i = 0; i < historical; i++) {
    for (const provider of providers) {
      const key = `history-${i}`;
      await stores.createSession(provider, key);
      await stores.appendUsage(provider, key);
    }
  }
  service = spawn(binary, stores.args, { cwd: packageRoot, stdio: ['ignore', 'pipe', 'pipe'], shell: false });
  closed = new Promise((resolveExit) => service.once('exit', (code, signal) => { serviceDone = true; resolveExit({ code, signal }); }));
  service.stdout.on('data', (bytes) => serviceOutput += bytes);
  service.stderr.on('data', (bytes) => serviceOutput += bytes);
  await until(async () => {
    if (serviceDone) throw new Error(serviceOutput);
    if (!serviceOutput.includes(`Pi Trace bootstrap: ${origin}`)) return false;
    try { return (await json('/api/v1/health')).status === 'ok'; } catch { return false; }
  }, 'native service did not start');
  const initialStarted = Date.now();
  await until(async () => {
    const value = await status();
    return value.coverage.retainedEntries === historical * 4 && value.stores.every((store) => store.sweepComplete);
  }, 'initial historical discovery did not finish', 30000);
  const initialSweep = { milliseconds: Date.now() - initialStarted, complete: true, entries: historical * 4 };
  const active = [];
  for (const provider of providers) {
    const session = await stores.createSession(provider, 'active');
    const createdAt = Date.now();
    await stores.appendUsage(provider, 'active');
    active.push({ provider, session, createdAt });
  }
  for (const entry of active) {
    await until(async () => {
      const found = (await catalog(entry.provider)).find((item) => item.nativeId === entry.session.id);
      if (!found) return false;
      entry.catalog = found;
      discoveries.push({ provider: entry.provider, createdAt: entry.createdAt, observedAt: Date.now(), latencyMs: Date.now() - entry.createdAt });
      return true;
    }, `new ${entry.provider} session not discovered`, 6000);
  }
  assert.equal(new Set(active.map((entry) => entry.catalog.id)).size, 4, 'same native ID has separate provider namespaces');
  browser = await chromium.launch({ headless: true, executablePath: process.env.PI_TRACE_CHROME ?? '/usr/bin/google-chrome', args: ['--no-sandbox', '--disable-background-networking'] });
  machine.browser = browser.version();
  const pages = [];
  for (const entry of active) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    const page = await context.newPage();
    await page.exposeFunction('recordTraceFrame', (frame) => browserObservations.push({ provider: entry.provider, ...frame }));
    page.on('pageerror', (error) => errors.push({ provider: entry.provider, type: 'page', message: error.message }));
    page.on('console', (message) => { if (['error', 'warning'].includes(message.type())) errors.push({ provider: entry.provider, type: 'console', message: message.text(), location: message.location() }); });
    page.on('request', (request) => requests.push({ provider: entry.provider, at: Date.now(), url: request.url(), method: request.method() }));
    page.on('response', (response) => responses.push({ provider: entry.provider, at: Date.now(), url: response.url(), status: response.status() }));
    page.on('requestfailed', (request) => { if (!request.failure()?.errorText.includes('ERR_ABORTED')) errors.push({ provider: entry.provider, type: 'request', url: request.url(), message: request.failure()?.errorText }); });
    await page.goto(`${origin}/?session=${entry.catalog.id}&theme=light`);
    await page.locator(`.investigation[data-session-id="${entry.catalog.id}"] .trace-row`).first().waitFor({ timeout: 15000 });
    await page.locator('.trace-row').first().locator('.select-row').click();
    await page.locator('.inspector[data-token-total="130"]').waitFor();
    assert.equal(await page.evaluate(() => document.visibilityState), 'visible', 'workload page remains visible');
    await page.evaluate(() => {
      let last = '';
      function inspect() {
        const investigation = document.querySelector('.investigation');
        const inspector = document.querySelector('.inspector[data-span-id]');
        if (!investigation || !inspector) return;
        const row = [...document.querySelectorAll('.trace-row')].find((element) => element.dataset.spanId === inspector.dataset.spanId);
        if (!row) return;
        const frame = { at: Date.now(), session: investigation.dataset.sessionId, run: investigation.dataset.runId, snapshot: investigation.dataset.snapshotId, rowSnapshot: row.dataset.snapshotId, detailSnapshot: inspector.dataset.snapshotId, span: inspector.dataset.spanId, row: row.dataset.tokenTotal, detail: inspector.dataset.tokenTotal, rowState: row.dataset.tokenTotalState, detailState: inspector.dataset.tokenTotalState };
        const key = JSON.stringify({ ...frame, at: 0 });
        if (key !== last) { last = key; void window.recordTraceFrame(frame); }
      }
      new MutationObserver(inspect).observe(document.body, { subtree: true, attributes: true, childList: true, characterData: true });
      inspect();
    });
    pages.push({ ...entry, page, context });
  }
  await until(async () => (await status()).coverage.retainedEntries === historical * 4 + 4, 'catalog count mismatch');
  const staticBefore = await stores.fingerprint();
  const idleStart = { metrics: await status(), process: processSample(service.pid) };
  await delay(2000);
  const idleEnd = { metrics: await status(), process: processSample(service.pid) };
  assert.deepEqual(await stores.fingerprint(), staticBefore, 'static source bytes and permissions unchanged during inspection');
  assert.equal(idleEnd.metrics.metrics.readRecords, idleStart.metrics.metrics.readRecords, 'idle polls do not decode histories again');
  assert.equal(idleEnd.metrics.catalogRevision, idleStart.metrics.catalogRevision, 'idle catalog revision stable');
  const activeStart = processSample(service.pid);
  const started = Date.now();
  const count = Math.floor(duration / 100);
  for (let i = 0; i < count; i++) {
    await delay(Math.max(0, started + i * 100 - Date.now()));
    const writeStartedAt = Date.now();
    commits.push({ ...await stores.appendUsage(providers[i % 4], 'active', 130 + 10 * (i % 4)), writeStartedAt });
  }
  await delay(Math.max(0, started + duration - Date.now()));
  await until(() => commits.every((commit) => browserObservations.some((frame) => frame.provider === commit.provider && frame.at >= commit.writeStartedAt && frame.row === frame.detail && frame.row !== 'null' && BigInt(frame.row) >= BigInt(commit.total))), 'not every committed update reached coherent rows and inspector', 6000);
  const activeEnd = processSample(service.pid);
  const latencies = commits.map((commit) => {
    const first = browserObservations.find((frame) => frame.provider === commit.provider && frame.at >= commit.writeStartedAt && frame.row === frame.detail && frame.row !== 'null' && BigInt(frame.row) >= BigInt(commit.total));
    assert.ok(first);
    return { ...commit, observedAt: first.at, latencyMs: Math.max(0, first.at - commit.committedAt), snapshot: first.snapshot };
  });
  for (const frame of browserObservations) {
    coherent(frame, active.find((entry) => entry.provider === frame.provider).catalog.id);
  }
  for (const entry of pages) {
    const workspace = await json(`/api/v1/sessions/${entry.catalog.id}/workspace`);
    const overview = await json(`/api/v1/sessions/${entry.catalog.id}/snapshots/${workspace.snapshotId}/overview`);
    assert.equal(overview.source.provider, entry.provider, 'source provider belongs to selected session');
    assert.equal(overview.source.storeId, entry.catalog.storeId, 'source store belongs to selected session');
    const expected = {
      inputTokens: entry.session.total - 50 * entry.session.sequence,
      outputTokens: 30 * entry.session.sequence,
      cacheReadTokens: 20 * entry.session.sequence,
      cacheWriteTokens: 0,
      totalTokens: entry.session.total,
    };
    for (const [component, value] of Object.entries(expected)) {
      assert.deepEqual(overview.tokenAccounting[component], { state: 'exact', value: String(value) }, `${entry.provider} exact ${component}`);
    }
    assert.equal(overview.tokenAccounting.contributorCount, entry.session.sequence, 'every persisted response contributes once');
    entry.expected = expected;
    assert.equal(await entry.page.locator('.inspector').getAttribute('data-token-total'), String(entry.session.total));
    await entry.page.keyboard.press('Control+Home');
    assert.equal(await entry.page.locator('html').getAttribute('data-theme'), 'light');
    await entry.page.screenshot({ path: resolve(output, `${entry.provider}-desktop-light.png`), fullPage: true });
    await entry.page.getByRole('button', { name: /^Dark$|Switch to dark theme/ }).click();
    assert.equal(await entry.page.locator('html').getAttribute('data-theme'), 'dark');
    await entry.page.screenshot({ path: resolve(output, `${entry.provider}-desktop-dark.png`), fullPage: true });
    await entry.page.setViewportSize({ width: 375, height: 900 });
    assert.ok(await entry.page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), '375px layout has no horizontal overflow');
    await entry.page.screenshot({ path: resolve(output, `${entry.provider}-mobile-dark.png`), fullPage: true });
    await entry.page.getByRole('button', { name: /^Light$|Switch to light theme/ }).click();
    assert.equal(await entry.page.locator('html').getAttribute('data-theme'), 'light');
    await entry.page.keyboard.press('Tab');
    const focus = await entry.page.evaluate(() => {
      const element = document.activeElement;
      const style = getComputedStyle(element);
      return { semantic: ['BUTTON', 'INPUT', 'SELECT', 'A'].includes(element.tagName), visible: element.matches(':focus-visible') && style.outlineStyle !== 'none' && parseFloat(style.outlineWidth) > 0 };
    });
    assert.ok(focus.semantic && focus.visible, 'keyboard focus stays on a visibly focused semantic control');
    await entry.page.screenshot({ path: resolve(output, `${entry.provider}-mobile-light.png`), fullPage: true });
  }
  const external = requests.filter((request) => !request.url.startsWith(origin) && !request.url.startsWith('data:'));
  const content = requests.filter((request) => /\/content(?:\?|$)/.test(request.url));
  assert.equal(external.length, 0, 'trace markup triggers no external request');
  assert.equal(content.length, 0, 'metadata refresh never requests bodies');
  assert.equal(errors.length, 0, 'browser console and requests remain clean');
  assert.equal(requestFailures, 0, 'producer verification HTTP has no failures');
  const browserHttpFailures = responses.filter((response) => response.status >= 400);
  assert.equal(browserHttpFailures.length, 0, 'browser HTTP has no failures');
  assert.ok(discoveries.every((item) => item.latencyMs <= 5000), 'new-session discovery within five seconds');
  assert.ok(latencies.every((item) => item.latencyMs <= 2000), 'selected updates within two seconds');
  const metrics = await status();
  assert.ok(metrics.metrics.loadedEntries <= 4);
  assert.ok(metrics.metrics.maxActiveParseJobs <= 2);
  assert.ok(metrics.metrics.retainedBytes <= metrics.limits.retainedDecodedBytes);
  assert.ok(metrics.metrics.databaseConnections <= 16);
  verifiedReport = { verdict: 'verified', machine, sourceDirectory, initialSweep, discoveries, expected: pages.map(({ provider, expected, session }) => ({ provider, ...expected, contributorCount: session.sequence })), durationMs: duration, commits: commits.length, selectedMaxLatencyMs: Math.max(...latencies.map((item) => item.latencyMs)), discoveryMaxLatencyMs: Math.max(...discoveries.map((item) => item.latencyMs)), idleStart, idleEnd, activeStart, activeEnd, metrics, externalRequests: external.length, implicitContentRequests: content.length, browserErrors: errors.length, browserHttpFailures: browserHttpFailures.length };
  await writeFile(resolve(output, 'latencies.json'), JSON.stringify(latencies, null, 2) + '\n');

} catch (error) {
  await writeFile(resolve(output, 'failure.json'), JSON.stringify({ verdict: 'not_verified', message: error.message, stack: error.stack, machine, errors, requestFailures, commits: commits.length, serviceOutput, lastStatus: statusSamples.at(-1) }, null, 2) + '\n');
  console.error(error);
  process.exitCode = 1;
} finally {
  await writeFile(resolve(output, 'responses.json'), JSON.stringify(responses, null, 2) + '\n');
  await writeFile(resolve(output, 'commits.json'), JSON.stringify(commits, null, 2) + '\n');
  await writeFile(resolve(output, 'network.json'), JSON.stringify(requests, null, 2) + '\n');
  await writeFile(resolve(output, 'browser-frames.json'), JSON.stringify(browserObservations, null, 2) + '\n');
  await writeFile(resolve(output, 'status-samples.json'), JSON.stringify(statusSamples, null, 2) + '\n');
  await browser?.close();
  if (service && !serviceDone) {
    service.kill('SIGTERM');
    const exited = await Promise.race([closed, delay(5000, null, { ref: false })]);
    if (!exited) { service.kill('SIGKILL'); process.exitCode = 1; }
    if (verifiedReport) verifiedReport.shutdown = exited ?? { forced: true };
  }
  stores.close();
  if (verifiedReport) {
    if (process.exitCode) verifiedReport.verdict = 'not_verified';
    await writeFile(resolve(output, 'result.json'), JSON.stringify(verifiedReport, null, 2) + '\n');
    console.log(JSON.stringify({ verdict: verifiedReport.verdict, samples: verifiedReport.commits, selectedMaxMs: verifiedReport.selectedMaxLatencyMs, output }));
  }
}
