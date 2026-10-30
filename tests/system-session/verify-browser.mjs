import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { appendFile, mkdir, mkdtemp, readFile, rename, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { createStores, providers, fabricatedBody } from './fixture-store.mjs';

const packageRoot = resolve(process.env.PI_TRACE_VERIFY_PACKAGE ?? process.cwd());
const output = resolve(process.env.PI_TRACE_VERIFY_ARTIFACTS ?? '.audit/system-session/browser');
const binary = resolve(process.env.PI_TRACE_VERIFY_BINARY ?? resolve(packageRoot, 'target/release/pi-trace'));
const modulePath = process.env.PI_TRACE_PLAYWRIGHT_MODULE;
const { chromium } = await import(modulePath ? pathToFileURL(resolve(modulePath)).href : 'playwright');
const origin = 'http://127.0.0.1:4317';
await mkdir(output, { recursive: true });
const sourceDirectory = await mkdtemp(resolve(tmpdir(), 'pi-trace browser stores '));
const stores = await createStores(sourceDirectory);
const network = [];
const errors = [];
const observations = [];
let service, browser, exited;
let serviceOutput = '';
let result = { verdict: 'not_verified', sourceDirectory };
async function until(predicate, message, timeout = 15000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await predicate()) return; await delay(50); }
  throw new Error(message);
}
async function json(path) {
  const response = await fetch(`${origin}${path}`, { signal: AbortSignal.timeout(3000) });
  assert.equal(response.status, 200, path);
  const text = await response.text();
  assert.ok(!text.includes(fabricatedBody), 'metadata does not disclose bodies');
  return JSON.parse(text);
}
async function workspace(entry) { return json(`/api/v1/sessions/${entry.item.id}/workspace`); }
async function capture(entry, label) {
  for (const theme of ['light', 'dark']) {
    const current = await entry.page.locator('html').getAttribute('data-theme');
    if (current !== theme) await entry.page.getByRole('button', { name: /^Light$|^Dark$|Switch to (?:light|dark) theme/ }).click();
    for (const width of [1440, 375]) {
      await entry.page.setViewportSize({ width, height: 1000 });
      await entry.page.keyboard.press('Control+Home');
      assert.ok(await entry.page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'no horizontal overflow');
      await entry.page.screenshot({ path: resolve(output, `${entry.provider}-${label}-${width}-${theme}.png`), fullPage: true });
    }
  }
}
function revealButton(page) { return page.getByRole('button', { name: /^Reveal (?:arguments \/ recorded result|input \/ output)$/ }); }
async function selectRoot(entry, total) {
  await entry.page.locator('.trace-row .select-row').first().click();
  await entry.page.locator(`.inspector[data-token-total="${total}"]`).waitFor();
}
try {
  const entries = [];
  for (const provider of providers) {
    const session = await stores.createSession(provider, 'functional');
    await stores.appendUsage(provider, 'functional');
    entries.push({ provider, session });
  }
  service = spawn(binary, stores.args, { cwd: packageRoot, stdio: ['ignore', 'pipe', 'pipe'] });
  exited = new Promise((done) => service.once('exit', (code, signal) => done({ code, signal })));
  service.stdout.on('data', (bytes) => serviceOutput += bytes);
  service.stderr.on('data', (bytes) => serviceOutput += bytes);
  await until(async () => {
    if (service.exitCode !== null) throw new Error(serviceOutput);
    if (!serviceOutput.includes(`Pi Trace bootstrap: ${origin}`)) return false;
    return (await json('/api/v1/health')).status === 'ok';
  }, 'owned native service did not start');
  await until(async () => (await json('/api/v1/sessions')).items.length === 4, 'four-provider discovery');
  const items = (await json('/api/v1/sessions')).items;
  browser = await chromium.launch({ headless: true, executablePath: process.env.PI_TRACE_CHROME ?? '/usr/bin/google-chrome', args: ['--no-sandbox', '--disable-background-networking'] });
  for (const entry of entries) {
    entry.item = items.find((item) => item.provider === entry.provider);
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    entry.page = await context.newPage();
    const page = entry.page;
    page.on('pageerror', (error) => errors.push({ provider: entry.provider, type: 'page', message: error.message }));
    page.on('console', (message) => { if (['error', 'warning'].includes(message.type())) errors.push({ provider: entry.provider, type: 'console', message: message.text() }); });
    page.on('request', (request) => network.push({ provider: entry.provider, at: Date.now(), url: request.url(), method: request.method() }));
    page.on('requestfailed', (request) => { if (!request.failure()?.errorText.includes('ERR_ABORTED')) errors.push({ provider: entry.provider, type: 'request', message: request.failure()?.errorText }); });
    await page.goto(`${origin}/?session=${entry.item.id}&theme=light`);
    await page.locator(`.investigation[data-session-id="${entry.item.id}"] .trace-row`).first().waitFor();
    await selectRoot(entry, '130');
  }
  const staticBefore = await stores.fingerprint();
  await delay(1200);
  assert.deepEqual(await stores.fingerprint(), staticBefore, 'inspection leaves DB, WAL, SHM, files and permissions unchanged');
  assert.equal(network.filter((request) => /\/content(?:\?|$)/.test(request.url)).length, 0, 'no implicit body reads');
  for (const entry of entries) {
    const page = entry.page;
    await stores.appendTool(entry.provider, 'functional');
    const tool = page.locator('.trace-row[data-type="tool"]').filter({ hasText: 'synthetic_tool' }).locator('.select-row');
    await tool.waitFor();
    await tool.click();
    await revealButton(page).waitFor();
    const before = await workspace(entry);
    const requestsBefore = network.filter((request) => /\/content(?:\?|$)/.test(request.url)).length;
    await revealButton(page).click();
    await page.locator('.content-box').filter({ hasText: 'FABRICATED-ONLY-secret' }).first().waitFor();
    assert.equal(await page.locator('.content-box img').count(), 0, 'untrusted markup stays text');
    assert.ok(network.filter((request) => /\/content(?:\?|$)/.test(request.url)).length > requestsBefore, 'reveal causes deliberate body request');
    await stores.appendTool(entry.provider, 'functional', true);
    await until(async () => (await workspace(entry)).snapshotId !== before.snapshotId, 'tool result creates a fresh snapshot');
    await revealButton(page).waitFor();
    assert.equal(await page.locator('.content-box').count(), 0, 'new evidence hides revealed body');
    await revealButton(page).click();
    await page.locator('.content-box').filter({ hasText: 'FABRICATED-ONLY-secret' }).last().waitFor();
    assert.equal(await page.locator('.content-box').count(), 2, 'recorded arguments and completed output are both rendered');
    assert.equal(await page.locator('.content-box img').count(), 0);
    await capture(entry, 'tool-result');
    await stores.appendUsage(entry.provider, 'functional');
    await revealButton(page).waitFor();
    await selectRoot(entry, '260');
    const current = await workspace(entry);
    const overview = await json(`/api/v1/sessions/${entry.item.id}/snapshots/${current.snapshotId}/overview`);
    assert.equal(overview.tokenAccounting.totalTokens.value, '260');
    console.log(`${entry.provider} tool result and usage verified`);
    observations.push({ provider: entry.provider, kind: 'tool-result-and-usage', sessionId: entry.item.id, runId: current.runId, snapshotId: current.snapshotId, total: '260' });
  }
  const pi = entries.find((entry) => entry.provider === 'pi');
  const codex = entries.find((entry) => entry.provider === 'codex');
  const claude = entries.find((entry) => entry.provider === 'claude');
  const opencode = entries.find((entry) => entry.provider === 'opencode');
  const page = pi.page;
  let unblock, held;
  const heldReady = new Promise((done) => held = done);
  const gate = new Promise((done) => unblock = done);
  await page.route(`**/sessions/${codex.item.id}/snapshots/*/overview`, async (route) => {
    const response = await route.fetch();
    const body = await response.body();
    held();
    await gate;
    try { await route.fulfill({ response, body }); } catch (error) { observations.push({ kind: 'old-response-client-aborted', message: error.message }); }
  });
  await page.locator('.session-choice').filter({ hasText: 'Codex' }).click();
  await Promise.race([heldReady, delay(5000).then(() => { throw new Error('old-session overview was not held'); })]);
  await page.locator('.session-choice').filter({ hasText: 'Claude Code' }).click();
  await page.locator(`.investigation[data-session-id="${claude.item.id}"] .trace-row`).first().waitFor();
  unblock();
  await delay(700);
  assert.equal(await page.locator('.investigation').getAttribute('data-session-id'), claude.item.id, 'late old response cannot replace new session');
  await page.unrouteAll({ behavior: 'wait' });
  await page.locator('.catalog-filters select').selectOption('pi');
  await page.getByText('The selected session is outside the current filters. Its investigation remains open.').waitFor();
  assert.equal(await page.locator('.investigation').getAttribute('data-session-id'), claude.item.id);
  assert.ok(!new URL(page.url()).search.includes('synthetic'), 'URL does not include project or source paths');
  observations.push({ kind: 'late-response-and-filter', selected: claude.item.id, filter: 'pi' });
  await page.locator('.catalog-filters select').selectOption('');
  await page.locator('.session-choice').filter({ hasText: 'Pi Agent' }).click();
  await page.locator(`.investigation[data-session-id="${pi.item.id}"] .trace-row`).first().waitFor();
  await selectRoot(pi, '260');
  const oldPi = await workspace(pi);
  const header = (await readFile(pi.session.source, 'utf8')).split('\n')[0];
  await writeFile(pi.session.source, `${header}\n${JSON.stringify({ type: 'message', id: 'replacement-entry', timestamp: '2026-10-30T10:00:00Z', message: { role: 'assistant', content: [{ type: 'text', text: fabricatedBody }], usage: { input: 4, output: 3, cacheRead: 2, cacheWrite: 0, totalTokens: 9 }, stopReason: 'stop' } })}\n`);
  await until(async () => (await workspace(pi)).runId !== oldPi.runId, 'supported Pi rewrite starts a fresh run');
  await until(async () => await page.locator('.inspector[data-span-id]').count() === 0, 'rewrite clears old span selection');
  await selectRoot(pi, '9');
  await capture(pi, 'rewrite');
  observations.push({ kind: 'rewrite', beforeRun: oldPi.runId, after: await workspace(pi) });
  await appendFile(codex.session.source, '{"FABRICATED-MALFORMED-PRIVATE":"broken\n');
  await stores.appendUsage('codex', 'functional');
  await until(async () => {
    const ws = await workspace(codex);
    const overview = await json(`/api/v1/sessions/${codex.item.id}/snapshots/${ws.snapshotId}/overview`);
    return overview.diagnosticCount > 0 && overview.tokenAccounting.totalTokens.value === '390';
  }, 'malformed middle record retains valid surrounding usage');
  await selectRoot(codex, '390');
  await capture(codex, 'malformed');
  const removed = `${claude.session.source}.removed`;
  await rename(claude.session.source, removed);
  await claude.page.getByText(/Showing the last valid evidence/).waitFor({ timeout: 15000 });
  const staleClaude = await workspace(claude);
  assert.equal(staleClaude.state, 'stale');
  await capture(claude, 'retention-deletion');
  await rename(removed, claude.session.source);
  await until(async () => (await workspace(claude)).state === 'ready', 'restored transcript recovers');
  stores.changeSchema(true);
  await opencode.page.getByText(/unsupported_schema/).waitFor({ timeout: 15000 });
  assert.equal((await workspace(opencode)).state, 'stale');
  await capture(opencode, 'unsupported-schema');
  stores.changeSchema(false);
  await until(async () => (await workspace(opencode)).state === 'ready', 'restored database schema recovers');
  const createdAt = Date.now();
  const late = await stores.createSession('claude', 'late-discovery');
  await stores.appendUsage('claude', 'late-discovery');
  await until(async () => (await json('/api/v1/sessions')).items.some((item) => item.nativeId === late.id), 'new session after recovery is discovered', 5000);
  await claude.page.locator('.session-choice').filter({ hasText: late.id }).waitFor();
  assert.ok(Date.now() - createdAt <= 5000);
  assert.equal(await opencode.page.locator('.investigation').getAttribute('data-session-id'), opencode.item.id, 'independent browser context keeps its provider');
  assert.equal(network.filter((request) => !request.url.startsWith(origin) && !request.url.startsWith('data:')).length, 0, 'no external request');
  assert.equal(errors.length, 0, 'clean browser console and requests');
  result = { verdict: 'verified', sourceDirectory, browser: browser.version(), providers, scenarios: ['four-provider-discovery', 'four-provider-tool-result', 'four-provider-usage', 'explicit-reveal-text-only', 'body-hidden-on-revision', 'late-response-session-switch', 'selected-outside-filter', 'Pi-rewrite-new-run', 'Codex-malformed-middle', 'Claude-retention-deletion-recovery', 'OpenCode-schema-failure-recovery', 'new-session-after-recovery', 'two-client-isolation', 'themes-and-375px'], browserErrors: errors.length, externalRequests: 0, observations };
} catch (error) {
  for (const context of browser?.contexts() ?? []) { for (const [index, page] of context.pages().entries()) { await page.screenshot({ path: resolve(output, `failure-${context === browser.contexts()[0] ? 'first' : browser.contexts().indexOf(context)}-${index}.png`), fullPage: true }).catch(() => {}); } }
  result = { ...result, message: error.message, stack: error.stack, observations, errors, serviceOutput };
  console.error(error);
  process.exitCode = 1;
} finally {
  await writeFile(resolve(output, 'network.json'), JSON.stringify(network, null, 2) + '\n');
  await browser?.close();
  if (service && service.exitCode === null) {
    service.kill('SIGTERM');
    const stopped = await Promise.race([exited, delay(5000, null, { ref: false })]);
    result.shutdown = stopped ?? { forced: true };
    if (!stopped) { service.kill('SIGKILL'); process.exitCode = 1; result.verdict = 'not_verified'; }
  }
  stores.close();
  await writeFile(resolve(output, 'result.json'), JSON.stringify(result, null, 2) + '\n');
  console.log(JSON.stringify({ verdict: result.verdict, output }));
}
