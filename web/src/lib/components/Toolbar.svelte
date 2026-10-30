<script lang="ts">
  import type { ExecutionView, Filter } from '../view-state'
  let { search, filter, view, onsearch, onfilter, onview }: {
    search: string; filter: Filter; view: ExecutionView
    onsearch: (value: string) => void; onfilter: (value: Filter) => void; onview: (value: ExecutionView) => void
  } = $props()
</script>

<div class="toolbar" aria-label="Execution controls">
  <label class="search"> <span class="visually-hidden">Search observation metadata</span>
    <input type="search" maxlength="512" value={search} oninput={(event) => onsearch(event.currentTarget.value)} placeholder="Search metadata…" aria-describedby="search-hint" />
  </label>
  <span id="search-hint" class="visually-hidden">Searches display metadata only. Maximum 256 characters.</span>
  <div class="filter-group" aria-label="Structural filter">
    <button class="filter" type="button" aria-pressed={filter === 'error'} onclick={() => onfilter(filter === 'error' ? '' : 'error')}>Errors</button>
    <button class="filter" type="button" aria-pressed={filter === 'tool'} onclick={() => onfilter(filter === 'tool' ? '' : 'tool')}>Tools</button>
    <button class="filter" type="button" aria-pressed={filter === 'generation'} onclick={() => onfilter(filter === 'generation' ? '' : 'generation')}>Generations</button>
    <button class="filter" type="button" aria-pressed={filter === 'slow'} onclick={() => onfilter(filter === 'slow' ? '' : 'slow')}>Over 5s</button>
  </div>
  <div class="segmented toolbar-end" aria-label="Execution view">
    <button type="button" aria-pressed={view === 'waterfall'} onclick={() => onview('waterfall')}>Waterfall</button>
    <button type="button" aria-pressed={view === 'list'} onclick={() => onview('list')}>List</button>
  </div>
</div>
