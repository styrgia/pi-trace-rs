<script lang="ts">
  import { sourceLabel } from '../presentation'
  import { onMount, tick, untrack } from 'svelte'
  import { traceApi, ApiError, type Overview, type ObservationPage, type DiagnosticPage, type LiveStatus, type WorkbenchApi } from '../trace-api'
  import { readViewState, viewUrl, type ViewState, type Filter, type ExecutionView, type InspectorTab } from '../view-state'
  import Summary from './Summary.svelte'
  import Toolbar from './Toolbar.svelte'
  import Execution from './Execution.svelte'
  import Inspector, { type DetailState, type ContentState } from './Inspector.svelte'
  import Diagnostics from './Diagnostics.svelte'
  import { loadSnapshotView, RETAINED_ROWS, type Selection } from '../snapshot-view'
  import type { ConnectionState } from '../workspace-poller'

  let { overview: initialOverview, initialRevision, initialView, target, live, connection, controlPending, controlError, resetNotice, oncontrol, onreconnect, ondisconnect, onworkspace, api = traceApi, sessionId = null, embedded = false }: {
    api?: WorkbenchApi; sessionId?: string | null; embedded?: boolean
    overview: Overview; initialRevision: number | null; initialView: ViewState
    target: { snapshotId: string; runId: string | null; revision: number | null }
    live: LiveStatus | undefined; connection: ConnectionState; controlPending: boolean; controlError: string | null; resetNotice: string | null
    oncontrol: (action: 'pause' | 'resume' | 'retry') => void; onreconnect: () => void
    ondisconnect: (message: string) => void; onworkspace: () => void
  } = $props()
  let overview = $state(untrack(() => initialOverview))
  let currentRun = $state(untrack(() => target.runId))
  let displayedRevision = $state(untrack(() => initialRevision))
  let runNotice = $state<string | null>(untrack(() => resetNotice))
  let selectedSpan = $state<string | null>(null)
  let refreshPending = $state(false)
  let refreshError = $state<string | null>(null)
  let retainedBound = $state(false)
  let refreshingSnapshot: string | null = null
  let refreshController: AbortController | null = null
  let refreshIdentity = 0
  let selected = $state(untrack(() => initialView.node))
  let filter = $state<Filter>(untrack(() => initialView.filter))
  let view = $state<ExecutionView>(untrack(() => initialView.view))
  let tab = $state<InspectorTab>(untrack(() => initialView.tab))
  let theme = $state(untrack(() => initialView.theme))
  let search = $state('')
  let collapsed = $state(new Set<string>())
  let section = $state<'execution' | 'diagnostics'>('execution')
  let page = $state<ObservationPage | null>(null)
  let queryPending = $state(false)
  let queryError = $state<string | null>(null)
  let detail = $state<DetailState>({ kind: 'idle' })
  let content = $state<ContentState>({ kind: 'hidden' })
  let diagnostics = $state<DiagnosticPage | null>(null)
  let diagnosticsPending = $state(false)
  let diagnosticsError = $state<string | null>(null)
  let queryController: AbortController | null = null
  let detailController: AbortController | null = null
  let contentController: AbortController | null = null
  let diagnosticsController: AbortController | null = null
  let queryIdentity = 0
  let contextQueryIdentity: number | null = null
  let selectedIdentity = 0
  let diagnosticsIdentity = 0
  let searchTimer: ReturnType<typeof setTimeout> | undefined
  let alive = true
  const filtered = $derived(search.length > 0 || filter !== '')
  const link = $derived(viewUrl({ snapshot: overview.snapshotId, node: selected, filter, view, tab, theme }, new URL(window.location.href), sessionId).href)
  const selectionNotice = $derived.by(() => {
    if (!selected || !page) return null
    const row = page.items.find((item) => item.id === selected)
    if (!row) return page.nextCursor ? 'Selected observation is outside the loaded results. Its details remain available.' : 'Selected observation is outside the current results. Its details remain available.'
    if (!filtered) {
      let parent = row.parentId
      while (parent) {
        if (collapsed.has(page.items.find((item) => item.id === parent)?.spanId ?? '')) return 'Selected observation is inside a collapsed branch. Its details remain available.'
        parent = page.items.find((item) => item.id === parent)?.parentId ?? null
      }
    }
    return null
  })
  function message(error: unknown): string { if (error instanceof ApiError) return `${error.structural.code}: ${error.message}`; return error instanceof Error ? error.message : 'The request failed.' }
  function handleError(error: unknown): string {
    if (error instanceof TypeError) ondisconnect('The local service could not be reached. Restore it and reconnect.')
    if (error instanceof ApiError && error.status === 404 && error.structural.code === 'unknown_snapshot') onworkspace()
    if (error instanceof ApiError && error.structural.source) return `${error.message} (${sourceLabel(error.structural.source)})`
    return message(error)
  }
  $effect(() => {
    document.documentElement.dataset.theme = theme
    window.history.replaceState(null, '', link)
  })
  $effect(() => {
    const next = target
    if (next.snapshotId !== overview.snapshotId || next.runId !== currentRun) untrack(() => { void refreshSnapshot(next) })
  })
  async function refreshSnapshot(next: typeof target, force = false) {
    if (!force && refreshingSnapshot === next.snapshotId) return
    refreshController?.abort()
    const controller = new AbortController(); refreshController = controller
    const identity = ++refreshIdentity
    refreshingSnapshot = next.snapshotId; refreshPending = true; refreshError = null
    queryController?.abort(); detailController?.abort(); diagnosticsController?.abort(); contentController?.abort()
    ++queryIdentity; ++selectedIdentity; ++diagnosticsIdentity
    queryPending = false; diagnosticsPending = false; content = { kind: 'hidden' }
    const changedRun = next.runId !== currentRun
    if (changedRun) { selected = null; selectedSpan = null; detail = { kind: 'idle' }; page = null; diagnostics = null; collapsed = new Set(); runNotice = 'A new analysis run started. The previous selection and revealed bodies were cleared.' }
    let span = changedRun ? null : selectedSpan ?? page?.items.find((row) => row.id === selected)?.spanId ?? (detail.kind === 'ready' ? detail.detail.observation.spanId : null)
    try {
      if (!changedRun && selected && !span) {
        try { span = (await api.detail(overview.snapshotId, selected, controller.signal)).observation.spanId }
        catch (error) {
          if (error instanceof ApiError && error.status === 404) runNotice = 'The previous selection could not be restored from retained evidence.'
          else throw error
        }
      }
      const selection: Selection = span ? { kind: 'span', spanId: span } : { kind: 'none' }
      const request = { snapshotId: next.snapshotId, search, filter, retainedRows: page?.items.length ?? 100, diagnosticsRows: diagnostics?.items.length ?? (section === 'diagnostics' ? 100 : 0), selection, signal: controller.signal }
      let frame
      try { frame = await loadSnapshotView(request, api) }
      catch (error) {
        if (error instanceof ApiError && error.structural.code === 'unknown_span') {
          runNotice = 'The selected span is unavailable in this run.'
          frame = await loadSnapshotView({ ...request, selection: { kind: 'none' } }, api)
        } else throw error
      }
      if (!alive || controller.signal.aborted || identity !== refreshIdentity) return
      contentController?.abort(); diagnosticsController?.abort(); ++selectedIdentity; ++diagnosticsIdentity; content = { kind: 'hidden' }
      overview = frame.overview; page = frame.page; diagnostics = frame.diagnostics
      detail = frame.detail ? { kind: 'ready', detail: frame.detail } : { kind: 'idle' }
      selected = frame.detail?.observation.id ?? null; selectedSpan = frame.detail?.observation.spanId ?? null
      retainedBound = frame.retainedBound
      currentRun = next.runId; displayedRevision = next.revision
      if (changedRun) { collapsed = new Set(); runNotice = 'A new live run started. The previous selection was cleared.' }
      queryError = null; diagnosticsError = null
    } catch (error) {
      if (alive && !controller.signal.aborted && identity === refreshIdentity) {
        refreshError = message(error)
        if (error instanceof ApiError && error.structural.code === 'unknown_snapshot') onworkspace()
      }
    } finally {
      if (identity === refreshIdentity) { refreshingSnapshot = null; refreshPending = false }
    }
  }
  function cancelRefresh() { refreshController?.abort(); ++refreshIdentity; refreshingSnapshot = null; refreshPending = false }
  onMount(() => {
    void loadQuery(false)
    if (selected) void loadDetail(selected)
    const restore = () => {
      const restored = readViewState(new URL(window.location.href))
      if (restored.snapshot !== overview.snapshotId) { onworkspace(); return }
      filter = restored.filter; view = restored.view; tab = restored.tab; theme = restored.theme
      select(restored.node, false)
      search = ''; void loadQuery(false)
    }
    window.addEventListener('popstate', restore)
    return () => {
      alive = false
      clearTimeout(searchTimer)
      queryController?.abort(); detailController?.abort(); contentController?.abort(); diagnosticsController?.abort(); refreshController?.abort()
      window.removeEventListener('popstate', restore)
    }
  })
  async function loadQuery(append: boolean, inspectContext = false) {
    if (append && (retainedBound || (page?.items.length ?? 0) >= RETAINED_ROWS)) { retainedBound = true; return }
    if (target.snapshotId !== overview.snapshotId) { void refreshSnapshot(target, true); return }
    cancelRefresh()
    queryController?.abort()
    const controller = new AbortController(); queryController = controller
    const identity = ++queryIdentity
    contextQueryIdentity = inspectContext || (append && contextQueryIdentity !== null) ? identity : null
    const snapshot = overview.snapshotId
    const cursor = append ? page?.nextCursor : null
    const previous = append ? page : null
    if (!append) { retainedBound = false }
    queryPending = true; queryError = null
    const query = new URLSearchParams()
    if (search) query.set('q', search)
    if (filter) query.set('filter', filter)
    if (cursor) query.set('cursor', cursor)
    try {
      const result = await api.observations(snapshot, query, controller.signal)
      if (!alive || controller.signal.aborted || identity !== queryIdentity || snapshot !== overview.snapshotId) return
      if (previous && previous.items.length + result.items.length > RETAINED_ROWS) { retainedBound = true; return }
      page = previous ? { ...result, items: [...previous.items, ...result.items] } : result
      retainedBound = page.items.length >= RETAINED_ROWS && page.nextCursor !== null
      if (contextQueryIdentity === identity) {
        const generation = result.items.find((item) => item.match && item.kind === 'generation')
        if (generation) { contextQueryIdentity = null; void select(generation.id, false) }
      } else if (!selected && !runNotice) {
        const match = result.items.find((item) => item.match && (filter !== 'generation' || item.kind === 'generation'))
        if (match) void select(match.id, false)
      }
    } catch (error) {
      if (alive && !controller.signal.aborted && identity === queryIdentity) queryError = handleError(error)
    } finally { if (alive && identity === queryIdentity) queryPending = false }
  }
  function changeSearch(value: string) {
    cancelRefresh(); search = value
    queryController?.abort(); ++queryIdentity; queryPending = false; queryError = null
    clearTimeout(searchTimer)
    if (Array.from(value).length > 256) { queryError = 'Metadata search is limited to 256 characters.'; return }
    searchTimer = setTimeout(() => void loadQuery(false), 180)
  }
  function inspectContext() {
    section = 'execution'; tab = 'context'; filter = 'generation'; search = ''
    clearTimeout(searchTimer)
    void loadQuery(false, true)
  }
  function changeFilter(value: Filter) { cancelRefresh(); filter = value; clearTimeout(searchTimer); void loadQuery(false) }
  function collapse(id: string) { const next = new Set(collapsed); if (next.has(id)) next.delete(id); else next.add(id); collapsed = next }
  async function select(id: string | null, focus = true) {
    cancelRefresh()
    contextQueryIdentity = null
    selected = id
    selectedSpan = page?.items.find((row) => row.id === id)?.spanId ?? (detail.kind === 'ready' && detail.detail.observation.id === id ? detail.detail.observation.spanId : null)
    contentController?.abort(); content = { kind: 'hidden' }
    if (id) void loadDetail(id)
    else { detailController?.abort(); ++selectedIdentity; detail = { kind: 'idle' } }
    const identity = selectedIdentity
    if (focus && alive) {
      await tick()
      if (alive && selected === id && selectedIdentity === identity) document.getElementById('inspector-heading')?.focus({ preventScroll: true })
    }
  }
  async function loadDetail(id: string) {
    detailController?.abort()
    const controller = new AbortController(); detailController = controller
    const identity = ++selectedIdentity
    const snapshot = overview.snapshotId
    detail = { kind: 'loading', id }
    try {
      const result = await api.detail(snapshot, id, controller.signal)
      if (alive && !controller.signal.aborted && identity === selectedIdentity && selected === id && snapshot === overview.snapshotId) { selectedSpan = result.observation.spanId; detail = { kind: 'ready', detail: result } }
    } catch (error) { if (alive && !controller.signal.aborted && identity === selectedIdentity) detail = { kind: 'failed', message: handleError(error) } }
  }
  async function reveal() {
    if (!selected) return
    contentController?.abort()
    const controller = new AbortController(); contentController = controller
    const id = selected; const snapshot = overview.snapshotId; const identity = selectedIdentity
    content = { kind: 'loading' }
    try {
      const result = await api.content(snapshot, id, controller.signal)
      if (alive && !controller.signal.aborted && selected === id && snapshot === overview.snapshotId && identity === selectedIdentity) content = { kind: 'ready', content: result }
    } catch (error) { if (alive && !controller.signal.aborted && selected === id && identity === selectedIdentity) content = { kind: 'failed', message: handleError(error) } }
  }
  async function loadDiagnostics(append: boolean) {
    if (refreshPending || target.snapshotId !== overview.snapshotId) { void refreshSnapshot(target, true); return }
    if (append && (diagnostics?.items.length ?? 0) >= RETAINED_ROWS) { diagnosticsError = 'Retained diagnostics are limited to 2,000 entries. Narrow the investigation or start again.'; return }
    diagnosticsController?.abort()
    const controller = new AbortController(); diagnosticsController = controller
    const identity = ++diagnosticsIdentity
    const snapshot = overview.snapshotId
    const previous = append ? diagnostics : null
    diagnosticsPending = true; diagnosticsError = null
    try {
      const result = await api.diagnostics(snapshot, previous?.nextCursor ?? null, controller.signal)
      if (previous && previous.items.length + result.items.length > RETAINED_ROWS) { diagnosticsError = 'The next diagnostics page exceeds the 2,000-entry retained view bound.'; return }
      if (alive && !controller.signal.aborted && identity === diagnosticsIdentity && snapshot === overview.snapshotId) diagnostics = previous ? { ...result, items: [...previous.items, ...result.items] } : result
    } catch (error) { if (alive && !controller.signal.aborted && identity === diagnosticsIdentity) diagnosticsError = handleError(error) }
    finally { if (alive && identity === diagnosticsIdentity) diagnosticsPending = false }
  }
  async function openDiagnostics() {
    section = 'diagnostics'
    if (!diagnostics && !diagnosticsPending) void loadDiagnostics(false)
    await tick(); document.getElementById('diagnostics-heading')?.focus()
  }
  async function diagnosticSelection(id: string) { section = 'execution'; await tick(); await select(id) }
</script>

<a class="skip" href="#recording-title">Skip to recording</a>
<div class="app investigation" data-session-id={sessionId ?? 'collector'} data-run-id={currentRun ?? 'offline'} data-snapshot-id={overview.snapshotId}>
  <header class="topbar">
    {#if !embedded}<a class="brand" href="/" aria-label="Pi Trace home"><span class="brand-mark" aria-hidden="true">π</span><span class="brand-wordmark">Pi Trace</span></a>{:else}<span class="muted">Session investigation</span>{/if}

    <nav aria-label="Report sections"><button type="button" class="nav-button" aria-pressed={section === 'execution'} onclick={() => section = 'execution'}>Trace</button><button type="button" class="nav-button" aria-pressed={section === 'diagnostics'} onclick={openDiagnostics}>Diagnostics <span>{overview.diagnosticCount}</span></button></nav>
    <div class="topbar-right"><a class="quiet-button" href={link}>View link</a><button class="quiet-button" type="button" onclick={() => theme = theme === 'light' ? 'dark' : 'light'} aria-label={`Switch to ${theme === 'light' ? 'dark' : 'light'} theme`}>{theme === 'light' ? 'Dark' : 'Light'}</button></div>
  </header>
  <main class="sheet">
    {#if live}
      <div class="live-status" role="status">
        <span>Live: {live.state === 'catchingUp' ? 'Catching up' : live.state === 'waiting' ? 'Waiting for appends' : live.state === 'paused' ? 'Updates paused for all tabs' : 'Stopped, showing the last valid snapshot'}. Displayed revision {displayedRevision ?? 'pending'}.</span>
        {#if refreshPending}<span>Refreshing snapshot…</span>{/if}
        {#if connection.kind === 'reconnecting'}<span>Reconnecting. {connection.message}</span><button class="quiet-button" type="button" onclick={onreconnect}>Reconnect</button>{/if}
        {#if live.state === 'stopped'}<button class="quiet-button" type="button" aria-disabled={controlPending} onclick={() => oncontrol('retry')}>Retry live run</button>
        {:else}<button class="quiet-button" type="button" aria-disabled={controlPending} onclick={() => oncontrol(live.state === 'paused' ? 'resume' : 'pause')}>{live.state === 'paused' ? 'Resume updates' : 'Pause updates'}</button>{/if}
        <span>Pause affects this workspace, not external capture.</span>
        {#if live.error}<span>{live.error.code}: {live.error.message}{live.error.source ? ` (${sourceLabel(live.error.source)})` : ''}</span>{/if}
      </div>
    {/if}
    {#if sessionId}<p class="live-status">Following persisted records · displayed revision {displayedRevision ?? 'pending'}{refreshPending ? ' · refreshing snapshot' : ''}. {connection.kind === 'suspended' ? 'Updates suspended while hidden.' : ''}</p>{/if}
    {#if runNotice}<p class="selection-notice" role="status">{runNotice}</p>{/if}
    {#if controlError}<p class="inline-error" role="alert">{controlError}</p>{/if}
    {#if refreshError}<p class="inline-error" role="alert">Snapshot refresh failed. {refreshError} <button type="button" class="quiet-button" onclick={() => { onworkspace(); void refreshSnapshot(target, true) }}>Refresh metadata</button></p>{/if}
    <Summary {overview} oncontext={inspectContext} />
    {#if section === 'execution'}
      <Toolbar {search} {filter} {view} onsearch={changeSearch} onfilter={changeFilter} onview={(value) => view = value} />
      <div class="workbench" class:inspect-expanded={tab === 'context' || (detail.kind === 'ready' && detail.detail.observation.kind === 'tool')}>
        <Execution {page} {overview} {retainedBound} {selected} {collapsed} {filtered} {view} pending={queryPending} error={queryError} onselect={select} oncollapse={collapse} onmore={() => loadQuery(true)} onretry={() => loadQuery(false)} />
        <Inspector state={detail} {content} {tab} notice={selectionNotice} ontab={(value) => tab = value} onreveal={reveal} onretry={() => selected && loadDetail(selected)} ondiagnostics={openDiagnostics} onselect={select} oncontext={inspectContext} />
      </div>
    {:else}
      <Diagnostics page={diagnostics} pending={diagnosticsPending} error={diagnosticsError} related={detail.kind === 'ready' ? detail.detail.diagnosticIds : []} onselect={diagnosticSelection} onmore={() => loadDiagnostics(true)} onretry={() => loadDiagnostics(false)} />
    {/if}
    <footer class="statusbar"><span>Local immutable snapshot</span><span>Recorded, derived and unknown measurements remain distinct.</span></footer>
  </main>
</div>
