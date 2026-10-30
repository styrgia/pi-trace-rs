import { ApiError, ContractError, sessionApi, systemApi, type WorkbenchApi, type CatalogItem, type CatalogPage, type SystemStatus, type SessionWorkspace, type Overview } from './trace-api.ts'
import { WorkspacePoller, type ConnectionState } from './workspace-poller.ts'

export type SystemState = {
  status: SystemStatus | null
  catalog: CatalogPage | null
  catalogPending: boolean
  catalogError: string | null
  statusError: string | null
  selected: CatalogItem | null
  selectedId: string | null
  workspace: SessionWorkspace | null
  overview: Overview | null
  initialRevision: number | null
  sessionError: string | null
  sessionPresence: 'unknown' | 'available' | 'unavailable'
  connection: ConnectionState
}
export type SystemClient = typeof systemApi
export type SystemControllerOptions = {
  api?: SystemClient
  investigationApi?: (session: string, namespace?: { provider: CatalogItem['provider']; storeId: string }) => WorkbenchApi
  onchange: (state: SystemState) => void
  oninvalidate?: (session: string) => void
  initialSession?: string | null
  intervalMs?: number
  timeoutMs?: number
}
export class SystemController {
  private state: SystemState = { status: null, catalog: null, catalogPending: false, catalogError: null, statusError: null, selected: null, selectedId: null, workspace: null, overview: null, initialRevision: null, sessionError: null, sessionPresence: 'unknown', connection: { kind: 'loading' } }
  private options: SystemControllerOptions
  private api: SystemClient
  private provider = ''
  private project = ''
  private loadedRows = 100
  private catalogIdentity = 0
  private sessionIdentity = 0
  private evidenceIdentity = 0
  private catalogController: AbortController | null = null
  private overviewController: AbortController | null = null
  private statusPoller: WorkspacePoller<SystemStatus>
  private sessionPoller: WorkspacePoller<SessionWorkspace> | null = null
  private catalogRevision: string | null = null
  private catalogInFlight = false
  private catalogAgain = false
  private overviewSnapshot: string | null = null
  private retryController: AbortController | null = null
  private retryPending = false
  private catalogFailures = 0
  private catalogRetryAt = 0
  private catalogRetryTimer: ReturnType<typeof setTimeout> | undefined
  private running = false
  private visible = true
  constructor(options: SystemControllerOptions) {
    this.options = options; this.api = options.api ?? systemApi
    this.state.selectedId = options.initialSession && /^[A-Za-z0-9_-]{1,2048}$/.test(options.initialSession) ? options.initialSession : null
    this.statusPoller = new WorkspacePoller({ request: (signal) => this.api.status(signal).catch((error: unknown) => { throw new Error(describe(error)) }), intervalMs: options.intervalMs ?? 1000, timeoutMs: options.timeoutMs ?? 3000,
      onworkspace: (status) => {
        this.state.status = status; this.publish()
        if (status.catalogRevision !== this.catalogRevision) void this.loadCatalog(false)
      }, onconnection: (connection) => { if (!this.state.selectedId) this.state.connection = connection; this.state.statusError = connection.kind === 'reconnecting' ? connection.message : null; this.publish() } })
  }
  start() { this.running = true; this.statusPoller.start(); if (this.state.selectedId) this.follow(this.state.selectedId); void this.loadCatalog(false) }
  stop() { this.running = false; ++this.catalogIdentity; ++this.sessionIdentity; this.statusPoller.stop(); this.sessionPoller?.stop(); this.catalogController?.abort(); this.overviewController?.abort(); this.retryController?.abort(); clearTimeout(this.catalogRetryTimer) }
  setVisible(visible: boolean) {
    this.visible = visible; this.statusPoller.setVisible(visible); this.sessionPoller?.setVisible(visible)
    if (!visible) { ++this.catalogIdentity; this.catalogController?.abort(); this.overviewController?.abort(); this.overviewSnapshot = null }
    else if (this.running) { this.catalogRevision = null; void this.loadCatalog(false) }
  }
  resync() { this.catalogFailures = 0; this.catalogRetryAt = 0; clearTimeout(this.catalogRetryTimer); this.statusPoller.resync(); this.sessionPoller?.resync(); this.catalogRevision = null; void this.loadCatalog(false) }
  filter(provider: string, project: string) {
    this.provider = provider; this.project = project; this.loadedRows = 100; this.catalogFailures = 0; this.catalogRetryAt = 0; clearTimeout(this.catalogRetryTimer); this.catalogRevision = null
    ++this.catalogIdentity; this.catalogController?.abort(); this.state.catalog = null; this.state.catalogError = null
    void this.loadCatalog(false)
  }
  more() { if (this.state.catalog?.nextCursor) void this.loadCatalog(true) }
  select(item: CatalogItem | null) {
    if (item?.id === this.state.selectedId) return
    ++this.sessionIdentity; ++this.evidenceIdentity; this.sessionPoller?.stop(); this.overviewController?.abort(); this.retryController?.abort(); this.retryPending = false; this.overviewSnapshot = null
    this.state.selected = item; this.state.selectedId = item?.id ?? null; this.state.workspace = null; this.state.overview = null; this.state.initialRevision = null; this.state.sessionError = null; this.state.sessionPresence = 'unknown'
    this.state.connection = { kind: 'loading' }; this.publish()
    if (item) this.follow(item.id)
  }
  async retry() {
    const id = this.state.selectedId
    if (!id || this.retryPending) return
    this.retryPending = true
    const identity = this.sessionIdentity
    this.sessionPoller?.stop()
    const controller = new AbortController(); this.retryController = controller
    const timeout = setTimeout(() => controller.abort(), this.options.timeoutMs ?? 10000)
    try { const workspace = await cancelled(this.api.retry(id, controller.signal), controller.signal); if (this.running && identity === this.sessionIdentity && id === this.state.selectedId) await this.acceptWorkspace(workspace, identity) }
    catch (error) { if (this.running && identity === this.sessionIdentity) { if (isUnknownSession(error)) this.invalidateSession(error); else { this.state.sessionError = controller.signal.aborted ? 'Recovery timed out. Reconnecting to discover session state.' : describe(error); this.publish() } } }
    finally { clearTimeout(timeout); if (identity === this.sessionIdentity) { this.retryPending = false; this.retryController = null; if (this.running) this.sessionPoller?.start() } }
  }
  private publish() { this.options.onchange({ ...this.state }) }
  private follow(id: string) {
    const identity = ++this.sessionIdentity
    this.sessionPoller = new WorkspacePoller({ request: (signal) => this.api.workspace(id, signal).catch((error: unknown) => {
        if (!signal.aborted && identity === this.sessionIdentity && id === this.state.selectedId && isUnknownSession(error)) this.invalidateSession(error)
        throw error
      }), intervalMs: this.options.intervalMs ?? 500, timeoutMs: this.options.timeoutMs ?? 3000,
      onworkspace: (workspace) => { if (identity === this.sessionIdentity && this.state.selectedId === id) void this.acceptWorkspace(workspace, identity) },
      onconnection: (connection) => { if (identity === this.sessionIdentity) { this.state.connection = connection; this.publish() } } })
    this.sessionPoller.setVisible(this.visible); this.sessionPoller.start()
  }
  private async acceptWorkspace(workspace: SessionWorkspace, identity: number) {
    if (workspace.sessionId !== this.state.selectedId) return
    this.state.workspace = workspace; this.state.sessionError = null; this.state.sessionPresence = 'available'
    if (!workspace.snapshotId || !workspace.runId) {
      ++this.evidenceIdentity; this.overviewController?.abort(); this.overviewSnapshot = null
      this.state.overview = null; this.state.initialRevision = null; this.publish(); return
    }
    this.publish()
    if (this.state.overview || this.overviewSnapshot === workspace.snapshotId) return
    this.overviewController?.abort(); const controller = new AbortController(); this.overviewController = controller
    const evidenceIdentity = this.evidenceIdentity
    const snapshot = workspace.snapshotId; this.overviewSnapshot = snapshot
    const timeout = setTimeout(() => controller.abort(), this.options.timeoutMs ?? 10000)
    try {
      const api = (this.options.investigationApi ?? sessionApi)(workspace.sessionId, this.state.selected ?? undefined)
      const overview = await cancelled(api.overview(snapshot, controller.signal), controller.signal)
      if (overview.snapshotId !== snapshot) throw new ContractError()
      if (this.running && !controller.signal.aborted && identity === this.sessionIdentity && evidenceIdentity === this.evidenceIdentity && this.state.workspace?.snapshotId === snapshot) {
        this.state.overview = overview; this.state.initialRevision = workspace.revision; this.publish()
      }
    } catch (error) {
      if (this.running && identity === this.sessionIdentity && this.overviewController === controller) {
        if (isUnknownSession(error)) this.invalidateSession(error)
        else if (error instanceof ApiError && error.structural.code === 'unknown_snapshot') this.sessionPoller?.resync()
        else { this.state.sessionError = controller.signal.aborted ? 'Investigation request timed out. Retrying from session status.' : describe(error); this.publish() }
      }
    } finally { clearTimeout(timeout); if (this.overviewController === controller) this.overviewSnapshot = null }
  }
  private invalidateSession(error: ApiError) {
    const notify = this.state.sessionPresence !== 'unavailable'
    ++this.evidenceIdentity; this.overviewController?.abort(); this.overviewController = null; this.overviewSnapshot = null
    this.state.workspace = null; this.state.overview = null; this.state.initialRevision = null
    this.state.sessionPresence = 'unavailable'; this.state.sessionError = `${describe(error)} The selected session is unavailable. Its identity is retained for lookup and recovery.`
    if (notify && this.state.selectedId) this.options.oninvalidate?.(this.state.selectedId)
    this.publish()
  }
  private async loadCatalog(append: boolean) {
    if (!this.running || !this.visible || Date.now() < this.catalogRetryAt) return
    if (this.catalogInFlight) { this.catalogAgain = true; return }
    this.catalogInFlight = true; this.catalogAgain = false
    const identity = ++this.catalogIdentity; const controller = new AbortController(); this.catalogController = controller
    const timeout = setTimeout(() => controller.abort(), this.options.timeoutMs ?? 10000)
    this.state.catalogPending = true; this.publish()
    try {
      let page = append ? this.state.catalog : null
      const desired = append ? (page?.items.length ?? 0) + 100 : this.loadedRows
      do {
        const query = new URLSearchParams({ limit: '100' })
        if (this.provider) query.set('provider', this.provider)
        if (this.project) query.set('project', this.project)
        if (page?.nextCursor) query.set('cursor', page.nextCursor)
        const next = await cancelled(this.api.catalog(query, controller.signal), controller.signal)
        if (!this.running || controller.signal.aborted || identity !== this.catalogIdentity) return
        if (page && (page.revision !== next.revision || page.items.some((old) => next.items.some((item) => old.id === item.id)))) throw new ContractError()
        page = page ? { ...next, items: [...page.items, ...next.items] } : next
      } while (page.nextCursor && page.items.length < Math.min(2000, desired))
      this.catalogRevision = page.revision; this.loadedRows = page.items.length
      this.state.catalog = page; this.state.catalogError = null; this.catalogFailures = 0; this.catalogRetryAt = 0; clearTimeout(this.catalogRetryTimer)
      const selected = page.items.find((item) => item.id === this.state.selectedId)
      if (selected) this.state.selected = selected
    } catch (error) {
      if (this.running && identity === this.catalogIdentity) {
        if (error instanceof ApiError && ['expired_cursor', 'unknown_catalog_revision', 'expired_catalog_revision', 'invalid_cursor'].includes(error.structural.code)) { this.catalogRevision = null; this.catalogAgain = true; this.state.catalogError = 'Catalog changed while paging. Refreshing the current revision.' }
        else {
          this.state.catalogError = controller.signal.aborted ? 'Catalog request timed out. Retained results remain visible.' : describe(error)
          const delay = Math.min(1000 * 2 ** Math.min(++this.catalogFailures, 3), 8000)
          this.catalogRetryAt = Date.now() + delay
          clearTimeout(this.catalogRetryTimer); this.catalogRetryTimer = setTimeout(() => void this.loadCatalog(false), delay)
        }
      }
    } finally {
      clearTimeout(timeout); this.catalogInFlight = false
      if (identity === this.catalogIdentity) this.state.catalogPending = false
      this.publish()
      if (this.catalogAgain && this.running && this.visible) { this.catalogAgain = false; void this.loadCatalog(false) }
    }
  }
}
function describe(error: unknown) { return error instanceof ApiError ? `${error.structural.code}: ${error.message}` : error instanceof Error ? error.message : 'The local service could not be reached.' }

function cancelled<T>(promise: Promise<T>, signal: AbortSignal): Promise<T> {
  return new Promise((resolve, reject) => {
    const abort = () => reject(new DOMException('Request cancelled.', 'AbortError'))
    if (signal.aborted) { abort(); return }
    signal.addEventListener('abort', abort, { once: true })
    promise.then(resolve, reject).finally(() => signal.removeEventListener('abort', abort))
  })
}

function isUnknownSession(error: unknown): error is ApiError { return error instanceof ApiError && error.status === 404 && error.structural.code === 'unknown_session' }
