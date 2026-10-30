<script lang="ts">
  import { sourceLabel } from '../presentation'
  import type { DiagnosticPage } from '../trace-api'
  let { page, pending, error, related, onselect, onmore, onretry }: {
    page: DiagnosticPage | null; pending: boolean; error: string | null; related: string[]
    onselect: (id: string) => void; onmore: () => void; onretry: () => void
  } = $props()
</script>
<section class="diagnostics" aria-labelledby="diagnostics-heading" aria-busy={pending}>
  <h2 id="diagnostics-heading" tabindex="-1">Recording diagnostics</h2>
  <p class="muted">Structural issues preserve their original source and line.</p>
  {#if related.length > 0}<p class="selection-notice">The selected observation references {related.length} diagnostics. Related loaded entries are marked below; more may be available on later pages.</p>{/if}
  {#if page}
    <p class="result-count" role="status">{page.items.length} of {page.total} diagnostics loaded.{#if page.nextCursor} More remain.{/if}</p>
    {#each page.items as item (item.id)}
      {@const nodeId = item.nodeId}
      <article class="diagnostic" class:related={related.includes(item.id)}>
        <div><h3>{item.code}{related.includes(item.id) ? ' (selected observation)' : ''}</h3><p>{item.message}</p><p class="source">{sourceLabel(item.source)}</p></div>
        {#if nodeId}<button class="quiet-button" type="button" onclick={() => onselect(nodeId)}>Inspect observation</button>{/if}
      </article>
    {/each}
    {#if page.total === 0}<p class="inline-state">No structural diagnostics were reported.</p>{/if}
    {#if page.nextCursor}<button class="quiet-button" type="button" disabled={pending} onclick={onmore}>{pending ? 'Loading…' : 'Load more diagnostics'}</button>{/if}
  {/if}
  {#if pending && !page}<p class="inline-state" role="status">Loading diagnostics…</p>{/if}
  {#if error}<p role="alert">{error}</p><button class="quiet-button" type="button" onclick={onretry}>Retry diagnostics</button>{/if}
</section>
