<script lang="ts">
  import { sourceLabel } from '../presentation'
  import type { Observation, ObservationPage, Overview } from '../trace-api'
  import type { ExecutionView } from '../view-state'
  import { milliseconds, timing, tokens } from '../presentation'
  let { page, overview, retainedBound, selected, collapsed, filtered, view, pending, error, onselect, oncollapse, onmore, onretry }: {
    page: ObservationPage | null; overview: Overview; retainedBound: boolean; selected: string | null; collapsed: Set<string>; filtered: boolean
    view: ExecutionView; pending: boolean; error: string | null
    onselect: (id: string) => void; oncollapse: (id: string) => void; onmore: () => void; onretry: () => void
  } = $props()
  let region: HTMLElement
  const visible = $derived.by(() => {
    const rows: Observation[] = []
    const hidden = new Set<string>()
    for (const row of page?.items ?? []) {
      if (!filtered && row.parentId && (collapsed.has(page?.items.find((item) => item.id === row.parentId)?.spanId ?? '') || hidden.has(row.parentId))) { hidden.add(row.id); continue }
      rows.push(row)
    }
    return rows
  })
  function navigate(event: KeyboardEvent) {
    if (!(event.target instanceof HTMLButtonElement) || !event.target.matches('.select-row')) return
    const buttons = Array.from(region.querySelectorAll<HTMLButtonElement>('.select-row'))
    const index = buttons.indexOf(event.target)
    let next: HTMLButtonElement | undefined
    if (event.key === 'ArrowDown') next = buttons[index + 1]
    else if (event.key === 'ArrowUp') next = buttons[index - 1]
    else if (event.key === 'Home') next = buttons[0]
    else if (event.key === 'End') next = buttons.at(-1)
    else return
    event.preventDefault()
    next?.focus()
  }
  function barStyle(row: Observation): string {
    if (!row.bar || !overview.timeline || overview.timeline.durationMs <= 0) return ''
    return `--start: ${Math.max(0, row.bar.offsetMs / overview.timeline.durationMs * 100)}%; --size: ${Math.max(0, row.bar.durationMs / overview.timeline.durationMs * 100)}%`
  }
</script>

<section class="execution" aria-labelledby="execution-heading" aria-busy={pending} bind:this={region}>
  <div class="ruler">
    <div class="tree-title"><h2 id="execution-heading">Execution</h2></div>
    <div class="ruler-ticks" aria-label="Global timeline scale">
      {#if view === 'waterfall' && overview.timeline}
        <span>0</span><span>{milliseconds(overview.timeline.durationMs / 2)}</span><span>{milliseconds(overview.timeline.durationMs)}</span>
      {:else if view === 'list'}<span>Outcome</span><span>Source</span>
      {:else}<span>Timing unavailable</span>{/if}
    </div>
  </div>
  {#if page}
    <p class="result-count" role="status">{page.totalMatches} matches, {page.totalRows - page.totalMatches} context. {page.items.length} of {page.totalRows} rows loaded.{#if page.nextCursor} More results remain.{/if}</p>
    <div class="trace-rows" class:list-view={view === 'list'}>
      {#each visible as row (row.spanId)}
        <div class="trace-row" class:selected={selected === row.id} class:context={row.context} data-type={row.kind} data-span-id={row.spanId} data-snapshot-id={page.snapshotId} data-token-total={row.spanUsage.inclusive.totalTokens.value ?? 'null'} data-token-total-state={row.spanUsage.inclusive.totalTokens.state}>
          <div class="row-name" style={`--depth: ${Math.min(row.depth, 8)}`}>
            {#if row.hasChildren}
              <button type="button" class="disclosure" aria-label={`${collapsed.has(row.spanId) && !filtered ? 'Expand' : 'Collapse'} ${row.label}`} aria-expanded={filtered || !collapsed.has(row.spanId)} disabled={filtered} onclick={() => oncollapse(row.spanId)}>{collapsed.has(row.spanId) && !filtered ? '›' : '⌄'}</button>
            {:else}<span class="disclosure-spacer"></span>{/if}
            <button type="button" class="select-row" aria-pressed={selected === row.id} onclick={() => onselect(row.id)} onkeydown={navigate} title={`${row.label}${row.labelTruncated ? ' (label truncated)' : ''}. ${row.kind}. ${row.duration.state} duration. ${sourceLabel(row.source)}`}>
              <span class="kind-mark" class:failed={row.outcome === 'error'} aria-hidden="true">{row.outcome === 'error' ? '!' : row.kind === 'generation' ? '◇' : row.kind === 'tool' ? '↳' : '▱'}</span>
              <span class="row-labels"><span class="node-name">{row.label}{row.labelTruncated ? '…' : ''}</span><span class="row-tokens" aria-label={`Inclusive tokens: ${tokens(row.spanUsage.inclusive.totalTokens)}`}>Inclusive {tokens(row.spanUsage.inclusive.totalTokens)}</span></span>
              {#if row.context}<span class="context-label">context</span>{/if}
              <span class="duration">{timing(row.duration)}</span>
              <span class="visually-hidden">{row.kind}, {row.outcome}</span>
            </button>
          </div>
          <div class="row-chart">
            {#if view === 'list'}
              <span class:attention={row.outcome === 'error'}>{row.outcome}</span>
              <span class="row-source">{sourceLabel(row.source)}</span>
            {:else if row.bar && overview.timeline && overview.timeline.durationMs > 0}
              <div class="chart-track" aria-label={`${row.duration.state} interval`}><span class={`span-bar ${row.kind}`} class:failed={row.outcome === 'error'} style={barStyle(row)}></span></div>
            {:else}<span class="unavailable-timing">No valid interval</span>{/if}
          </div>
        </div>
      {/each}
    </div>
    {#if page.totalRows === 0}<div class="empty-state"><h3>{overview.nodeCount === 0 ? 'This recording is empty' : 'No matching observations'}</h3><p>{overview.nodeCount === 0 ? 'The source was imported successfully and contains no observations.' : 'Change the metadata search or structural filter.'}</p></div>{/if}
  {:else if pending}<p class="inline-state" role="status">Loading observations…</p>{/if}
  {#if error}<div class="inline-error" role="alert"><p>{error}</p><button class="quiet-button" type="button" onclick={onretry}>Retry observations</button></div>{/if}
  {#if page?.nextCursor}<div class="continuation"><button class="quiet-button" type="button" disabled={pending || retainedBound} onclick={onmore}>{pending ? 'Loading…' : 'Load more observations'}</button><p>{retainedBound ? 'The retained view is limited to 2,000 rows. Selection remains available through independent detail lookup.' : 'Loaded pages are appended in source hierarchy order.'}</p></div>{/if}
  <div class="legend"><span><i class="swatch generation"></i>Generation</span><span><i class="swatch tool"></i>Tool</span><span><i class="swatch failed"></i>Failed</span><span>Inclusive tokens include nested spans. These totals are non-additive.</span><span>Unknown intervals stay visible without a bar.</span></div>
</section>
