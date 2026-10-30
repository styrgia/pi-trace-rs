<script lang="ts">
  import { onMount } from 'svelte'
  import { SystemController, type SystemState } from '../system-controller'
  import { sessionApi, type CatalogItem } from '../trace-api'
  import { sourceLabel } from '../presentation'
  import { readViewState } from '../view-state'
  import Workbench from './Workbench.svelte'

  let model = $state<SystemState | null>(null)
  let provider = $state('')
  let project = $state('')
  let theme = $state(readViewState(new URL(window.location.href)).theme)
  let controller: SystemController | null = null
  const providers = [{ id: 'codex', label: 'Codex' }, { id: 'pi', label: 'Pi Agent' }, { id: 'claude', label: 'Claude Code' }, { id: 'opencode', label: 'OpenCode' }]
  const selectedExcluded = $derived(model?.selected && ((provider && provider !== model.selected.provider) || (project && project !== model.selected.project)))
  const target = $derived(model?.workspace?.snapshotId ? { snapshotId: model.workspace.snapshotId, runId: model.workspace.runId, revision: model.workspace.revision } : null)
  const boundApi = $derived(model?.selectedId ? sessionApi(model.selectedId, model.selected ?? undefined) : null)
  const initialView = $derived.by(() => { const url = new URL(window.location.href); const view = readViewState(url); if (url.searchParams.get('session') !== model?.selectedId || (model?.overview && view.snapshot !== model.overview.snapshotId)) return { ...view, snapshot: null, node: null }; return view })
  onMount(() => {
    controller = new SystemController({ initialSession: new URL(window.location.href).searchParams.get('session'), onchange: (next) => model = next, oninvalidate: clearInvestigationReferences })
    const visibility = () => controller?.setVisible(document.visibilityState !== 'hidden')
    visibility(); controller.start()
    document.addEventListener('visibilitychange', visibility)
    return () => { controller?.stop(); document.removeEventListener('visibilitychange', visibility) }
  })
  function clearInvestigationReferences() { const url = new URL(window.location.href); url.searchParams.delete('snapshot'); url.searchParams.delete('node'); window.history.replaceState(null, '', url) }
  function choose(item: CatalogItem) {
    const url = new URL(window.location.href)
    url.search = ''; url.hash = ''; url.searchParams.set('session', item.id); url.searchParams.set('theme', document.documentElement.dataset.theme ?? theme)
    window.history.replaceState(null, '', url)
    controller?.select(item)
  }
  function changeFilters() { controller?.filter(provider, project) }
  function changeTheme() { theme = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark'; document.documentElement.dataset.theme = theme; const url = new URL(window.location.href); url.searchParams.set('theme', theme); window.history.replaceState(null, '', url) }
  function timestamp(value: number | null) { return value === null ? 'Unknown' : new Date(value).toLocaleString() }
</script>

<a class="skip" href="#session-catalog">Skip to sessions</a>
<div class="app system-app">
  <header class="topbar">
    <a class="brand" href="/" aria-label="Pi Trace home"><span class="brand-mark" aria-hidden="true">π</span><span class="brand-wordmark">Pi Trace</span></a>
    <span class="muted">System sessions</span>
    {#if !model?.overview}<button class="quiet-button" type="button" onclick={changeTheme} aria-label={`Switch to ${theme === 'light' ? 'dark' : 'light'} theme`}>{theme === 'light' ? 'Dark' : 'Light'}</button>{/if}
  </header>
  <section class="sheet session-browser" aria-labelledby="session-catalog">
    <div class="catalog-heading"><div><p class="eyebrow">Native local stores</p><h1 id="session-catalog">Sessions</h1></div><button type="button" class="quiet-button" onclick={() => controller?.resync()}>Refresh sessions</button></div>
    <p class="muted">Following persisted records only. Unsaved streaming output is unavailable. Storage changes do not establish whether an agent process is running.</p>
    <div class="catalog-filters">
      <label>Provider<select bind:value={provider} onchange={changeFilters}><option value="">All providers</option>{#each providers as option}<option value={option.id}>{option.label}</option>{/each}</select></label>
      <label>Project<input type="search" bind:value={project} onchange={changeFilters} aria-describedby="project-filter-help" placeholder="Exact project metadata" /></label>
      <span id="project-filter-help" class="muted">Project matches recorded metadata exactly. Filters remain in this tab.</span>
    </div>
    {#if model?.status}
      <div class="catalog-coverage"><span>Catalog coverage: <strong>{model.catalog?.coverage.state ?? model.status.coverage.state}</strong>. {model.status.coverage.retainedEntries.toLocaleString()} retained sessions.</span><span>Omitted sessions: {model.status.coverage.omittedCount === null ? 'unknown' : model.status.coverage.omittedCount}.</span>{#each model.status.coverage.limits as limit}<span>Limit: {limit}</span>{/each}</div>
      <details class="inspection-disclosure"><summary>Provider stores and discovery limits</summary>
        <ul class="store-list">{#each model.status.stores as store (store.id)}<li><strong>{providers.find((item) => item.id === store.provider)?.label ?? store.provider}</strong> · {store.state} · store {store.id} · {store.visited.toLocaleString()} visited · {store.sweepComplete ? 'sweep complete' : 'sweep in progress'}<p class="source">Configured root: {store.root}</p>{#if store.error}<p>{store.error.code}: {store.error.message}{store.error.source ? ` (${sourceLabel(store.error.source)})` : ''}</p>{/if}</li>{/each}</ul>
        {#if model.status.disabledProviders.length}<p>Disabled providers: {model.status.disabledProviders.join(', ')}</p>{/if}
        <p class="muted">Discovery per pass: {model.status.limits.discoveryEntriesPerPass.toLocaleString()}. Catalog bound: {model.status.limits.catalogEntries.toLocaleString()}. Retained analyses: {model.status.limits.analyses}. Session records: {model.status.limits.sessionRecords.toLocaleString()}. Decoded session bytes: {model.status.limits.sessionDecodedBytes.toLocaleString()}.</p>
      </details>
    {/if}
    {#if model?.statusError}<p class="inline-error" role="status">System status unavailable. {model.statusError} <button type="button" class="quiet-button" onclick={() => controller?.resync()}>Reconnect status</button></p>{/if}
    {#if model?.catalogError}<p class="inline-error" role="status">{model.catalogError} <button type="button" class="quiet-button" onclick={() => controller?.resync()}>Retry catalog</button></p>{/if}
    <div class="catalog-results" aria-busy={model?.catalogPending ?? true}>
      {#if model?.catalog}
        <p class="muted">{model.catalog.items.length.toLocaleString()} sessions in this view · catalog revision {model.catalog.revision}{model.catalogPending ? ' · refreshing' : ''}.</p>
        {#if model.catalog.items.length === 0}<p>No sessions match the current filters. Discovery may still be in progress.</p>{/if}
        <ul class="session-list">{#each model.catalog.items as item (item.id)}<li><button type="button" class="session-choice" aria-pressed={model.selectedId === item.id} onclick={() => choose(item)}><strong>{providers.find((value) => value.id === item.provider)?.label ?? item.provider}</strong><span class="session-native-id">{item.nativeId}</span><span>{item.project ?? 'Project unavailable'}</span><span class="muted">{item.ingestionState} · observed {timestamp(item.lastObservedMs)} · source activity {timestamp(item.activityMs)} · process unknown</span></button></li>{/each}</ul>
        {#if model.catalog.nextCursor}<button type="button" class="quiet-button" disabled={model.catalogPending || model.catalog.items.length >= 2000} onclick={() => controller?.more()}>Load more sessions</button>{#if model.catalog.items.length >= 2000}<p>Retained browser results are limited to 2,000 sessions. Narrow the filters to continue.</p>{/if}{/if}
      {:else}<p>{model?.catalogError ? 'Catalog unavailable.' : 'Discovering persisted sessions…'}</p>{/if}
    </div>
    {#if model?.selectedId}
      <div class="selected-session"><h2>Selected session</h2><p>{model.selected?.provider ?? 'Native'} · {model.selected?.nativeId ?? model.selectedId} · {model.selected?.project ?? 'Project unavailable'}</p>
        {#if selectedExcluded}<p class="selection-notice">The selected session is outside the current filters. Its investigation remains open.</p>{:else if model.catalog && !model.catalog.items.some((item) => item.id === model?.selectedId)}<p class="selection-notice">The selected session is outside this catalog page. Its investigation remains open.</p>{/if}
        {#if model.workspace}
          <p class="session-freshness">{model.workspace.state} · {model.workspace.progress} · persisted revision {model.workspace.revision} · process {model.workspace.processState}{model.connection.kind === 'suspended' ? ' · updates suspended while hidden' : ''}</p>
          <p class="muted">Profile {model.workspace.capabilities.profile}. Messages {model.workspace.capabilities.messages}; usage {model.workspace.capabilities.usage}; execution parents {model.workspace.capabilities.executionParents}; timing {model.workspace.capabilities.timing}.</p>
          {#if model.workspace.error}<p class="inline-error" role="status">{model.workspace.error.code}: {model.workspace.error.message}{model.workspace.error.source ? ` (${sourceLabel(model.workspace.error.source)})` : ''}. {model.overview ? 'Showing the last valid evidence.' : ''} <button type="button" class="quiet-button" onclick={() => controller?.retry()}>Retry session</button></p>{/if}
        {:else if model.sessionPresence === 'unavailable'}<p>Session unavailable. Its selected identity is retained for lookup and recovery.</p>{:else}<p>Loading selected session…</p>{/if}
        {#if model.connection.kind === 'reconnecting'}<p role="status">Reconnecting. {model.connection.message} <button type="button" class="quiet-button" onclick={() => controller?.resync()}>Reconnect session</button></p>{/if}
        {#if model.sessionError}<p class="inline-error" role="status">{model.sessionError} <button type="button" class="quiet-button" onclick={() => controller?.resync()}>Retry investigation</button></p>{/if}
      </div>
    {/if}
  </section>
  {#if model?.overview && model.selectedId && target && boundApi}
    {#key model.selectedId}<Workbench overview={model.overview} initialRevision={model.initialRevision} {initialView} {target} api={boundApi} sessionId={model.selectedId} live={undefined} connection={model.connection} controlPending={false} controlError={null} resetNotice={null} oncontrol={() => controller?.retry()} onreconnect={() => controller?.resync()} ondisconnect={() => controller?.resync()} onworkspace={() => controller?.resync()} embedded />{/key}
  {:else if !model?.selectedId}<div class="sheet"><p class="muted">Select a session to inspect Rust-projected observations, exact usage and source evidence.</p></div>{/if}
</div>
