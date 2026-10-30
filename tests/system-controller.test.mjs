import assert from 'node:assert/strict'
import test from 'node:test'
import { SystemController } from '../web/src/lib/system-controller.ts'
import { viewUrl, readViewState } from '../web/src/lib/view-state.ts'

const delay = (ms = 0) => new Promise((resolve) => setTimeout(resolve, ms))
async function until(condition) { for (let i = 0; i < 100; i++) { if (condition()) return; await delay(2) } assert.fail('Controlled state was not published') }
function deferred() { let resolve; let reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no }); return { promise, resolve, reject } }
const capabilities = { profile: 'synthetic', messages: 'complete', usage: 'partial', executionParents: 'partial', timing: 'missing', persistence: 'persistedRecordsOnly' }
const item = (id, provider = 'pi', project = '/synthetic/project') => ({ id, provider, storeId: '1111111111111111', nativeId: 'fabricated', recordingId: 'fabricated', project, sourceRevision: 'v1', capabilities, source: { kind: 'jsonl', provider, storeId: '1111111111111111', source: 'fabricated.jsonl', line: '1', byteRange: null }, relationships: [], activityMs: null, ingestionState: 'discovered', lastObservedMs: 1, processState: 'unknown' })
const a = item('aaaaaaaaaaaaaaaa', 'codex')
const b = item('bbbbbbbbbbbbbbbb', 'pi')
const coverage = { state: 'complete', limits: [], retainedEntries: 2, omittedCount: 0 }
const catalog = (items = [a, b], revision = '1', nextCursor = null) => ({ items, revision, nextCursor, coverage })
const workspace = (sessionId, snapshotId = `${sessionId}-s1`, runId = `${sessionId}-run`) => ({ state: 'ready', sessionId, snapshotId, runId, revision: 1, progress: 'current', error: null, sourceRevision: 'v1', capabilities, processState: 'unknown' })
function harness(overrides = {}) {
  const frames = []
  let source = catalog()
  const api = { status: async () => ({ mode: 'system', catalogRevision: source.revision, stores: [], disabledProviders: [], coverage, limits: {}, metrics: {} }), catalog: async () => source, workspace: async (id) => workspace(id), retry: async (id) => workspace(id), ...overrides }
  const requests = []
  const controller = new SystemController({ api, intervalMs: 100000, investigationApi: (id) => ({ overview: async (snapshotId) => { requests.push({ id, snapshotId }); return { snapshotId } } }), onchange: (frame) => frames.push(structuredClone(frame)), timeoutMs: 1000 })
  return { controller, frames, requests, get latest() { return frames.at(-1) }, catalog: (next) => { source = next } }
}

test('catalog reorder, filters, and page exclusion preserve selected session independently of rows', async () => {
  const h = harness()
  h.controller.start()
  try {
    await until(() => h.latest?.catalog)
    h.controller.select(a)
    await until(() => h.latest?.overview)
    h.catalog(catalog([b, a], '2')); h.controller.resync()
    await until(() => h.latest.catalog?.revision === '2')
    assert.equal(h.latest.selectedId, a.id)
    assert.equal(h.latest.overview.snapshotId, `${a.id}-s1`)
    h.catalog(catalog([b], '3')); h.controller.filter('pi', '/synthetic/project')
    await until(() => h.latest.catalog?.revision === '3')
    assert.equal(h.latest.selectedId, a.id)
    assert.equal(h.latest.selected.provider, 'codex')
    assert.equal(h.latest.catalog.items.some((item) => item.id === a.id), false)
  } finally { h.controller.stop() }
})

test('late workspace and initial overview from a superseded session cannot replace the new session', async () => {
  const old = deferred(); const overview = deferred(); const frames = []
  const api = { status: async () => ({ catalogRevision: '1' }), catalog: async () => catalog(), workspace: (id) => id === a.id ? old.promise : Promise.resolve(workspace(id)), retry: async (id) => workspace(id) }
  const controller = new SystemController({ api, intervalMs: 100000, investigationApi: (id) => ({ overview: (snapshotId) => id === b.id ? overview.promise : Promise.resolve({ snapshotId }) }), onchange: (frame) => frames.push(structuredClone(frame)) })
  controller.start()
  try {
    controller.select(a); controller.select(b)
    await until(() => frames.at(-1)?.workspace?.sessionId === b.id)
    old.resolve(workspace(a.id)); overview.resolve({ snapshotId: `${b.id}-s1` })
    await until(() => frames.at(-1)?.overview)
    assert.equal(frames.at(-1).selectedId, b.id)
    assert.equal(frames.at(-1).overview.snapshotId, `${b.id}-s1`)
    assert.equal(frames.some((frame) => frame.workspace?.sessionId === a.id), false)
  } finally { controller.stop() }
})

test('filter change aborts a pending catalog and ignores its late HTTP response', async () => {
  const pending = deferred(); let calls = 0; let aborted = false
  const h = harness({ catalog: (_query, signal) => { calls++; if (calls === 1) { signal.addEventListener('abort', () => aborted = true); return pending.promise } return Promise.resolve(catalog([b], '2')) } })
  h.controller.start()
  try {
    await until(() => calls === 1)
    h.controller.filter('pi', '')
    await until(() => h.latest?.catalog?.revision === '2')
    assert.equal(aborted, true)
    pending.resolve(catalog([a], '1')); await delay(5)
    assert.equal(h.latest.catalog.revision, '2')
    assert.equal(h.latest.catalog.items[0].id, b.id)
  } finally { h.controller.stop() }
})

test('two controllers can investigate different providers without selecting each other or requesting bodies', async () => {
  const one = harness(); const two = harness()
  one.controller.start(); two.controller.start()
  try {
    one.controller.select(a); two.controller.select(b)
    await until(() => one.latest?.overview && two.latest?.overview)
    assert.equal(one.latest.selected.provider, 'codex'); assert.equal(two.latest.selected.provider, 'pi')
    assert.deepEqual(one.requests.map((request) => request.id), [a.id]); assert.deepEqual(two.requests.map((request) => request.id), [b.id])
  } finally { one.controller.stop(); two.controller.stop() }
})

test('visibility resync retains valid evidence and requests current session status immediately', async () => {
  let calls = 0
  const h = harness({ workspace: async (id) => { calls++; return workspace(id) } })
  h.controller.start(); h.controller.select(a)
  try {
    await until(() => h.latest?.overview)
    h.controller.setVisible(false); const previous = calls
    await delay(5); assert.equal(calls, previous)
    assert.equal(h.latest.overview.snapshotId, `${a.id}-s1`)
    h.controller.setVisible(true)
    await until(() => calls > previous)
  } finally { h.controller.stop() }
})

test('session links omit project, source, search, and reveal state while preserving opaque selection', () => {
  const url = new URL('http://127.0.0.1/?project=/private&source=/private&q=SECRET&body=SECRET#revealed')
  const state = { snapshot: 'opaque-snapshot', node: 'opaque-node', filter: 'generation', view: 'list', tab: 'metadata', theme: 'dark' }
  const link = viewUrl(state, url, a.id)
  assert.deepEqual([...link.searchParams.keys()], ['session', 'snapshot', 'node', 'filter', 'view', 'tab', 'theme'])
  assert.equal(link.searchParams.get('session'), a.id)
  assert.deepEqual(readViewState(link), state)
  assert.equal(link.href.includes('private'), false); assert.equal(link.href.includes('SECRET'), false); assert.equal(link.hash, '')
})

test('late initial overview cannot escape a session switch even when transport ignores abort', async () => {
  const pending = deferred(); const frames = []; let oldOverviewStarted = false
  const api = { status: async () => ({ catalogRevision: '1' }), catalog: async () => catalog(), workspace: async (id) => workspace(id), retry: async (id) => workspace(id) }
  const controller = new SystemController({ api, intervalMs: 100000, investigationApi: (id) => ({ overview: (snapshotId) => { if (id === a.id) { oldOverviewStarted = true; return pending.promise } return Promise.resolve({ snapshotId }) } }), onchange: (frame) => frames.push(structuredClone(frame)) })
  controller.start(); controller.select(a)
  try {
    await until(() => oldOverviewStarted)
    controller.select(b)
    await until(() => frames.at(-1)?.overview)
    pending.resolve({ snapshotId: `${a.id}-s1` }); await delay(5)
    assert.equal(frames.at(-1).selectedId, b.id)
    assert.equal(frames.at(-1).overview.snapshotId, `${b.id}-s1`)
  } finally { controller.stop() }
})

test('expired catalog cursor restarts the current revision without mixing retained pages', async () => {
  const { ApiError } = await import('../web/src/lib/trace-api.ts')
  let calls = 0
  const h = harness({ catalog: async (query) => {
    calls++
    if (query.has('cursor')) throw new ApiError(410, { code: 'expired_catalog_revision', message: 'Retired revision.', source: null })
    return calls < 3 ? catalog([a], '1', '1:1:fingerprint') : catalog([b], '2')
  } })
  h.controller.start()
  try {
    await until(() => h.latest?.catalog)
    h.controller.more()
    await until(() => h.latest?.catalog?.revision === '2')
    assert.deepEqual(h.latest.catalog.items.map((item) => item.id), [b.id])
    assert.equal(h.latest.catalogError, null)
  } finally { h.controller.stop() }
})

test('a workspace without retained evidence invalidates old investigation references explicitly', async () => {
  let retained = true
  const h = harness({ workspace: async (id) => retained ? workspace(id) : { ...workspace(id), state: 'failed', runId: null, snapshotId: null, error: { code: 'source_unavailable', message: 'No retained snapshot.', source: null } } })
  h.controller.start(); h.controller.select(a)
  try {
    await until(() => h.latest?.overview)
    retained = false; h.controller.resync()
    await until(() => h.latest?.workspace?.state === 'failed')
    assert.equal(h.latest.overview, null)
    assert.equal(h.latest.initialRevision, null)
    assert.equal(h.latest.selectedId, a.id)
    assert.equal(h.latest.workspace.error.code, 'source_unavailable')
  } finally { h.controller.stop() }
})

test('retry retains valid stale evidence while recovery has no structural error', async () => {
  let recovering = false
  const recovered = (id) => recovering ? { ...workspace(id), state: 'stale', progress: 'catchingUp', error: null } : workspace(id)
  const h = harness({ workspace: async (id) => recovered(id), retry: async (id) => { recovering = true; return recovered(id) } })
  h.controller.start(); h.controller.select(a)
  try {
    await until(() => h.latest?.overview)
    const snapshot = h.latest.overview.snapshotId
    await h.controller.retry()
    await until(() => h.latest?.workspace?.state === 'stale')
    assert.equal(h.latest.workspace.error, null)
    assert.equal(h.latest.overview.snapshotId, snapshot)
    assert.equal(h.latest.sessionPresence, 'available')
    assert.equal(h.latest.sessionError, null)
    assert.equal(h.requests.length, 1)
  } finally { h.controller.stop() }
})

test('confirmed unknown session invalidates ready evidence while transient network failures retain it', async () => {
  const { ApiError } = await import('../web/src/lib/trace-api.ts')
  let failure = null
  const h = harness({ workspace: async (id) => { if (failure) throw failure; return workspace(id) } })
  h.controller.start(); h.controller.select(a)
  try {
    await until(() => h.latest?.overview)
    const snapshot = h.latest.overview.snapshotId
    failure = new TypeError('Synthetic connection interruption.'); h.controller.resync()
    await until(() => h.latest.connection.kind === 'reconnecting')
    assert.equal(h.latest.overview.snapshotId, snapshot)
    assert.equal(h.latest.workspace.sessionId, a.id)
    failure = new ApiError(404, { code: 'unknown_session', message: 'Synthetic session removed.', source: null }); h.controller.resync()
    await until(() => h.latest.sessionPresence === 'unavailable')
    assert.equal(h.latest.overview, null)
    assert.equal(h.latest.workspace, null)
    assert.equal(h.latest.initialRevision, null)
    assert.equal(h.latest.selectedId, a.id)
    assert.match(h.latest.sessionError, /unknown_session.*unavailable/)
    assert.equal(h.requests.length, 1)
  } finally { h.controller.stop() }
})

test('late old overview after unknown-session removal cannot revive evidence or replace recovered evidence', async () => {
  const { ApiError } = await import('../web/src/lib/trace-api.ts')
  const old = deferred(); const frames = []; const calls = []; const invalidations = []; let mode = 'old'; let signal
  const api = { status: async () => ({ catalogRevision: '1' }), catalog: async () => catalog(), workspace: async (id) => {
    if (mode === 'removed') throw new ApiError(404, { code: 'unknown_session', message: 'Synthetic session removed.', source: null })
    return workspace(id, mode === 'old' ? 'old-snapshot' : 'new-snapshot', mode === 'old' ? 'old-run' : 'new-run')
  }, retry: async (id) => workspace(id) }
  const controller = new SystemController({ api, intervalMs: 100000, oninvalidate: (id) => invalidations.push(id), investigationApi: () => ({ overview: (snapshotId, requestSignal) => { calls.push(snapshotId); if (snapshotId === 'old-snapshot') { signal = requestSignal; return old.promise } return Promise.resolve({ snapshotId }) }, content: () => assert.fail('Recovery must not reveal bodies') }), onchange: (frame) => frames.push(structuredClone(frame)) })
  controller.start(); controller.select(a)
  try {
    await until(() => calls.includes('old-snapshot'))
    mode = 'removed'; controller.resync()
    await until(() => frames.at(-1).sessionPresence === 'unavailable')
    assert.equal(signal.aborted, true)
    assert.deepEqual(invalidations, [a.id])
    assert.equal(frames.at(-1).workspace, null)
    assert.equal(frames.at(-1).overview, null)
    old.resolve({ snapshotId: 'old-snapshot' }); await delay(5)
    assert.equal(frames.at(-1).overview, null)
    mode = 'recovered'; controller.resync()
    await until(() => frames.at(-1).overview?.snapshotId === 'new-snapshot')
    assert.equal(frames.at(-1).sessionPresence, 'available')
    assert.equal(frames.at(-1).selectedId, a.id)
    assert.equal(frames.some((frame) => frame.overview?.snapshotId === 'old-snapshot'), false)
  } finally { controller.stop() }
})
