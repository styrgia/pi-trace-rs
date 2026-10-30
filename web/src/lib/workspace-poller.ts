import type { Workspace } from './trace-api.ts'

export type ConnectionState =
  | { kind: 'loading' | 'connected' | 'suspended' }
  | { kind: 'reconnecting'; message: string; retryInMs: number }
export type PollerOptions<T = Workspace> = {
  request: (signal: AbortSignal) => Promise<T>
  onworkspace: (workspace: T) => void
  onconnection: (state: ConnectionState) => void
  intervalMs?: number
  timeoutMs?: number
  maxBackoffMs?: number
}

export class WorkspacePoller<T = Workspace> {
  private running = false
  private visible = true
  private active = false
  private identity = 0
  private failures = 0
  private immediate = false
  private controller: AbortController | null = null
  private timer: ReturnType<typeof setTimeout> | undefined
  private options: PollerOptions<T>
  constructor(options: PollerOptions<T>) { this.options = options }
  start() { this.running = true; this.resync() }
  stop() { this.running = false; ++this.identity; clearTimeout(this.timer); this.controller?.abort() }
  setVisible(visible: boolean) {
    this.visible = visible
    if (visible) this.resync()
    else { ++this.identity; clearTimeout(this.timer); this.controller?.abort(); this.options.onconnection({ kind: 'suspended' }) }
  }
  resync() {
    ++this.identity; this.failures = 0; clearTimeout(this.timer)
    if (!this.running || !this.visible) return
    if (this.active) { this.immediate = true; this.controller?.abort() }
    else void this.poll()
  }
  private async poll() {
    if (!this.running || !this.visible || this.active) return
    this.active = true; this.immediate = false
    const identity = this.identity
    const controller = new AbortController(); this.controller = controller
    let timedOut = false
    let nextDelay = this.options.intervalMs ?? 500
    let cancel = () => {}
    const aborted = new Promise<never>((_, reject) => {
      cancel = () => reject(new DOMException('Workspace status cancelled.', 'AbortError'))
      controller.signal.addEventListener('abort', cancel, { once: true })
    })
    const timeout = setTimeout(() => { timedOut = true; controller.abort() }, this.options.timeoutMs ?? 3000)
    try {
      const workspace = await Promise.race([this.options.request(controller.signal), aborted])
      if (!this.running || !this.visible || identity !== this.identity) return
      this.failures = 0
      this.options.onworkspace(workspace)
      this.options.onconnection({ kind: 'connected' })
    } catch (error) {
      if (!this.running || !this.visible || identity !== this.identity) return
      this.failures += 1
      nextDelay = Math.min((this.options.intervalMs ?? 500) * 2 ** Math.min(this.failures, 5), this.options.maxBackoffMs ?? 8000)
      this.options.onconnection({ kind: 'reconnecting', retryInMs: nextDelay, message: timedOut ? 'The workspace status request timed out.' : error instanceof Error ? error.message : 'The local service could not be reached.' })
    } finally {
      clearTimeout(timeout); controller.signal.removeEventListener('abort', cancel)
      this.active = false
      if (this.controller === controller) this.controller = null
      if (this.running && this.visible) this.timer = setTimeout(() => void this.poll(), this.immediate ? 0 : nextDelay)
    }
  }
}
