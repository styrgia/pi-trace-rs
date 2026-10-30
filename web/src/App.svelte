<script lang="ts">
  import { sourceLabel } from './lib/presentation'
  import { onMount } from 'svelte'
  import { traceApi, ApiError, type Overview, type Workspace, type LiveStatus, type StructuralError } from './lib/trace-api'
  import { WorkspacePoller, type ConnectionState } from './lib/workspace-poller'
  import { readViewState } from './lib/view-state'
  import SystemWorkbench from './lib/components/SystemWorkbench.svelte'
  import Workbench from './lib/components/Workbench.svelte'
  import LoadState from './lib/components/LoadState.svelte'
  type WorkspaceLoad =
    | { kind: 'system' }
    | { kind: 'loading' }
    | { kind: 'empty' }
    | { kind: 'importing'; attemptId: string }
    | { kind: 'failed'; attemptId: string; error: StructuralError }
    | { kind: 'ready'; attemptId: string; overview: Overview; runId: string | null; revision: number | null }
    | { kind: 'unavailable'; message: string }
    | { kind: 'stale' }
  let workspaceLoad = $state<WorkspaceLoad>({ kind: 'loading' })
  let initialView = $state(readViewState(new URL(window.location.href)))
  let target = $state<{ snapshotId: string; runId: string | null; revision: number | null } | null>(null)
  let live = $state<LiveStatus | undefined>(undefined)
  let connection = $state<ConnectionState>({ kind: 'loading' })
  let controlPending = $state(false)
  let controlError = $state<string | null>(null)
  let resetNotice = $state<string | null>(null)
  let knownRun: string | null = null
  let poller: WorkspacePoller | null = null
  let controller: AbortController | null = null
  let controlController: AbortController | null = null
  let loadingSnapshot: string | null = null
  let generation = 0
  let alive = true

  onMount(() => {
    document.documentElement.dataset.theme = initialView.theme
    poller = new WorkspacePoller({ request: traceApi.workspace, onworkspace: (workspace) => { void accept(workspace) }, onconnection: (state) => {
      connection = state
      if (state.kind === 'reconnecting' && workspaceLoad.kind !== 'ready') workspaceLoad = { kind: 'unavailable', message: state.message }
    } })
    const visibility = () => poller?.setVisible(document.visibilityState !== 'hidden')
    visibility(); poller.start()
    document.addEventListener('visibilitychange', visibility)
    return () => { alive = false; ++generation; controller?.abort(); controlController?.abort(); poller?.stop(); document.removeEventListener('visibilitychange', visibility) }
  })
  function clearSelection() {
    const url = new URL(window.location.href)
    url.searchParams.delete('snapshot'); url.searchParams.delete('node')
    window.history.replaceState(null, '', url)
    initialView = readViewState(url)
  }
  async function accept(workspace: Workspace) {
    if (!alive) return
    if (workspace.state === 'system') { controller?.abort(); ++generation; poller?.stop(); workspaceLoad = { kind: 'system' }; return }
    live = workspace.live
    if (live) {
      if (knownRun !== null && knownRun !== live.runId) {
        clearSelection(); resetNotice = 'A new live run started. The previous selection was cleared.'
        controller?.abort(); ++generation; loadingSnapshot = null
        workspaceLoad = { kind: 'loading' }
      }
      knownRun = live.runId
    }
    if (workspace.state === 'empty') { workspaceLoad = initialView.snapshot ? { kind: 'stale' } : { kind: 'empty' }; if (!live) poller?.stop(); return }
    if (workspace.state === 'failed') { workspaceLoad = { kind: 'failed', attemptId: workspace.attemptId, error: workspace.error }; if (!live) poller?.stop(); return }
    if (workspace.state === 'importing') { workspaceLoad = { kind: 'importing', attemptId: workspace.attemptId }; return }
    const runId = live?.runId ?? null
    target = { snapshotId: workspace.snapshotId, runId, revision: live?.revision ?? null }
    if (workspaceLoad.kind === 'ready' && workspaceLoad.runId === runId) {
      if (!live && workspaceLoad.overview.snapshotId !== workspace.snapshotId) workspaceLoad = { kind: 'stale' }
      return
    }
    if (initialView.snapshot && initialView.snapshot !== workspace.snapshotId) { workspaceLoad = { kind: 'stale' }; return }
    if (loadingSnapshot === workspace.snapshotId) return
    controller?.abort()
    const request = new AbortController(); controller = request
    const identity = ++generation
    loadingSnapshot = workspace.snapshotId
    try {
      const overview = await traceApi.overview(workspace.snapshotId, request.signal)
      if (alive && !request.signal.aborted && identity === generation) {
        workspaceLoad = { kind: 'ready', attemptId: workspace.attemptId, overview, runId, revision: workspace.live?.revision ?? null }
        if (!live) poller?.stop()
      }
    } catch (error) {
      if (alive && !request.signal.aborted && identity === generation) {
        if (error instanceof ApiError && error.structural.code === 'unknown_snapshot') poller?.resync()
        else workspaceLoad = { kind: 'unavailable', message: error instanceof Error ? error.message : 'The local service could not be reached.' }
      }
    } finally { if (identity === generation) loadingSnapshot = null }
  }
  function reconnect() { if (!controlPending) poller?.start() }
  function disconnect(message: string) {
    if (live && workspaceLoad.kind === 'ready') { connection = { kind: 'reconnecting', message, retryInMs: 500 }; poller?.resync() }
    else { controller?.abort(); ++generation; initialView = readViewState(new URL(window.location.href)); workspaceLoad = { kind: 'unavailable', message } }
  }
  async function control(action: 'pause' | 'resume' | 'retry') {
    if (controlPending) return
    controlPending = true; controlError = null
    poller?.stop()
    controlController?.abort()
    const request = new AbortController(); controlController = request
    const timeout = setTimeout(() => { request.abort(); controlError = 'The workspace control timed out. Reconnecting to discover its state.' }, 10000)
    try {
      const workspace = await traceApi[action](request.signal)
      if (alive && !request.signal.aborted) await accept(workspace)
    } catch (error) {
      if (alive && !request.signal.aborted) controlError = error instanceof Error ? error.message : 'The workspace control failed.'
    } finally { clearTimeout(timeout); if (alive) { controlPending = false; reconnect() } }
  }
  function currentWorkspace() { clearSelection(); workspaceLoad = { kind: 'loading' }; reconnect() }
</script>

<svelte:head><meta name="description" content="Investigate a local Pi recording with source-linked measurements, execution views and diagnostics." /></svelte:head>
{#if workspaceLoad.kind === 'system'}
  <SystemWorkbench />
{:else if workspaceLoad.kind === 'ready'}
  <Workbench overview={workspaceLoad.overview} initialRevision={workspaceLoad.revision} target={target ?? { snapshotId: workspaceLoad.overview.snapshotId, runId: workspaceLoad.runId, revision: live?.revision ?? null }} {live} {connection} {controlPending} {controlError} {resetNotice} {initialView} oncontrol={control} onreconnect={reconnect} ondisconnect={disconnect} onworkspace={() => poller?.resync()} />
{:else}
  <div class="app">
    <header class="topbar"><a class="brand" href="/" aria-label="Pi Trace home"><span class="brand-mark" aria-hidden="true">π</span><span class="brand-wordmark">Pi Trace</span></a><span class="muted">Local workspace</span></header>
    <main class="sheet">
      {#if resetNotice}<p role="status">{resetNotice}</p>{/if}
      {#if workspaceLoad.kind === 'failed'}<LoadState kind="failed" message={workspaceLoad.error.message} code={workspaceLoad.error.code} source={workspaceLoad.error.source ? `${sourceLabel(workspaceLoad.error.source)}` : ''} onaction={() => control('retry')} />
      {:else if workspaceLoad.kind === 'unavailable'}<LoadState kind="unavailable" message={workspaceLoad.message} onaction={reconnect} />
      {:else if workspaceLoad.kind === 'stale'}<LoadState kind="stale" onaction={currentWorkspace} />
      {:else}<LoadState kind={workspaceLoad.kind} />{/if}
      {#if controlError}<p role="alert">{controlError}</p>{/if}
      <p class="visually-hidden" role="status">{workspaceLoad.kind === 'importing' ? 'Import in progress. Waiting for a completed snapshot.' : workspaceLoad.kind === 'loading' ? 'Connecting to local workspace.' : ''}</p>
    </main>
  </div>
{/if}
