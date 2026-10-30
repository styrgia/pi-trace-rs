import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import { loadSnapshotView } from '../web/src/lib/snapshot-view.ts'
import { ContractError, decodeDetail, decodeObservations, decodeOverview, decodeDiagnostics } from '../web/src/lib/trace-api.ts'

const read = async (name) => JSON.parse(await readFile(new URL(`../crates/trace-core/tests/contracts/${name}.json`, import.meta.url), 'utf8'))
async function fixture() {
  const overview = decodeOverview(await read('overview'))
  const page = decodeObservations(await read('observations'))
  const detail = decodeDetail(await read('detail'))
  const diagnostics = decodeDiagnostics(await read('diagnostics'))
  const calls = [], signals = []
  const api = {
    overview: async (_snapshot, signal) => { signals.push(signal); return overview },
    observations: async (_snapshot, query, signal) => { calls.push(['rows', new URLSearchParams(query)]); signals.push(signal); return page },
    detail: async (_snapshot, node, signal) => { calls.push(['node', node]); signals.push(signal); return detail },
    spanDetail: async (_snapshot, span, signal) => { calls.push(['span', span]); signals.push(signal); return detail },
    diagnostics: async (_snapshot, cursor, signal) => { calls.push(['diagnostics', cursor]); signals.push(signal); return diagnostics },
    content: () => assert.fail('Metadata refresh must never reveal content'),
  }
  return { overview, page, detail, diagnostics, calls, signals, api, request: { snapshotId: overview.snapshotId, search: '', filter: '', retainedRows: 100, diagnosticsRows: 100, selection: { kind: 'none' }, signal: new AbortController().signal } }
}

test('coherent snapshot refresh resolves stable selection independently of filtered pages', async () => {
  const f = await fixture()
  f.api.observations = async (_snapshot, query) => {
    assert.equal(query.get('filter'), 'tool')
    assert.equal(query.get('cursor'), null)
    return { ...f.page, items: [], totalMatches: 0, totalRows: 0, nextCursor: null }
  }
  const frame = await loadSnapshotView({ ...f.request, filter: 'tool', selection: { kind: 'span', spanId: f.detail.observation.spanId } }, f.api)
  assert.equal(frame.detail.observation.spanId, f.detail.observation.spanId)
  assert.equal(frame.page.items.length, 0)
  assert.equal(frame.overview.snapshotId, frame.detail.snapshotId)
  assert.equal(frame.diagnostics.snapshotId, frame.overview.snapshotId)
  assert.ok(f.calls.some(([kind]) => kind === 'span'))
  assert.equal(f.calls.some(([kind]) => kind === 'node'), false)
})

test('snapshot mismatch and old node references are rejected without a replacement view', async () => {
  const f = await fixture()
  f.api.overview = async () => ({ ...f.overview, snapshotId: 'another-snapshot' })
  await assert.rejects(loadSnapshotView(f.request, f.api), ContractError)
  assert.ok(f.signals.every((signal) => signal.aborted))
  f.api.overview = async () => f.overview
  await assert.rejects(loadSnapshotView({ ...f.request, selection: { kind: 'node', nodeId: 'n4', snapshotId: 'old-snapshot' } }, f.api), ContractError)
})

test('new snapshot pages restart cursors and retain no more than 2000 rows', async () => {
  const f = await fixture()
  const cursors = []
  f.api.observations = async (_snapshot, query) => {
    const offset = query.has('cursor') ? Number(query.get('cursor').slice('new-'.length)) : 0
    cursors.push(query.get('cursor'))
    return { ...f.page, items: Array.from({ length: 100 }, (_, index) => ({ ...f.page.items[0], id: `n${offset + index + 1}`, spanId: `span-${offset + index + 1}`, parentId: null })), totalRows: 2100, totalMatches: 2100, nextCursor: `new-${offset + 100}` }
  }
  const frame = await loadSnapshotView({ ...f.request, retainedRows: 3000, diagnosticsRows: 0 }, f.api)
  assert.equal(cursors[0], null)
  assert.equal(cursors[1], 'new-100')
  assert.equal(frame.page.items.length, 2000)
  assert.equal(frame.retainedBound, true)
  assert.equal(frame.page.nextCursor, 'new-2000')
})

test('a byte-limited last page is retained whole before the row bound instead of silently dropping rows', async () => {
  const f = await fixture()
  let request = 0
  f.api.observations = async () => {
    const offset = request++ * 99
    return { ...f.page, items: Array.from({ length: 99 }, (_, index) => ({ ...f.page.items[0], id: `n${offset + index + 1}`, spanId: `span-${offset + index + 1}` })), totalRows: 2079, totalMatches: 2079, nextCursor: request === 21 ? null : `new-${request}` }
  }
  const frame = await loadSnapshotView({ ...f.request, retainedRows: 2000, diagnosticsRows: 0 }, f.api)
  assert.equal(frame.page.items.length, 1980)
  assert.equal(frame.retainedBound, true)
  assert.equal(frame.page.nextCursor, 'new-20')
})

test('superseded controlled responses cannot yield a replacement snapshot', async () => {
  const f = await fixture()
  let resolveOverview
  f.api.overview = () => new Promise((resolve) => { resolveOverview = resolve })
  const controller = new AbortController()
  const loading = loadSnapshotView({ ...f.request, signal: controller.signal }, f.api)
  controller.abort()
  resolveOverview(f.overview)
  await assert.rejects(loading, { name: 'AbortError' })
})
