import { traceApi, ContractError, type Overview, type ObservationPage, type DiagnosticPage, type NodeDetail } from './trace-api.ts'
import type { Filter } from './view-state.ts'

export type Selection = { kind: 'none' } | { kind: 'span'; spanId: string } | { kind: 'node'; snapshotId: string; nodeId: string }
export type SnapshotView = { overview: Overview; page: ObservationPage; detail: NodeDetail | null; diagnostics: DiagnosticPage | null; retainedBound: boolean }
type ViewApi = Pick<typeof traceApi, 'overview' | 'observations' | 'detail' | 'spanDetail' | 'diagnostics'>
export type SnapshotRequest = {
  snapshotId: string
  search: string
  filter: Filter
  retainedRows: number
  diagnosticsRows: number
  selection: Selection
  signal: AbortSignal
}
export const RETAINED_ROWS = 2000
function sameSnapshot<T extends { snapshotId: string }>(value: T, snapshot: string): T {
  if (value.snapshotId !== snapshot) throw new ContractError()
  return value
}

export async function loadSnapshotView(request: SnapshotRequest, api: ViewApi = traceApi): Promise<SnapshotView> {
  const controller = new AbortController()
  const cancel = () => controller.abort()
  request.signal.addEventListener('abort', cancel, { once: true })
  if (request.signal.aborted) controller.abort()
  const { signal } = controller
  const snapshot = request.snapshotId
  let retainedBound = false
  async function rows() {
    const retained = Math.min(RETAINED_ROWS, Math.max(100, request.retainedRows))
    let page: ObservationPage | null = null
    const ids = new Set<string>()
    do {
      const query = new URLSearchParams()
      if (request.search) query.set('q', request.search)
      if (request.filter) query.set('filter', request.filter)
      if (page?.nextCursor) query.set('cursor', page.nextCursor)
      const next: ObservationPage = sameSnapshot(await api.observations(snapshot, query, signal), snapshot)
      if (next.items.length > 200 || (next.nextCursor !== null && next.items.length === 0)) throw new ContractError()
      if (page && (page.totalMatches !== next.totalMatches || page.totalRows !== next.totalRows)) throw new ContractError()
      for (const row of next.items) { if (ids.has(row.id)) throw new ContractError(); ids.add(row.id) }
      if (page && page.items.length + next.items.length > RETAINED_ROWS) { retainedBound = true; break }
      page = page ? { ...next, items: [...page.items, ...next.items] } : next
    } while (page.nextCursor && page.items.length < retained)
    retainedBound ||= page.items.length >= RETAINED_ROWS && page.nextCursor !== null
    return page
  }
  async function diagnostics() {
    if (request.diagnosticsRows === 0) return null
    const retained = Math.min(RETAINED_ROWS, Math.max(100, request.diagnosticsRows))
    let page: DiagnosticPage | null = null
    do {
      const next: DiagnosticPage = sameSnapshot(await api.diagnostics(snapshot, page?.nextCursor ?? null, signal), snapshot)
      if (next.items.length > 200 || (next.nextCursor !== null && next.items.length === 0)) throw new ContractError()
      if (page && page.total !== next.total) throw new ContractError()
      if (page && page.items.length + next.items.length > RETAINED_ROWS) break
      page = page ? { ...next, items: [...page.items, ...next.items] } : next
    } while (page.nextCursor && page.items.length < retained)
    return page
  }
  async function detail() {
    const selected = request.selection
    if (selected.kind === 'none') return null
    if (selected.kind === 'node' && selected.snapshotId !== snapshot) throw new ContractError()
    const result = sameSnapshot(await (selected.kind === 'span' ? api.spanDetail(snapshot, selected.spanId, signal) : api.detail(snapshot, selected.nodeId, signal)), snapshot)
    if (selected.kind === 'span' && result.observation.spanId !== selected.spanId) throw new ContractError()
    if (selected.kind === 'node' && result.observation.id !== selected.nodeId) throw new ContractError()
    return result
  }
  try {
    const [overview, page, selected, diagnosticPage] = await Promise.all([
      api.overview(snapshot, signal).then((value) => sameSnapshot(value, snapshot)), rows(), detail(), diagnostics(),
    ])
    if (signal.aborted) throw new DOMException('Snapshot refresh cancelled.', 'AbortError')
    if (selected) {
      const row = page.items.find((row) => row.spanId === selected.observation.spanId)
      if (row && JSON.stringify(row.spanUsage) !== JSON.stringify(selected.observation.spanUsage)) throw new ContractError()
    }
    return { overview, page, detail: selected, diagnostics: diagnosticPage, retainedBound }
  } finally { controller.abort(); request.signal.removeEventListener('abort', cancel) }
}
