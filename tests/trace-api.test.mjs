import assert from 'node:assert/strict'
import { readFile, readdir } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { join } from 'node:path'
import test from 'node:test'
import {
  ContractError, decodeContent, decodeDetail, decodeDiagnostics, decodeError,
  decodeObservations, decodeOverview, decodeWorkspace, decodeSystem, decodeCatalog, decodeSessionWorkspace, decodeNativeError, decodeNativeSource, decodeNativeOverview, decodeNativeObservations, decodeNativeDetail, decodeNativeDiagnostics, decodeNativeUsage, sessionApi,
} from '../web/src/lib/trace-api.ts'
import { readViewState, viewUrl } from '../web/src/lib/view-state.ts'

const root = process.env.TRACE_CONTRACT_ROOT ?? fileURLToPath(new URL('../', import.meta.url))
const directory = join(root, 'crates/trace-core/tests/contracts')
const read = async (name) => JSON.parse(await readFile(join(directory, `${name}.json`), 'utf8'))

test('client accepts Rust-serialized empty importing ready and failed workspace contracts', async () => {
  const workspaceDirectory = join(root, 'crates/pi-trace/tests/contracts')
  const states = new Set()
  for (const file of await readdir(workspaceDirectory)) {
    if (!file.startsWith('workspace-') || !file.endsWith('.json')) continue
    const value = JSON.parse(await readFile(join(workspaceDirectory, file), 'utf8'))
    assert.deepEqual(decodeWorkspace(value), value, file)
    states.add(value.state)
  }
  assert.deepEqual(states, new Set(['empty', 'importing', 'ready', 'failed']))
})

function decoderFor(value) {
  if ('state' in value) return decodeWorkspace
  if ('error' in value) return decodeError
  if ('observation' in value) return decodeDetail
  if ('input' in value) return decodeContent
  if ('totalMatches' in value) return decodeObservations
  if ('items' in value) return decodeDiagnostics
  if ('sourceCount' in value) return decodeOverview
  assert.fail('Unrecognized Rust contract fixture')
}

test('client accepts actual Rust-serialized application views without changing values', async () => {
  const files = (await readdir(directory)).filter((name) => name.endsWith('.json') && !name.startsWith('native-'))
  assert.ok(files.length >= 8, 'all core response families are serialized by Rust')
  for (const file of files) {
    const raw = await readFile(join(directory, file), 'utf8')
    const value = JSON.parse(raw)
    assert.ok(Buffer.byteLength(raw) <= 1024 * 1024, `${file} fits the response boundary`)
    assert.deepEqual(decoderFor(value)(value), value, file)
    if (!('input' in value)) {
      assert.equal(raw.includes('FABRICATED_PRIVATE_PROMPT'), false, `${file} omits bodies`)
      assert.equal(raw.includes('<script>'), false, `${file} omits recorded markup`)
    }
  }
})

test('client preserves nullable provenance, usage, sources and snapshot-scoped identifiers', async () => {
  const overview = decodeOverview(await read('overview'))
  const detail = decodeDetail(await read('detail'))
  const rows = decodeObservations(await read('observations'))
  const unavailable = decodeContent(await read('content-unavailable'))
  assert.equal(detail.snapshotId, overview.snapshotId)
  assert.ok(rows.items.some((item) => item.id === detail.observation.id))
  assert.equal(detail.ttft.state, 'unknown')
  assert.equal(detail.ttft.valueMs, null)
  assert.equal(detail.usage.cost, null)
  assert.equal(detail.usage.cacheWriteTokens, 0)
  assert.ok(detail.sources.every((source) => source.line >= 1 && source.file.length > 0))
  assert.equal(unavailable.input.state, 'unavailable')
  assert.equal(unavailable.input.text, null)
  assert.equal(unavailable.output.text, null)
  assert.equal(decodeError(await read('error')).error.source, null)
})

test('incompatible enums, absent required nullable fields and invented measurements fail visibly', async () => {
  const detail = await read('detail')
  for (const mutate of [
    (value) => { value.observation.kind = 'span' },
    (value) => { value.observation.outcome = 'recovered' },
    (value) => { value.relationship.state = 'guessed' },
    (value) => { value.usage.state = 'estimated' },
    (value) => { value.usage.fallbackSource = 'inferred' },
    (value) => { delete value.metadata.model },
    (value) => { delete value.usage.cost },
    (value) => { delete value.usage.cacheReadTokens },
    (value) => { value.ttft.valueMs = 0 },
    (value) => { value.requestToHeaders.valueMs = -1 },
    (value) => { value.observation.duration.state = 'exact' },
    (value) => { value.observation.id = '' },
    (value) => { value.sources[0].line = -1 },
  ]) {
    const incompatible = structuredClone(detail)
    mutate(incompatible)
    assert.throws(() => decodeDetail(incompatible), ContractError)
  }
})

test('Rust continuation preserves ancestor pages and opaque cursor identity', async () => {
  const first = decodeObservations(await read('observations-page'))
  const all = decodeObservations(await read('observations'))
  assert.ok(first.nextCursor)
  assert.equal(first.snapshotId, all.snapshotId)
  assert.equal(first.totalRows, all.totalRows)
  assert.ok(first.items.length < first.totalRows)
  assert.deepEqual(first.items, all.items.slice(0, first.items.length))
  const invalid = structuredClone(first)
  invalid.nextCursor = 'x'.repeat(2049)
  assert.throws(() => decodeObservations(invalid), ContractError)
})

test('preview boundary rejects excess UTF-8 bytes and unavailable text', async () => {
  const content = await read('content')
  assert.match(decodeContent(content).input.text, /<script>/)
  for (const mutate of [
    (value) => { value.input.text = 'é'.repeat(4097) },
    (value) => { value.input.limitBytes = 16384 },
    (value) => { value.input.state = 'unavailable' },
    (value) => { value.output.text = null },
  ]) {
    const incompatible = structuredClone(content)
    mutate(incompatible)
    assert.throws(() => decodeContent(incompatible), ContractError)
  }
  const oversizedLabel = await read('detail')
  oversizedLabel.observation.label = 'é'.repeat(257)
  assert.throws(() => decodeDetail(oversizedLabel), ContractError)
})

test('view links allow only snapshot selection and safe presentation fields', () => {
  const source = new URL('http://127.0.0.1:4317/?snapshot=s1&node=n4&filter=error&view=list&tab=metadata&theme=dark&q=private&source=/private/trace#revealed')
  const view = readViewState(source)
  const link = viewUrl(view, source)
  assert.deepEqual([...link.searchParams.keys()], ['snapshot', 'node', 'filter', 'view', 'tab', 'theme'])
  assert.equal(link.hash, '')
  assert.equal(link.href.includes('private'), false)
  assert.deepEqual(readViewState(link), view)
  const unknown = readViewState(new URL('http://localhost/?node=n4&filter=estimated&view=other&tab=other&theme=other'))
  assert.deepEqual(unknown, { snapshot: null, node: null, filter: '', view: 'waterfall', tab: 'details', theme: 'dark' })
})

test('new workspaces default to dark and explicit light survives a view link', () => {
  assert.equal(readViewState(new URL('http://localhost/')).theme, 'dark')
  const lightUrl = new URL('http://localhost/?theme=light')
  const light = readViewState(lightUrl)
  assert.equal(light.theme, 'light')
  assert.equal(readViewState(viewUrl(light, lightUrl)).theme, 'light')
})

test('recorded original lengths remain lossless above JavaScript safe integers', async () => {
  const detail = decodeDetail(await read('detail-inspection-tool-huge-length'))
  assert.equal(detail.toolInspection.result.originalLength.value, '18446744073709551615')
  for (const value of [9007199254740992, '18446744073709551616', '01', '-1', '1.0']) {
    const invalid = await read('detail-inspection-tool-huge-length')
    invalid.toolInspection.result.originalLength.value = value
    assert.throws(() => decodeDetail(invalid), ContractError)
  }
})

test('inspection variants retain unknown evidence and distinct size units', async () => {
  const provider = decodeDetail(await read('detail-inspection-provider'))
  assert.equal(provider.context.shape, 'providerInput')
  assert.equal(provider.context.counts.toolDefinitions, 45)
  assert.equal(provider.context.counts.skillDescriptors, 167)
  assert.equal(provider.context.largestTools.length, 16)
  assert.equal(provider.context.omitted.tools, 29)
  assert.ok(provider.context.categories.some((category) => category.kind === 'opaqueReasoning' && category.size.textChars === null))
  for (const name of ['missing', 'null', 'unsupported', 'plain']) assert.equal(decodeDetail(await read(`detail-inspection-${name}`)).context.counts, null)
  const tool = decodeDetail(await read('detail-inspection-tool'))
  assert.equal(tool.toolInspection.callIdTruncated, true)
  assert.equal(Buffer.byteLength(tool.toolInspection.callId), 512)
  assert.equal(tool.toolInspection.result.originalLength.unit, 'utf16CodeUnitsOfSerializedResult')
  assert.equal(tool.toolInspection.result.sourceTruncation, 'knownTruncated')
  assert.equal(tool.toolInspection.result.localTruncated, true)
  assert.ok(tool.toolInspection.result.selectedSize.compactJsonBytes > tool.toolInspection.result.previewUtf8Bytes)
  assert.equal(decodeDetail(await read('detail-inspection-tool-foreign')).toolInspection.result.originalLength.unit, 'unspecified')
  assert.equal(decodeDetail(await read('detail-inspection-tool-unknown')).toolInspection.result.sourceTruncation, 'unknown')
  assert.equal(decodeDetail(await read('detail-inspection-tool-unavailable')).toolInspection.result.availability, 'unavailable')
  const limited = decodeDetail(await read('detail-inspection-retention-limited')).context
  assert.equal(limited.retention, 'retentionLimited'); assert.equal(limited.coverage, 'complete')
  assert.equal(limited.payloadSize, null); assert.equal(limited.counts, null); assert.equal(limited.omitted.items, null)
  assert.equal(decodeDetail(await read('detail-inspection-tool-retention-limited')).toolInspection.retention, 'retentionLimited')
})

test('inspection decoders reject invalid variants, unbounded lists and names, and unavailable evidence', async () => {
  const provider = await read('detail-inspection-provider')
  for (const mutate of [
    (value) => { delete value.context },
    (value) => { delete value.toolInspection },
    (value) => { value.context = null },
    (value) => { value.observation.kind = 'session' },
    (value) => { value.context.coverage = 'estimated' },
    (value) => { value.context.retention = 'silent' },
    (value) => { value.context.shape = 'inferred' },
    (value) => { value.context.requestField = 'http' },
    (value) => { value.context.payloadSize.compactJsonBytes = -1 },
    (value) => { value.context.counts.skillDescriptors = -1 },
    (value) => { value.context.largestItems.push(value.context.largestItems[0]) },
    (value) => { value.context.largestTools.push(value.context.largestTools[0]) },
    (value) => { value.context.largestSkills.push(value.context.largestSkills[0]) },
    (value) => { value.context.largestTools[0].name = '😀'.repeat(129) },
    (value) => { value.context.sections = Array(33).fill(value.context.sections[0]) },
    (value) => { value.context.sections[0].kind = 'private-tag' },
    (value) => { value.context.categories.push(value.context.categories[0]) },
    (value) => { value.context.retention = 'retentionLimited' },
    (value) => { value.context.contextInputTokens = -1 },
  ]) {
    const value = structuredClone(provider); mutate(value)
    assert.throws(() => decodeDetail(value), ContractError)
  }
  const tool = await read('detail-inspection-tool')
  for (const mutate of [
    (value) => { value.toolInspection.retention = 'listsLimited' },
    (value) => { value.toolInspection = null },
    (value) => { value.observation.kind = 'generation' },
    (value) => { value.toolInspection.callId = 'é'.repeat(257) },
    (value) => { value.toolInspection.result.recordedField = 'modelOutput' },
    (value) => { value.toolInspection.result.originalLength.unit = 'tokens' },
    (value) => { value.toolInspection.result.sourceTruncation = false },
    (value) => { value.toolInspection.arguments.previewUtf8Bytes = 8193 },
    (value) => { value.toolInspection.arguments.availability = 'unavailable' },
  ]) {
    const value = structuredClone(tool); mutate(value)
    assert.throws(() => decodeDetail(value), ContractError)
  }
})

test('generation and Context view links round trip without body state', () => {
  const state = readViewState(new URL('http://localhost/?snapshot=s1&node=n4&filter=generation&tab=context'))
  assert.equal(state.filter, 'generation'); assert.equal(state.tab, 'context')
  const url = viewUrl(state, new URL('http://localhost/?body=PRIVATE#revealed'))
  assert.deepEqual(readViewState(url), state)
  assert.equal(url.href.includes('PRIVATE'), false)
})

test('exact token counters retain u64 values from Rust contracts', async () => {
  for (const [file, expected] of [['counter-7', '9007199254740993'], ['counter-8', '18446744073709551615']]) {
    const detail = decodeDetail(await read(`detail-accounting-${file}`))
    assert.equal(detail.spanUsage.own.totalTokens.value, expected)
    assert.equal(detail.observation.spanUsage.inclusive.totalTokens.value, expected)
    assert.equal(detail.spanUsage.evidence[0].reportedTotal.value, expected)
  }
})

test('token decoder rejects invalid states, noncanonical integers and evidence budget violations', async () => {
  const detail = await read('detail-accounting-counter-7')
  for (const counter of [
    { state: 'exact', value: 123 }, { state: 'exact', value: '01' }, { state: 'exact', value: '-1' },
    { state: 'exact', value: '18446744073709551616' }, { state: 'exact', value: null },
    { state: 'unknown', value: '0' }, { state: 'invalid', value: '1' }, { state: 'estimated', value: '1' },
    { state: 'partial' },
  ]) {
    const incompatible = structuredClone(detail)
    incompatible.spanUsage.own.totalTokens = counter
    incompatible.observation.spanUsage.own.totalTokens = counter
    assert.throws(() => decodeDetail(incompatible), ContractError)
  }
  const cap = await read('detail-accounting-evidence-cap')
  assert.equal(decodeDetail(cap).spanUsage.omittedCount, 4)
  for (const mutate of [
    (value) => { value.spanUsage.evidence.push(value.spanUsage.evidence[0]) },
    (value) => { value.spanUsage.omittedCount = 0 },
    (value) => { value.spanUsage.own.provenance = 'estimated' },
    (value) => { delete value.spanUsage.own.totalTokens.value },
  ]) {
    const incompatible = structuredClone(cap)
    mutate(incompatible)
    assert.throws(() => decodeDetail(incompatible), ContractError)
  }
})

test('live workspace metadata rejects absent nulls, impossible states and invalid activity', async () => {
  const file = join(root, 'crates/pi-trace/tests/contracts/workspace-live-ready.json')
  const ready = JSON.parse(await readFile(file, 'utf8'))
  assert.equal(decodeWorkspace(ready).live.state, 'waiting')
  for (const mutate of [
    (value) => { delete value.live.error },
    (value) => { delete value.live.activity },
    (value) => { value.live.state = 'recording' },
    (value) => { value.live.revision = null },
    (value) => { value.live.revision = -1 },
    (value) => { value.live.error = { code: 'error', message: 'structural', source: null } },
    (value) => { value.live.activity.pendingBytes = value.live.activity.readBytes + 1 },
  ]) {
    const incompatible = structuredClone(ready)
    mutate(incompatible)
    assert.throws(() => decodeWorkspace(incompatible), ContractError)
  }
})

test('scoped span transport rejects snapshot and selection substitution and oversized responses', async (t) => {
  const { traceApi } = await import('../web/src/lib/trace-api.ts')
  const detail = await read('detail')
  let body = JSON.stringify(detail)
  t.mock.method(globalThis, 'fetch', async () => new Response(body, { status: 200 }))
  const signal = new AbortController().signal
  await assert.rejects(traceApi.spanDetail('different-snapshot', detail.observation.spanId, signal), ContractError)
  await assert.rejects(traceApi.spanDetail(detail.snapshotId, 'different-span', signal), ContractError)
  const accepted = await traceApi.spanDetail(detail.snapshotId, detail.observation.spanId, signal)
  assert.equal(accepted.observation.spanId, detail.observation.spanId)
  body = ' '.repeat(1024 * 1024 + 1)
  await assert.rejects(traceApi.spanDetail(detail.snapshotId, detail.observation.spanId, signal), ContractError)
})


const systemRoot = process.env.SYSTEM_CONTRACT_ROOT ?? root
const readSystem = async (name) => JSON.parse(await readFile(join(systemRoot, 'tests/contracts', `system-${name}.json`), 'utf8'))
test('system decoders preserve actual Rust HTTP status, catalog, workspace and native snapshot contracts', async () => {
  for (const [name, decode] of [['status', decodeSystem], ['catalog', decodeCatalog], ['workspace-ready', decodeSessionWorkspace], ['workspace-loading', decodeSessionWorkspace], ['workspace-stale', decodeSessionWorkspace], ['workspace-recovering', decodeSessionWorkspace], ['workspace-failed', decodeSessionWorkspace], ['node', decodeNativeDetail], ['span', decodeNativeDetail], ['content', decodeContent], ['overview', decodeNativeOverview], ['observations', decodeNativeObservations], ['diagnostics', decodeNativeDiagnostics], ['error', decodeNativeError]]) {
    const value = await readSystem(name)
    assert.deepEqual(decode(value), value, name)
  }
  assert.deepEqual(decodeWorkspace({ state: 'system' }), { state: 'system' })
})

test('system contracts reject absent nulls, malformed provenance, unbounded pages and namespace substitution', async () => {
  const catalog = await readSystem('catalog')
  for (const mutate of [
    (value) => { delete value.coverage.omittedCount },
    (value) => { delete value.items[0].project },
    (value) => { delete value.items[0].activityMs },
    (value) => { delete value.items[0].source.byteRange },
    (value) => { value.items[0].source.line = '0' },
    (value) => { value.items[0].source.line = 1 },
    (value) => { value.items[0].source.byteRange = { start: '2', end: '1' } },
    (value) => { value.items[0].source.kind = 'file' },
    (value) => { value.items[0].source.provider = 'invented' },
    (value) => { value.items[0].source.storeId = 'other-store' },
    (value) => { value.items[0].provider = 'codex' },
    (value) => { value.items[0].id = '../source.jsonl' },
    (value) => { value.items[0].processState = 'running' },
    (value) => { value.items[0].capabilities.persistence = 'streaming' },
    (value) => { value.items[0].capabilities.timing = 'estimated' },
    (value) => { value.items = Array(101).fill(value.items[0]) },
    (value) => { value.nextCursor = 'obsolete-revision:100' },
  ]) { const value = structuredClone(catalog); mutate(value); assert.throws(() => decodeCatalog(value), ContractError) }
  const workspace = await readSystem('workspace-ready')
  for (const mutate of [
    (value) => { delete value.error }, (value) => { delete value.runId }, (value) => { delete value.snapshotId },
    (value) => { value.runId = null }, (value) => { value.state = 'failed' },
    (value) => { value.state = 'unsupported' }, (value) => { value.processState = 'complete' },
  ]) { const value = structuredClone(workspace); mutate(value); assert.throws(() => decodeSessionWorkspace(value), ContractError) }
  const status = await readSystem('status')
  for (const mutate of [(value) => { delete value.disabledProviders }, (value) => { delete value.stores[0].error }, (value) => { delete value.stores[0].root }, (value) => { value.stores[0].state = 'running' }, (value) => { value.stores = Array(17).fill(value.stores[0]) }]) {
    const value = structuredClone(status); mutate(value); assert.throws(() => decodeSystem(value), ContractError)
  }
})

test('native session transport rejects provider, store, snapshot and node substitution', async (t) => {
  const catalog = await readSystem('catalog')
  const descriptor = catalog.items[0]
  const overview = await readSystem('overview')
  let value = overview
  const paths = []
  t.mock.method(globalThis, 'fetch', async (path) => { paths.push(path); return new Response(JSON.stringify(value), { status: 200 }) })
  const api = sessionApi(descriptor.id, descriptor)
  const signal = new AbortController().signal
  assert.deepEqual(await api.overview(overview.snapshotId, signal), overview)
  assert.ok(paths[0].startsWith(`/api/v1/sessions/${descriptor.id}/snapshots/`))
  value = structuredClone(overview); value.source.provider = 'codex'
  await assert.rejects(api.overview(overview.snapshotId, signal), ContractError)
  value = structuredClone(overview); value.source.storeId = '0000000000000000'
  await assert.rejects(api.overview(overview.snapshotId, signal), ContractError)
  value = overview
  await assert.rejects(api.overview('other-snapshot', signal), ContractError)
  assert.throws(() => sessionApi('../private'), ContractError)
})


test('native detail requires explicit evidence and preserves exact native counter strings without client arithmetic', async () => {
  const detail = await readSystem('node')
  const decoded = decodeNativeDetail(detail)
  assert.ok(decoded.nativeUsage.length)
  assert.equal(decoded.context, null)
  assert.ok(decoded.nativeUsage.some((evidence) => evidence.accepted))
  for (const field of ['nativeUsage', 'relationships', 'timingEvidence', 'selectedContext']) {
    const value = structuredClone(detail); delete value[field]
    assert.throws(() => decodeNativeDetail(value), ContractError)
  }
  for (const mutate of [
    (value) => { value.nativeUsage = Array(17).fill(value.nativeUsage[0]) },
    (value) => { value.nativeUsage[0].role.kind = 'estimated' },
    (value) => { value.nativeUsage[0].source.provider = 'codex' },
    (value) => { value.nativeUsage[0].nativeCounters[0].source.storeId = 'another-store' },
    (value) => { value.nativeUsage[0].nativeCounters[0].value = { state: 'exact', value: 9007199254740992 } },
    (value) => { value.nativeUsage[0].nativeCounters[0].value = { state: 'exact', value: '18446744073709551616' } },
    (value) => { value.timingEvidence = [{ meaning: 'inferredDuration', value: '1', unit: 'ms', source: value.observation.source }] },
  ]) { const value = structuredClone(detail); mutate(value); assert.throws(() => decodeNativeDetail(value), ContractError) }
  const huge = structuredClone(detail)
  huge.nativeUsage[0].nativeCounters[0].value = { state: 'exact', value: '18446744073709551615' }
  assert.equal(decodeNativeDetail(huge).nativeUsage[0].nativeCounters[0].value.value, '18446744073709551615')
})

test('native session transport requests bodies only through deliberate content calls', async (t) => {
  const catalog = await readSystem('catalog'); const item = catalog.items[0]
  const detail = await readSystem('node'); const content = await readSystem('content')
  const paths = []
  t.mock.method(globalThis, 'fetch', async (path) => { paths.push(path); return new Response(JSON.stringify(path.endsWith('/content') ? content : detail), { status: 200 }) })
  const api = sessionApi(item.id, item); const signal = new AbortController().signal
  await api.detail(detail.snapshotId, detail.observation.id, signal)
  assert.equal(paths.some((path) => path.endsWith('/content')), false)
  await api.content(content.snapshotId, content.nodeId, signal)
  assert.equal(paths.filter((path) => path.endsWith('/content')).length, 1)
  await assert.rejects(api.detail(detail.snapshotId, 'another-node', signal), ContractError)
})

const nativeRoot = process.env.NATIVE_CONTRACT_ROOT ?? root
const readNative = async (name) => JSON.parse(await readFile(join(nativeRoot, 'crates/trace-core/tests/contracts', `${name}.json`), 'utf8'))
test('native provider goldens retain actual Rust projections, normalization operands and SQLite provenance', async () => {
  for (const [name, decode] of [['native-pi-overview', decodeNativeOverview], ['native-pi-detail', decodeNativeDetail], ['native-sqlite-detail', decodeNativeDetail]]) {
    const value = await readNative(name)
    assert.deepEqual(decode(value), value, name)
  }
  const sqlite = decodeNativeDetail(await readNative('native-sqlite-detail'))
  assert.equal(sqlite.observation.source.kind, 'sqlite')
  assert.ok(sqlite.nativeUsage.some((evidence) => evidence.normalization.some((rule) => rule.rule === 'addDisjointReasoning')))
  assert.equal('line' in sqlite.observation.source, false)
  const pi = decodeNativeDetail(await readNative('native-pi-detail'))
  assert.equal(pi.spanUsage.own.observedGenerations, pi.spanUsage.own.totalGenerations)
  assert.ok(pi.timingEvidence.some((timing) => timing.meaning === 'requestStart'))
})


test('typed provenance variants preserve Rust JSON, compressed offsets and SQLite keys without fabricated lines', async () => {
  const values = await readNative('native-provenance-variants')
  const { sourceLabel } = await import('../web/src/lib/presentation.ts')
  assert.equal(values.length, 9)
  for (const value of values) assert.deepEqual(decodeNativeSource(value), value)
  assert.deepEqual(new Set(values.map((value) => value.kind)), new Set(['jsonl', 'compressedJsonl', 'json', 'sqlite']))
  const compressed = values.find((value) => value.kind === 'compressedJsonl' && value.decodedByteRange)
  assert.equal(compressed.decodedByteRange.start, '9007199254740993')
  assert.match(sourceLabel(decodeNativeSource(compressed)), /zstd decoded line 3/)
  const document = values.find((value) => value.kind === 'json' && value.pointer)
  assert.match(sourceLabel(decodeNativeSource(document)), /JSON \/toolUseId/)
  assert.equal('line' in decodeNativeSource(document), false)
  const sqlite = values.find((value) => value.kind === 'sqlite' && value.field?.kind === 'column')
  assert.match(sourceLabel(decodeNativeSource(sqlite)), /ordinal=9223372036854775807/)
  assert.match(sourceLabel(decodeNativeSource(sqlite)), /digest=blob\(00ff\)/)
  assert.equal('line' in decodeNativeSource(sqlite), false)
  for (const mutate of [
    (value) => { delete value.field }, (value) => { delete value.observedRevision },
    (value) => { value.primaryKey[0].value.value = '9223372036854775808' },
    (value) => { value.primaryKey[0].value.value = '01' },
    (value) => { value.primaryKey[1].value.hex = '0fg' },
    (value) => { value.primaryKey = Array(33).fill(value.primaryKey[0]) },
    (value) => { value.field.kind = 'guessed' },
    (value) => { value.field = { kind: 'json', column: 'data', pointer: '/bad~2escape' } },
    (value) => { value.field = { kind: 'json', column: 'data', pointer: '/bad~' } },
  ]) { const value = structuredClone(sqlite); mutate(value); assert.throws(() => decodeNativeSource(value), ContractError) }
  for (const mutate of [(value) => { delete value.pointer }, (value) => { value.pointer = 'not/a/pointer' }, (value) => { value.pointer = '/invalid~escape' }]) {
    const value = structuredClone(document); mutate(value); assert.throws(() => decodeNativeSource(value), ContractError)
  }
})

test('native usage variants retain every Rust role, exact counter state and normalization rule', async () => {
  const values = await readNative('native-usage-variants')
  for (const value of values) assert.deepEqual(decodeNativeUsage(value), value)
  assert.deepEqual(new Set(values.map((value) => value.role.kind)), new Set(['contribution', 'reconciliation', 'inherited', 'breakdown']))
  const states = new Set(values.flatMap((value) => value.nativeCounters.map((counter) => counter.value.state)))
  assert.deepEqual(states, new Set(['exact', 'partial', 'unknown', 'invalid', 'notApplicable']))
  const rules = new Set(values.flatMap((value) => value.normalization.map((rule) => rule.rule)))
  assert.ok(rules.has('addDisjointReasoning')); assert.ok(rules.has('subtractCachedInputSubset'))
  assert.ok(values.some((value) => value.nativeCounters.some((counter) => counter.value.value === '9007199254740993')))
  for (const mutate of [
    (value) => { delete value.role.category }, (value) => { delete value.accepted },
    (value) => { delete value.nativeCounters[0].value.value },
    (value) => { value.nativeCounters = Array(65).fill(value.nativeCounters[0]) },
    (value) => { value.normalization = Array(65).fill(value.normalization[0]) },
    (value) => { value.normalization[0].operands = Array(65).fill(value.normalization[0].operands[0]) },
  ]) { const value = structuredClone(values[0]); mutate(value); assert.throws(() => decodeNativeUsage(value), ContractError) }
})


test('store errors stay bound to the declared provider and store namespace', async () => {
  const status = await readSystem('status')
  const catalog = await readSystem('catalog')
  const source = catalog.items[0].source
  status.stores[0].error = { code: 'synthetic_error', message: 'Synthetic store error.', source: { ...source, provider: status.stores[0].provider, storeId: status.stores[0].id } }
  assert.deepEqual(decodeSystem(status), status)
  for (const mutate of [(value) => { value.stores[0].error.source.provider = 'codex' }, (value) => { value.stores[0].error.source.storeId = 'foreign-store' }]) {
    const value = structuredClone(status); mutate(value); assert.throws(() => decodeSystem(value), ContractError)
  }
})


test('actual Rust recovery workspace preserves retained snapshot with stale state and no error', async () => {
  const value = await readSystem('workspace-recovering')
  assert.equal(value.state, 'stale')
  assert.equal(value.error, null)
  assert.ok(value.snapshotId)
  assert.deepEqual(decodeSessionWorkspace(value), value)
})

test('actual native auxiliary contributions do not invent observed generations or collector fallbacks', async () => {
  const overview = await readNative('native-auxiliary-overview')
  const detail = await readNative('native-auxiliary-detail')
  assert.deepEqual(decodeNativeOverview(overview), overview)
  assert.deepEqual(decodeNativeDetail(detail), detail)
  assert.equal(overview.tokenAccounting.contributorCount, 3)
  assert.equal(overview.tokenAccounting.totalTokens.value, '30')
  assert.equal(detail.selectedContext, 'b')
  assert.deepEqual(new Set(detail.nativeUsage.filter((evidence) => evidence.accepted).map((evidence) => evidence.role.category)), new Set(['usage', 'compaction', 'branch_summary']))
  assert.equal(overview.tokenAccounting.observedGenerations, 0)
  assert.equal(overview.tokenAccounting.totalGenerations, 0)
  assert.equal(overview.tokenAccounting.fallbackContributions, 0)
  assert.ok(detail.nativeUsage.some((evidence) => evidence.accepted && evidence.role.kind === 'contribution' && evidence.role.category !== 'modelResponse'))
  for (const mutate of [
    (value) => { value.tokenAccounting.observedGenerations = 1 },
    (value) => { value.tokenAccounting.observedGenerations = value.tokenAccounting.contributorCount + 1; value.tokenAccounting.totalGenerations = value.tokenAccounting.observedGenerations },
    (value) => { value.tokenAccounting.fallbackContributions = value.tokenAccounting.contributorCount + 1 },
  ]) { const value = structuredClone(overview); mutate(value); assert.throws(() => decodeNativeOverview(value), ContractError) }
  const collector = await read('overview')
  collector.tokenAccounting.contributorCount = collector.tokenAccounting.observedGenerations + collector.tokenAccounting.fallbackContributions + 1
  collector.tokenAccounting.provenance = 'aggregate'
  assert.throws(() => decodeOverview(collector), ContractError)
})

test('native requests serialize a tab and cancel queued work before it fetches', async (t) => {
  const value = await read('native-pi-overview')
  const started = []
  let release
  const held = new Promise((done) => { release = done })
  t.mock.method(globalThis, 'fetch', async (path) => {
    started.push(path)
    if (started.length === 1) await held
    return new Response(JSON.stringify(value), { status: 200 })
  })
  const api = sessionApi('synthetic-session')
  const queued = new AbortController()
  const first = api.overview(value.snapshotId, new AbortController().signal)
  const second = api.overview(value.snapshotId, queued.signal)
  void second.catch(() => {})
  const third = api.overview(value.snapshotId, new AbortController().signal)
  try {
    await new Promise((done) => setImmediate(done))
    assert.equal(started.length, 1, 'only one native request per tab reaches the service')
    queued.abort()
    await assert.rejects(second, (error) => error.name === 'AbortError')
    assert.equal(started.length, 1, 'cancelled queued work never fetches')
  } finally {
    queued.abort()
    release()
    await Promise.allSettled([first, second, third])
  }
  assert.equal(started.length, 2, 'remaining requests make progress')
})
