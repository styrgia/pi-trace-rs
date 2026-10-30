type Decoder<T> = (value: unknown) => T
type Decoded<D> = D extends Decoder<infer T> ? T : never

export class ContractError extends Error {
  constructor() { super('The local service returned an incompatible API response.'); this.name = 'ContractError' }
}
function invalid(): never { throw new ContractError() }
const string: Decoder<string> = (value) => typeof value === 'string' ? value : invalid()
const boolean: Decoder<boolean> = (value) => typeof value === 'boolean' ? value : invalid()
const number: Decoder<number> = (value) => typeof value === 'number' && Number.isFinite(value) ? value : invalid()
const count: Decoder<number> = (value) => { const n = number(value); return Number.isSafeInteger(n) && n >= 0 ? n : invalid() }
function nullable<T>(decode: Decoder<T>): Decoder<T | null> { return (value) => value === null ? null : decode(value) }
function array<T>(decode: Decoder<T>): Decoder<T[]> { return (value) => Array.isArray(value) ? value.map(decode) : invalid() }
function enumeration<const T extends readonly string[]>(...values: T): Decoder<T[number]> {
  return (value) => { for (const item of values) if (item === value) return item; return invalid() }
}
function object<S extends Record<string, Decoder<unknown>>>(shape: S): Decoder<{ [K in keyof S]: Decoded<S[K]> }> {
  return (value) => {
    if (typeof value !== 'object' || value === null || Array.isArray(value)) return invalid()
    const decoded: Record<string, unknown> = {}
    for (const key in shape) {
      const decoder = shape[key]
      if (!decoder) return invalid()
      decoded[key] = decoder(Reflect.get(value, key))
    }
    return decoded as { [K in keyof S]: Decoded<S[K]> }
  }
}
function boundedArray<T>(decode: Decoder<T>, max: number): Decoder<T[]> {
  return (value) => Array.isArray(value) && value.length <= max ? value.map(decode) : invalid()
}
const boundedString = (maxBytes: number): Decoder<string> => (value) => {
  const result = string(value)
  return new TextEncoder().encode(result).length <= maxBytes ? result : invalid()
}
const id: Decoder<string> = (value) => { const result = boundedString(2048)(value); return result.length > 0 ? result : invalid() }
const collectorSource = object({ file: string, line: count })
const provider = enumeration('codex', 'pi', 'claude', 'opencode')
export type Provider = Decoded<typeof provider>
const opaqueId: Decoder<string> = (value) => { const text = string(value); return /^[A-Za-z0-9_-]{1,2048}$/.test(text) ? text : invalid() }
const positiveDecimal: Decoder<string> = (value) => { const result = decimalU64(value); return result !== '0' ? result : invalid() }
const byteRangeShape = object({ start: (value: unknown) => decimalU64(value), end: (value: unknown) => decimalU64(value) })
const byteRange: Decoder<Decoded<typeof byteRangeShape>> = (value) => { const result = byteRangeShape(value); return BigInt(result.end) >= BigInt(result.start) ? result : invalid() }
const nativeSourceBase = { provider, storeId: opaqueId }
const rowKey: Decoder<{ kind: 'text'; value: string } | { kind: 'integer'; value: string } | { kind: 'blob'; hex: string }> = (value) => {
  const { kind } = object({ kind: enumeration('text', 'integer', 'blob') })(value)
  if (kind === 'text') return { kind, ...object({ value: boundedString(8192) })(value) }
  if (kind === 'blob') { const result = object({ hex: boundedString(16384) })(value); if (!/^(?:[a-fA-F0-9]{2})*$/.test(result.hex)) return invalid(); return { kind, ...result } }
  const result = object({ value: boundedString(20) })(value)
  if (!/^(0|-?[1-9][0-9]*)$/.test(result.value) || BigInt(result.value) < -9223372036854775808n || BigInt(result.value) > 9223372036854775807n) return invalid()
  return { kind, ...result }
}
const jsonPointer: Decoder<string> = (value) => { const result = boundedString(8192)(value); return (result === '' || result.startsWith('/')) && !/~(?:[^01]|$)/.test(result) ? result : invalid() }
const fieldPath: Decoder<{ kind: 'column'; column: string } | { kind: 'json'; column: string; pointer: string }> = (value) => {
  const { kind } = object({ kind: enumeration('column', 'json') })(value)
  if (kind === 'column') return { kind, ...object({ column: boundedString(512) })(value) }
  const result = object({ column: boundedString(512), pointer: jsonPointer })(value)
  return { kind, ...result }
}
const jsonSource = object({ ...nativeSourceBase, kind: enumeration('json'), source: boundedString(8192), pointer: nullable(jsonPointer) })
const jsonlSource = object({ ...nativeSourceBase, kind: enumeration('jsonl'), source: boundedString(8192), line: positiveDecimal, byteRange: nullable(byteRange) })
const compressedSource = object({ ...nativeSourceBase, kind: enumeration('compressedJsonl'), source: boundedString(8192), encoding: enumeration('zstd'), decodedLine: positiveDecimal, decodedByteRange: nullable(byteRange) })
const sqliteSource = object({ ...nativeSourceBase, kind: enumeration('sqlite'), database: boundedString(8192), schemaProfile: boundedString(512), table: boundedString(512), primaryKey: boundedArray(object({ column: boundedString(512), value: rowKey }), 32), field: nullable(fieldPath), observedRevision: id })
export type NativeSource = Decoded<typeof jsonSource> | Decoded<typeof jsonlSource> | Decoded<typeof compressedSource> | Decoded<typeof sqliteSource>
export type Source = Decoded<typeof collectorSource> | NativeSource
export const decodeNativeSource: Decoder<NativeSource> = (value) => {
  const { kind } = object({ kind: enumeration('json', 'jsonl', 'compressedJsonl', 'sqlite') })(value)
  switch (kind) {
    case 'json': return jsonSource(value)
    case 'jsonl': return jsonlSource(value)
    case 'compressedJsonl': return compressedSource(value)
    case 'sqlite': { const result = sqliteSource(value); if (new Set(result.primaryKey.map((key) => key.column)).size !== result.primaryKey.length) return invalid(); return result }
  }
}

const measurementShape = object({ state: enumeration('recorded', 'derived', 'unknown', 'invalid'), valueMs: nullable(number), meaning: boundedString(512) })
const measurement: Decoder<Decoded<typeof measurementShape>> = (value) => {
  const result = measurementShape(value)
  if ((result.state === 'unknown' || result.state === 'invalid') && result.valueMs !== null) return invalid()
  if ((result.state === 'recorded' || result.state === 'derived') && (result.valueMs === null || result.valueMs < 0)) return invalid()
  return result
}
const bar = object({ offsetMs: number, durationMs: number })
const decimalU64: Decoder<string> = (value) => {
  const text = boundedString(20)(value)
  return /^(0|[1-9][0-9]{0,19})$/.test(text) && BigInt(text) <= 18446744073709551615n ? text : invalid()
}
export type TokenCounter = { state: 'exact' | 'partial'; value: string } | { state: 'unknown' | 'invalid' | 'notApplicable'; value: null }
const tokenCounter: Decoder<TokenCounter> = (value) => {
  const { state } = object({ state: enumeration('exact', 'partial', 'unknown', 'invalid', 'notApplicable') })(value)
  if (state === 'exact' || state === 'partial') return { state, ...object({ value: decimalU64 })(value) }
  const result = object({ value: nullable(decimalU64) })(value)
  if (result.value !== null) return invalid()
  return { state, value: null }
}
function createTraceDecoders(source: Decoder<Source>, native = false) {
  const tokenAccountingShape = object({
    inputTokens: tokenCounter, outputTokens: tokenCounter, cacheReadTokens: tokenCounter, cacheWriteTokens: tokenCounter, totalTokens: tokenCounter,
    provenance: enumeration('none', 'recordedGeneration', 'turnSummaryFallback', 'aggregate', 'mixed', 'nativeResponse', 'nativeAuxiliary', 'derivedCumulative'),
    observedGenerations: count, totalGenerations: count, fallbackContributions: count, contributorCount: count,
  })
  const tokenAccounting: Decoder<Decoded<typeof tokenAccountingShape>> = (value) => {
    const result = tokenAccountingShape(value)
    if (!native && ['nativeResponse', 'nativeAuxiliary', 'derivedCumulative'].includes(result.provenance)) return invalid()
    if (result.observedGenerations > result.totalGenerations) return invalid()
    const classifiedContributions = result.observedGenerations + result.fallbackContributions
    if (native ? result.contributorCount < classifiedContributions : result.contributorCount !== classifiedContributions) return invalid()
    if ((result.provenance === 'none') !== (result.contributorCount === 0)) return invalid()
    return result
  }
  const spanUsageFields = { own: tokenAccounting, inclusive: tokenAccounting, childAttribution: enumeration('notApplicable', 'complete', 'unresolved', 'ambiguous') }
  const spanUsage = object(spanUsageFields)
  const usageEvidence = object({ contributionId: id, spanId: id, source, accounting: tokenAccounting, reportedTotal: tokenCounter })
  const spanUsageDetailShape = object({ ...spanUsageFields, evidence: boundedArray(usageEvidence, 16), contributorCount: count, omittedCount: count })
  const spanUsageDetail: Decoder<Decoded<typeof spanUsageDetailShape>> = (value) => {
    const result = spanUsageDetailShape(value)
    if (result.contributorCount !== result.inclusive.contributorCount || result.evidence.length + result.omittedCount !== result.contributorCount) return invalid()
    if (new Set(result.evidence.map((entry) => entry.contributionId)).size !== result.evidence.length) return invalid()
    for (const entry of result.evidence) if (entry.reportedTotal.state === 'partial' || entry.reportedTotal.state === 'notApplicable') return invalid()
    return result
  }
  const legacyToken: Decoder<number> = (value) => { const n = number(value); return Number.isInteger(n) && n >= 0 && n <= Number(18446744073709551615n) ? n : invalid() }
  const observationShape = object({
    id, spanId: id, spanUsage, kind: enumeration('session', 'interaction', 'turn', 'generation', 'tool', 'message'), label: boundedString(512), labelTruncated: boolean,
    parentId: nullable(id), depth: count, hasChildren: boolean, match: boolean, context: boolean,
    outcome: enumeration('success', 'error', 'unknown', 'incomplete'), duration: measurement, bar: nullable(bar), source,
  })
  const observation: Decoder<Decoded<typeof observationShape>> = (value) => { const result = observationShape(value); if (!native && result.kind === 'message') return invalid(); return result }
  const usage = object({ state: enumeration('complete', 'partial', 'fallback', 'unknown'), inputTokens: nullable(legacyToken), outputTokens: nullable(legacyToken), cacheReadTokens: nullable(legacyToken), cacheWriteTokens: nullable(legacyToken),
    totalTokens: nullable(legacyToken), cost: nullable(number), observedGenerations: count, totalGenerations: count, fallbackSource: nullable(enumeration('turnSummary')) })
  const structuralError = object({ code: boundedString(512), message: boundedString(512), source: nullable(source) })
  const decodeError = object({ error: structuralError })
  type StructuralError = Decoded<typeof structuralError>
  const liveStatusShape = object({ runId: id, revision: nullable(count), state: enumeration('catchingUp', 'waiting', 'paused', 'stopped'),
    activity: nullable(object({ sourceCount: count, recordCount: count, readBytes: count, pendingBytes: count })), error: nullable(structuralError) })
  const liveStatus: Decoder<Decoded<typeof liveStatusShape>> = (value) => {
    const result = liveStatusShape(value)
    if (result.state !== 'stopped' && result.error !== null) return invalid()
    if (result.activity && result.activity.pendingBytes > result.activity.readBytes) return invalid()
    return result
  }
  type LiveStatus = Decoded<typeof liveStatus>
  type WorkspaceBase = { state: 'system' } | { state: 'empty' } | { state: 'importing'; attemptId: string } | { state: 'ready'; attemptId: string; snapshotId: string } | { state: 'failed'; attemptId: string; error: StructuralError }
  type Workspace = WorkspaceBase & { live?: LiveStatus }
  const workspaceBase: Decoder<WorkspaceBase> = (value) => {
    const { state } = object({ state: enumeration('system', 'empty', 'importing', 'ready', 'failed') })(value)
    switch (state) {
      case 'system': return { state }
      case 'empty': return { state }
      case 'importing': return { state, ...object({ attemptId: id })(value) }
      case 'ready': return { state, ...object({ attemptId: id, snapshotId: id })(value) }
      case 'failed': return { state, ...object({ attemptId: id, error: structuralError })(value) }
    }
  }
  const decodeWorkspace: Decoder<Workspace> = (value) => {
    const base = workspaceBase(value)
    if (typeof value === 'object' && value !== null && 'live' in value) {
      if (base.state === 'system') return invalid()
      const { live } = object({ live: liveStatus })(value)
      if (base.state === 'ready' && live.revision === null) return invalid()
      return { ...base, live }
    }
    return base
  }
  const importStarted: Decoder<Workspace & { state: 'importing' }> = (value) => {
    const result = decodeWorkspace(value)
    return result.state === 'importing' ? result : invalid()
  }
  const decodeOverview = object({
    snapshotId: id, title: boundedString(512), source: nullable(source), model: nullable(boundedString(512)), sourceCount: count, recordCount: count,
    nodeCount: count, sessionCount: count, interactionCount: count, turnCount: count, generationCount: count, toolCount: count, diagnosticCount: count, duration: measurement, usage, tokenAccounting,
    timeline: nullable(object({ startMs: number, endMs: number, durationMs: number })),
    measurementAvailability: object({ duration: boolean, requestToHeaders: boolean, ttft: boolean }),
  })
  const decodeObservations = object({ snapshotId: id, items: array(observation), totalMatches: count, totalRows: count, nextCursor: nullable(boundedString(2048)) })
  const coverage = enumeration('complete', 'partial', 'unsupported', 'missing')
  const retention = enumeration('complete', 'listsLimited', 'retentionLimited')
  const valueSize = object({ compactJsonBytes: count, textChars: nullable(count) })
  const contextKind = enumeration('systemInstructions', 'developerInstructions', 'user', 'assistantHistory', 'toolCalls', 'toolOutputs', 'toolDefinitions', 'opaqueReasoning', 'other')
  const namedContributor = object({ name: nullable(boundedString(512)), nameTruncated: boolean, ordinal: count, size: valueSize })
  const contextInspectionShape = object({
    coverage, retention, shape: enumeration('messages', 'providerInput', 'plainText', 'unsupported', 'missing'),
    requestField: nullable(enumeration('messages', 'prompt', 'input')), source: nullable(source), payloadSize: nullable(valueSize),
    counts: nullable(object({ messages: nullable(count), toolDefinitions: nullable(count), skillDescriptors: nullable(count), unclassifiedItems: count })),
    categories: boundedArray(object({ kind: contextKind, count, size: valueSize, textCoverage: coverage }), 9),
    largestItems: boundedArray(object({ ordinal: count, kind: contextKind, size: valueSize, textCoverage: coverage }), 16),
    largestTools: boundedArray(namedContributor, 16), largestSkills: boundedArray(namedContributor, 16),
    sections: boundedArray(object({ id: count, parentId: nullable(count), kind: enumeration('tools', 'rules', 'docs', 'project_context', 'skills', 'available_skills', 'cwd', 'acp_delegate', 'ponytail'), itemOrdinal: count, textChars: count }), 32),
    omitted: object({ items: nullable(count), tools: nullable(count), skills: nullable(count), sections: nullable(count) }), contextInputTokens: nullable(legacyToken), contextInputTokensExact: tokenCounter,
  })
  const contextInspection: Decoder<Decoded<typeof contextInspectionShape>> = (value) => {
    const result = contextInspectionShape(value)
    if (new TextEncoder().encode(JSON.stringify(result)).length > 65536) return invalid()
    if (new Set(result.categories.map((category) => category.kind)).size !== result.categories.length) return invalid()
    if (result.coverage === 'missing' && (result.shape !== 'missing' || result.payloadSize !== null || result.counts !== null || result.categories.length)) return invalid()
    if (result.shape === 'unsupported' && (result.coverage !== 'unsupported' || result.counts !== null)) return invalid()
    if (result.shape === 'plainText' && result.counts !== null) return invalid()
    if (result.retention === 'retentionLimited' && (result.payloadSize !== null || result.counts !== null || result.categories.length || result.largestItems.length || result.largestTools.length || result.largestSkills.length || result.sections.length)) return invalid()
    return result
  }
  const previewEvidenceFields = {
    availability: enumeration('available', 'unavailable'), selectedSize: nullable(valueSize), previewUtf8Bytes: nullable(count), source: nullable(source),
    localTruncated: boolean, sourceTruncation: enumeration('knownTruncated', 'knownComplete', 'unknown'),
  }
  const previewEvidence = object(previewEvidenceFields)
  const resultEvidence = object({ ...previewEvidenceFields, recordedField: nullable(enumeration('resultPreview', 'result', 'output')),
    originalLength: nullable(object({ value: decimalU64, unit: enumeration('utf16CodeUnitsOfSerializedResult', 'unspecified'), source })),
  })
  const toolInspectionShape = object({ retention: enumeration('complete', 'retentionLimited'), callId: nullable(boundedString(512)), callIdTruncated: boolean, callIdSource: nullable(source), arguments: previewEvidence, result: resultEvidence })
  const toolInspection: Decoder<Decoded<typeof toolInspectionShape>> = (value) => {
    const result = toolInspectionShape(value)
    if (new TextEncoder().encode(JSON.stringify(result)).length > 65536) return invalid()
    for (const evidence of [result.arguments, result.result]) {
      if (evidence.previewUtf8Bytes !== null && evidence.previewUtf8Bytes > 8192) return invalid()
      if (evidence.availability === 'available' && evidence.previewUtf8Bytes === null) return invalid()
      if (evidence.availability === 'unavailable' && (evidence.selectedSize !== null || evidence.previewUtf8Bytes !== null || evidence.localTruncated)) return invalid()
    }
    return result
  }
  const detailShape = object({
    snapshotId: id, observation, spanUsage: spanUsageDetail, relationship: object({ state: enumeration('root', 'linked', 'unresolved', 'ambiguous'), executionParentId: nullable(id) }),
    requestToHeaders: measurement, ttft: measurement, usage,
    metadata: object({ sessionId: nullable(boundedString(512)), model: nullable(boundedString(512)), toolName: nullable(boundedString(512)), turnIndex: nullable(count), stepIndex: nullable(count) }),
    sources: array(source), diagnosticIds: array(id), context: nullable(contextInspection), toolInspection: nullable(toolInspection),
  })
  const decodeDetail: Decoder<Decoded<typeof detailShape>> = (value) => {
    const result = detailShape(value)
    const { own, inclusive, childAttribution } = result.spanUsage
    if (JSON.stringify({ own, inclusive, childAttribution }) !== JSON.stringify(result.observation.spanUsage)) return invalid()
    if (native ? result.context !== null && result.observation.kind !== 'generation' : (result.observation.kind === 'generation') !== (result.context !== null)) return invalid()
    if (native ? result.toolInspection !== null && result.observation.kind !== 'tool' : (result.observation.kind === 'tool') !== (result.toolInspection !== null)) return invalid()
    return result
  }
  const contentFieldShape = object({ state: enumeration('available', 'unavailable'), text: nullable(boundedString(8192)), truncated: boolean, limitBytes: count })
  const contentField: Decoder<Decoded<typeof contentFieldShape>> = (value) => {
    const result = contentFieldShape(value)
    if (result.limitBytes !== 8192 || (result.state === 'unavailable' && (result.text !== null || result.truncated)) || (result.state === 'available' && result.text === null)) return invalid()
    return result
  }
  const decodeContent = object({ snapshotId: id, nodeId: id, input: contentField, output: contentField })
  const decodeDiagnostics = object({ snapshotId: id, items: array(object({ id, code: boundedString(512), message: boundedString(512), source, nodeId: nullable(id) })), total: count, nextCursor: nullable(boundedString(2048)) })
  return { decodeWorkspace, decodeOverview, decodeObservations, decodeDetail, decodeContent, decodeDiagnostics, decodeError, importStarted, tokenAccounting, spanUsageDetail, contextInspection, previewEvidence, measurement, usage, liveStatus }
}
const collectorDecoders = createTraceDecoders(collectorSource)
export const { decodeWorkspace, decodeOverview, decodeObservations, decodeDetail, decodeContent, decodeDiagnostics, decodeError } = collectorDecoders
const { importStarted } = collectorDecoders
export type StructuralError = Decoded<typeof decodeError>['error']
export type Workspace = Decoded<typeof decodeWorkspace>
export type LiveStatus = Decoded<typeof collectorDecoders.liveStatus>
export type TokenAccounting = Decoded<typeof collectorDecoders.tokenAccounting>
export type SpanUsageDetail = Decoded<typeof collectorDecoders.spanUsageDetail>
export type Overview = Decoded<typeof collectorDecoders.decodeOverview>
export type ObservationPage = Decoded<typeof collectorDecoders.decodeObservations>
export type NodeDetail = Decoded<typeof collectorDecoders.decodeDetail> & { nativeUsage?: NativeUsageEvidence[]; relationships?: Decoded<typeof nativeRelationship>[]; timingEvidence?: Decoded<typeof timingEvidence>[]; selectedContext?: string | null }
export type ContextInspection = Decoded<typeof collectorDecoders.contextInspection>
export type PreviewEvidence = Decoded<typeof collectorDecoders.previewEvidence>
export type NodeContent = Decoded<typeof collectorDecoders.decodeContent>
export type DiagnosticPage = Decoded<typeof collectorDecoders.decodeDiagnostics>
export type Measurement = Decoded<typeof collectorDecoders.measurement>
export type Usage = Decoded<typeof collectorDecoders.usage>
export type Observation = ObservationPage['items'][number]
export class ApiError extends Error {
  readonly status: number
  readonly structural: StructuralError
  constructor(status: number, error: StructuralError) { super(error.message); this.name = 'ApiError'; this.status = status; this.structural = error }
}
let nativeRequestActive = false
const nativeRequests: (() => void)[] = []
function nativeRequestTurn(signal: AbortSignal): Promise<() => void> {
  if (signal.aborted) return Promise.reject(new DOMException('API request cancelled.', 'AbortError'))
  if (nativeRequests.length >= 8) return Promise.reject(new Error('The local request queue is full. Retry after the current request finishes.'))
  return new Promise((resolve, reject) => {
    const cancel = () => {
      const index = nativeRequests.indexOf(begin)
      if (index >= 0) nativeRequests.splice(index, 1)
      reject(new DOMException('API request cancelled.', 'AbortError'))
    }
    const begin = () => {
      signal.removeEventListener('abort', cancel)
      nativeRequestActive = true
      let released = false
      resolve(() => {
        if (released) return
        released = true
        const next = nativeRequests.shift()
        if (next) next()
        else nativeRequestActive = false
      })
    }
    if (nativeRequestActive) {
      nativeRequests.push(begin)
      signal.addEventListener('abort', cancel, { once: true })
    } else begin()
  })
}
async function request<T>(path: string, decode: Decoder<T>, signal: AbortSignal, method = 'GET', errorDecoder = decodeError): Promise<T> {
  if (signal.aborted) throw new DOMException('API request cancelled.', 'AbortError')
  const controller = new AbortController()
  let timedOut = false
  let releaseNative = () => {}
  let cancel = () => {}
  const cancelled = new Promise<never>((_, reject) => {
    cancel = () => { controller.abort(); reject(new DOMException('API request cancelled.', 'AbortError')) }
    if (signal.aborted) cancel()
    else signal.addEventListener('abort', cancel, { once: true })
  })
  const timeout = setTimeout(() => { timedOut = true; cancel() }, 10000)
  async function receive() {
    if (/^(system|sessions)(?:[/?]|$)/.test(path)) {
      const release = await nativeRequestTurn(controller.signal)
      if (controller.signal.aborted) { release(); throw new DOMException('API request cancelled.', 'AbortError') }
      releaseNative = release
    }
    const response = await fetch(`/api/v1/${path}`, { signal: controller.signal, method, cache: 'no-store', headers: { Accept: 'application/json' } })
    const body = await response.text()
    if (new TextEncoder().encode(body).length > 1024 * 1024) throw new ContractError()
    let value: unknown
    try { value = JSON.parse(body) } catch { throw new ContractError() }
    if (!response.ok) throw new ApiError(response.status, errorDecoder(value).error)
    return decode(value)
  }
  try { return await Promise.race([receive(), cancelled]) }
  catch (error) { if (timedOut) throw new Error('The local service request timed out.'); throw error }
  finally { clearTimeout(timeout); signal.removeEventListener('abort', cancel); releaseNative() }
}
function snapshotPath(snapshot: string) { return `snapshots/${encodeURIComponent(snapshot)}` }
async function scoped<T extends { snapshotId: string }>(path: string, snapshot: string, decode: Decoder<T>, signal: AbortSignal): Promise<T> {
  const result = await request(`${snapshotPath(snapshot)}/${path}`, decode, signal)
  if (result.snapshotId !== snapshot) throw new ContractError()
  return result
}
export const traceApi = {
  workspace: (signal: AbortSignal) => request('workspace', decodeWorkspace, signal),
  retry: (signal: AbortSignal) => request('retry', importStarted, signal, 'POST'),
  pause: (signal: AbortSignal) => request('live/pause', decodeWorkspace, signal, 'POST'),
  resume: (signal: AbortSignal) => request('live/resume', decodeWorkspace, signal, 'POST'),
  overview: (snapshot: string, signal: AbortSignal) => scoped('overview', snapshot, decodeOverview, signal),
  observations: (snapshot: string, query: URLSearchParams, signal: AbortSignal) => scoped(`observations?${query}`, snapshot, decodeObservations, signal),
  detail: async (snapshot: string, node: string, signal: AbortSignal) => {
    const result = await scoped(`nodes/${encodeURIComponent(node)}`, snapshot, decodeDetail, signal)
    if (result.observation.id !== node) throw new ContractError()
    return result
  },
  spanDetail: async (snapshot: string, span: string, signal: AbortSignal) => {
    const result = await scoped(`spans/${encodeURIComponent(span)}`, snapshot, decodeDetail, signal)
    if (result.observation.spanId !== span) throw new ContractError()
    return result
  },
  content: async (snapshot: string, node: string, signal: AbortSignal) => {
    const result = await scoped(`nodes/${encodeURIComponent(node)}/content`, snapshot, decodeContent, signal)
    if (result.nodeId !== node) throw new ContractError()
    return result
  },
  diagnostics: (snapshot: string, cursor: string | null, signal: AbortSignal) => scoped(`diagnostics${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ''}`, snapshot, decodeDiagnostics, signal),
}

const nativeDecoders = createTraceDecoders(decodeNativeSource, true)
const nativeStructuralError = object({ code: boundedString(512), message: boundedString(512), source: nullable(decodeNativeSource) })
export const decodeNativeError = object({ error: nativeStructuralError, retryAfterMs: nullable(count) })
const capabilityCoverage = enumeration('complete', 'partial', 'unsupported', 'missing')
const capabilities = object({ profile: boundedString(512), messages: capabilityCoverage, usage: capabilityCoverage, executionParents: capabilityCoverage, timing: capabilityCoverage, persistence: enumeration('persistedRecordsOnly') })
const nativeRelationship = object({ kind: boundedString(512), target: boundedString(2048), source: decodeNativeSource, execution: boolean })
const nativeCounter = object({ name: boundedString(512), value: tokenCounter, source: decodeNativeSource })
const usageRole: Decoder<{ kind: 'contribution'; category: string } | { kind: 'reconciliation'; scope: string } | { kind: 'inherited'; origin: string } | { kind: 'breakdown'; includedIn: string }> = (value) => {
  const { kind } = object({ kind: enumeration('contribution', 'reconciliation', 'inherited', 'breakdown') })(value)
  switch (kind) {
    case 'contribution': return { kind, ...object({ category: boundedString(512) })(value) }
    case 'reconciliation': return { kind, ...object({ scope: boundedString(512) })(value) }
    case 'inherited': return { kind, ...object({ origin: boundedString(2048) })(value) }
    case 'breakdown': return { kind, ...object({ includedIn: boundedString(2048) })(value) }
  }
}
const nativeUsage = object({ id, role: usageRole, authority: boundedString(512), accepted: boolean, source: decodeNativeSource,
  nativeCounters: boundedArray(nativeCounter, 64), normalization: boundedArray(object({ component: boundedString(512), rule: boundedString(512), operands: boundedArray(nativeCounter, 64) }), 64), accounting: nativeDecoders.tokenAccounting })
export const decodeNativeUsage = (value: unknown) => { const result = nativeUsage(value); nativeNamespace(result); return result }
export type NativeUsageEvidence = Decoded<typeof nativeUsage>
type NativeNamespace = { provider: Provider; storeId: string }
function nativeNamespace(value: unknown, namespace?: NativeNamespace): NativeNamespace | undefined {
  let expected = namespace
  function visit(value: unknown): void {
    if (typeof value !== 'object' || value === null) return
    if ('kind' in value && (value.kind === 'json' || value.kind === 'jsonl' || value.kind === 'compressedJsonl' || value.kind === 'sqlite')) {
      const source = decodeNativeSource(value)
      expected ??= { provider: source.provider, storeId: source.storeId }
      if (source.provider !== expected.provider || source.storeId !== expected.storeId) throw new ContractError()
    } else for (const child of Object.values(value)) visit(child)
  }
  visit(value)
  return expected
}
function nativeView<T>(decode: Decoder<T>): Decoder<T> { return (value) => { const result = decode(value); nativeNamespace(result); return result } }
export const decodeNativeOverview = nativeView(nativeDecoders.decodeOverview)
export const decodeNativeObservations = nativeView(nativeDecoders.decodeObservations)
export const decodeNativeDiagnostics = nativeView(nativeDecoders.decodeDiagnostics)
const timingEvidence = object({ meaning: enumeration('persistence', 'requestStart', 'resultCreation', 'executionStart', 'executionEnd'), value: boundedString(512), unit: boundedString(512), source: decodeNativeSource })
export const decodeNativeDetail = nativeView((value: unknown) => ({ ...nativeDecoders.decodeDetail(value), ...object({ nativeUsage: boundedArray(nativeUsage, 16), relationships: boundedArray(nativeRelationship, 1024), timingEvidence: boundedArray(timingEvidence, 1024), selectedContext: nullable(boundedString(2048)) })(value) }))
const catalogCoverage = object({ state: enumeration('complete', 'partial', 'limited'), limits: boundedArray(boundedString(512), 32), retainedEntries: count, omittedCount: nullable(count) })
const catalogItemShape = object({ id: opaqueId, provider, storeId: opaqueId, nativeId: boundedString(2048), recordingId: boundedString(2048), project: nullable(boundedString(8192)), sourceRevision: id, capabilities, source: decodeNativeSource, relationships: boundedArray(nativeRelationship, 1024), activityMs: nullable(count), ingestionState: enumeration('discovered', 'unavailable'), lastObservedMs: count, processState: enumeration('unknown') })
const catalogItem: Decoder<Decoded<typeof catalogItemShape>> = (value) => {
  const result = catalogItemShape(value)
  if (result.source.provider !== result.provider || result.source.storeId !== result.storeId) return invalid()
  for (const relation of result.relationships) if (relation.source.provider !== result.provider || relation.source.storeId !== result.storeId) return invalid()
  return result
}
const catalogPageShape = object({ revision: positiveDecimal, items: boundedArray(catalogItem, 100), nextCursor: nullable(boundedString(2048)), coverage: catalogCoverage })
export const decodeCatalog: Decoder<Decoded<typeof catalogPageShape>> = (value) => {
  const result = catalogPageShape(value)
  if (new Set(result.items.map((item) => item.id)).size !== result.items.length || result.items.length > result.coverage.retainedEntries) return invalid()
  if (result.nextCursor !== null && (!result.items.length || !result.nextCursor.startsWith(`${result.revision}:`))) return invalid()
  return result
}
const systemStatusShape = object({ mode: enumeration('system'), catalogRevision: positiveDecimal,
  stores: boundedArray(object({ id: opaqueId, provider, root: boundedString(8192), state: enumeration('pending', 'indexing', 'current', 'partial', 'absent', 'unavailable'), error: nullable(nativeStructuralError), visited: count, sweepComplete: boolean }), 16), disabledProviders: boundedArray(provider, 4), coverage: catalogCoverage,
  limits: object({ stores: count, catalogEntries: count, discoveryEntriesPerPass: count, discoveryBatch: count, catalogPage: count, catalogRevisions: count, analyses: count, activeDemandMs: count, idleEvictionMs: count, parseJobs: count, databaseConnections: count, batchBytes: count, batchRecords: count, retainedDecodedBytes: count, sessionDecodedBytes: count, sessionRecords: count, recordBytes: count, relatedSources: count, compressedBytes: count }),
  metrics: object({ parseJobs: count, readRecords: count, readBytes: count, databaseConnections: count, loadedEntries: count, retainedBytes: count, discoveryBatches: count, activeParseJobs: count, maxActiveParseJobs: count }),
})
export const decodeSystem: Decoder<Decoded<typeof systemStatusShape>> = (value) => {
  const result = systemStatusShape(value)
  for (const store of result.stores) {
    const source = store.error?.source
    if (source && (source.provider !== store.provider || source.storeId !== store.id)) return invalid()
  }
  return result
}
const sessionWorkspaceShape = object({ state: enumeration('loading', 'ready', 'stale', 'failed'), sessionId: opaqueId, runId: nullable(id), snapshotId: nullable(id), revision: count, progress: enumeration('pending', 'current', 'catchingUp'), error: nullable(nativeStructuralError), sourceRevision: id, capabilities, processState: enumeration('unknown') })
export const decodeSessionWorkspace: Decoder<Decoded<typeof sessionWorkspaceShape>> = (value) => {
  const result = sessionWorkspaceShape(value)
  if ((result.runId === null) !== (result.snapshotId === null)) return invalid()
  if ((result.state === 'ready' || result.state === 'stale') && result.snapshotId === null) return invalid()
  if (result.state === 'failed' && result.snapshotId !== null) return invalid()
  if (result.state === 'failed' && result.error === null) return invalid()
  if (result.error !== null && result.state !== 'failed' && result.state !== 'stale') return invalid()
  return result
}
export type CatalogItem = Decoded<typeof catalogItem>
export type CatalogPage = Decoded<typeof decodeCatalog>
export type SystemStatus = Decoded<typeof decodeSystem>
export type SessionWorkspace = Decoded<typeof decodeSessionWorkspace>
export type WorkbenchApi = Pick<typeof traceApi, 'overview' | 'observations' | 'detail' | 'spanDetail' | 'content' | 'diagnostics'>
export const systemApi = {
  status: (signal: AbortSignal) => request('system', decodeSystem, signal, 'GET', decodeNativeError),
  catalog: (query: URLSearchParams, signal: AbortSignal) => request(`sessions?${query}`, decodeCatalog, signal, 'GET', decodeNativeError),
  workspace: async (session: string, signal: AbortSignal) => {
    opaqueId(session)
    const result = await request(`sessions/${encodeURIComponent(session)}/workspace`, decodeSessionWorkspace, signal, 'GET', decodeNativeError)
    if (result.sessionId !== session) throw new ContractError()
    return result
  },
  retry: async (session: string, signal: AbortSignal) => {
    opaqueId(session)
    const result = await request(`sessions/${encodeURIComponent(session)}/retry`, decodeSessionWorkspace, signal, 'POST', decodeNativeError)
    if (result.sessionId !== session) throw new ContractError()
    return result
  },
}
export function sessionApi(session: string, namespace?: { provider: Provider; storeId: string }): WorkbenchApi {
  opaqueId(session)
  let expectedNamespace = namespace
  async function scopedNative<T extends { snapshotId: string }>(path: string, snapshot: string, decode: Decoder<T>, signal: AbortSignal): Promise<T> {
    const result = await request(`sessions/${encodeURIComponent(session)}/snapshots/${encodeURIComponent(snapshot)}/${path}`, decode, signal, 'GET', decodeNativeError)
    if (result.snapshotId !== snapshot) throw new ContractError()
    expectedNamespace = nativeNamespace(result, expectedNamespace)
    return result
  }
  return {
    overview: (snapshot, signal) => scopedNative('overview', snapshot, decodeNativeOverview, signal),
    observations: (snapshot, query, signal) => scopedNative(`observations?${query}`, snapshot, decodeNativeObservations, signal),
    detail: async (snapshot, node, signal) => { const result = await scopedNative(`nodes/${encodeURIComponent(node)}`, snapshot, decodeNativeDetail, signal); if (result.observation.id !== node) throw new ContractError(); return result },
    spanDetail: async (snapshot, span, signal) => { const result = await scopedNative(`spans/${encodeURIComponent(span)}`, snapshot, decodeNativeDetail, signal); if (result.observation.spanId !== span) throw new ContractError(); return result },
    content: async (snapshot, node, signal) => { const result = await scopedNative(`nodes/${encodeURIComponent(node)}/content`, snapshot, decodeContent, signal); if (result.nodeId !== node) throw new ContractError(); return result },
    diagnostics: (snapshot, cursor, signal) => scopedNative(`diagnostics${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ''}`, snapshot, decodeNativeDiagnostics, signal),
  }
}
