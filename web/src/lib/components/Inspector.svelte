<script lang="ts">
  import { sourceLabel } from '../presentation'
  import type { NodeContent, NodeDetail } from '../trace-api'
  import ContextInspector from './ContextInspector.svelte'
  import ToolEvidence from './ToolEvidence.svelte'
  import NativeEvidence from './NativeEvidence.svelte'
  import SpanAccounting from './SpanAccounting.svelte'
  import type { InspectorTab } from '../view-state'
  import { timing, tokens } from '../presentation'
  export type DetailState = { kind: 'idle' } | { kind: 'loading'; id: string } | { kind: 'ready'; detail: NodeDetail } | { kind: 'failed'; message: string }
  export type ContentState = { kind: 'hidden' } | { kind: 'loading' } | { kind: 'ready'; content: NodeContent } | { kind: 'failed'; message: string }
  let { state, content, tab, notice, ontab, onreveal, onretry, ondiagnostics, onselect, oncontext }: {
    state: DetailState; content: ContentState; tab: InspectorTab; notice: string | null
    ontab: (tab: InspectorTab) => void; onreveal: () => void; onretry: () => void; ondiagnostics: () => void; onselect: (id: string) => void; oncontext: () => void
  } = $props()
</script>

<aside class="inspector" aria-labelledby="inspector-heading" data-span-id={state.kind === 'ready' ? state.detail.observation.spanId : null} data-snapshot-id={state.kind === 'ready' ? state.detail.snapshotId : null} data-token-total={state.kind === 'ready' ? state.detail.spanUsage.inclusive.totalTokens.value ?? 'null' : null} data-token-total-state={state.kind === 'ready' ? state.detail.spanUsage.inclusive.totalTokens.state : null}>
  <div class="inspector-heading"><h2 id="inspector-heading" tabindex="-1">Observation details</h2></div>
  {#if notice}<p class="selection-notice" role="status">{notice}</p>{/if}
  <div class="inspector-body" aria-busy={state.kind === 'loading'}>
    {#if state.kind === 'idle'}<p class="muted">Select an observation to inspect its measurements and source.</p>
    {:else if state.kind === 'loading'}<p role="status">Loading observation…</p>
    {:else if state.kind === 'failed'}<p role="alert">{state.message}</p><button class="quiet-button" type="button" onclick={onretry}>Retry details</button>
    {:else}
      {@const detail = state.detail}
      {@const parentId = detail.relationship.executionParentId}
      <div class="node-title"><h3>{detail.observation.label}{detail.observation.labelTruncated ? '…' : ''}</h3><span class="node-kind">{detail.observation.kind}</span></div>
      <p class="node-status" class:attention={detail.observation.outcome === 'error'}>Outcome: {detail.observation.outcome}</p>
      <dl class="detail-stats">
        <div><dt>Duration</dt><dd>{timing(detail.observation.duration)}</dd><dd class="measurement-note">{detail.observation.duration.state}</dd></div>
        <div><dt>Inclusive tokens</dt><dd>{tokens(detail.spanUsage.inclusive.totalTokens)}</dd><dd class="measurement-note">Nested, non-additive</dd></div>
      </dl>
      <div class="inspector-tabs" aria-label="Inspector view">
        <button type="button" aria-pressed={tab === 'details'} onclick={() => ontab('details')}>Details</button>
        <button type="button" aria-pressed={tab === 'context'} onclick={() => ontab('context')}>Context</button>
        <button type="button" aria-pressed={tab === 'metadata'} onclick={() => ontab('metadata')}>Metadata</button>
      </div>
      {#if tab === 'details'}
        <SpanAccounting usage={detail.spanUsage} native={'nativeUsage' in detail} />
        <NativeEvidence {detail} />
        {#if !detail.toolInspection}
        <dl class="metadata">
          <div><dt>Duration meaning</dt><dd>{detail.observation.duration.meaning}</dd></div>
          <div><dt>Request to headers</dt><dd>{timing(detail.requestToHeaders)}{detail.requestToHeaders.valueMs === null ? '' : ` (${detail.requestToHeaders.state})`}</dd></div>
          <div><dt>Header meaning</dt><dd>{detail.requestToHeaders.meaning}</dd></div>
          <div><dt>First token (TTFT)</dt><dd>{timing(detail.ttft)}{detail.ttft.valueMs === null ? '' : ` (${detail.ttft.state})`}</dd></div>
          <div><dt>TTFT meaning</dt><dd>{detail.ttft.meaning}</dd></div>
          <div><dt>Relationship</dt><dd>{detail.relationship.state}</dd></div>
          {#if parentId}<div><dt>Execution parent</dt><dd><button class="text-button" type="button" onclick={() => onselect(parentId)}>Open parent</button></dd></div>{/if}
        </dl>
        {:else if parentId}<p><button class="text-button" type="button" onclick={() => onselect(parentId)}>Open parent</button></p>{/if}
        {#if detail.toolInspection}<ToolEvidence {detail} />{/if}
        <div class="field-heading"><h4>{detail.toolInspection ? 'Arguments and recorded result' : 'Recorded input and output'}</h4></div>
        {#if content.kind === 'hidden'}
          <div class="locked-content"><p>Bodies stay hidden until requested. Previews may contain sensitive text.</p><button class="quiet-button" type="button" onclick={onreveal}>{detail.toolInspection ? 'Reveal arguments / recorded result' : 'Reveal input / output'}</button></div>
        {:else if content.kind === 'loading'}<p role="status">Loading bounded previews…</p>
        {:else if content.kind === 'failed'}<p role="alert">{content.message}</p><button class="quiet-button" type="button" onclick={onreveal}>{detail.toolInspection ? 'Retry arguments / recorded result' : 'Retry input / output'}</button>
        {:else}
          {#each [{ name: detail.toolInspection ? 'Arguments' : 'Input', field: content.content.input }, { name: detail.toolInspection ? 'Recorded result' : 'Output', field: content.content.output }] as entry}
            <div class="field-heading"><h4>{entry.name}</h4><span>{entry.field.state}</span></div>
            {#if entry.field.state === 'available'}<div class="content-box"><pre>{entry.field.text}</pre></div>{:else}<p class="muted">Not recorded.</p>{/if}
            {#if entry.field.truncated}<p class="truncation">Content is truncated. Preview limit: {entry.field.limitBytes.toLocaleString()} UTF-8 bytes.</p>{/if}
          {/each}
        {/if}
      {:else if tab === 'context'}
        {#if detail.context}<ContextInspector context={detail.context} usage={detail.spanUsage.own} />
        {:else if detail.nativeUsage}<p class="muted">Request composition is unavailable for this native profile. Recorded selected context: {detail.selectedContext ?? 'Unavailable'}.</p>
        {:else}<div class="locked-content"><p>Context composition belongs to individual recorded generation requests. These choices include all recorded generations across the trace.</p><button class="quiet-button" type="button" onclick={oncontext}>All recorded generations</button></div>{/if}
      {:else}
        <dl class="metadata">
          <div><dt>Stable span</dt><dd>{detail.observation.spanId}</dd></div>
          <div><dt>Occurrence</dt><dd>{detail.observation.id}</dd></div>
          <div><dt>Snapshot</dt><dd>{detail.snapshotId}</dd></div>
          <div><dt>Session</dt><dd>{detail.metadata.sessionId ?? 'Unknown'}</dd></div>
          <div><dt>Model</dt><dd>{detail.metadata.model ?? 'Unknown'}</dd></div>
          <div><dt>Tool</dt><dd>{detail.metadata.toolName ?? 'Unknown'}</dd></div>
          <div><dt>Turn / step index</dt><dd>{detail.metadata.turnIndex ?? 'Unknown'} / {detail.metadata.stepIndex ?? 'Unknown'}</dd></div>
          <div><dt>Input / output tokens</dt><dd>{tokens(detail.spanUsage.own.inputTokens)} / {tokens(detail.spanUsage.own.outputTokens)}</dd></div>
          <div><dt>Cache read / write tokens</dt><dd>{tokens(detail.spanUsage.own.cacheReadTokens)} / {tokens(detail.spanUsage.own.cacheWriteTokens)}</dd></div>
          <div><dt>Own generation coverage</dt><dd>{detail.spanUsage.own.observedGenerations}/{detail.spanUsage.own.totalGenerations} observed generations</dd></div>
          <div><dt>Own summary fallbacks</dt><dd>{detail.spanUsage.own.fallbackContributions}</dd></div>
          <div><dt>Cost</dt><dd>{detail.usage.cost ?? 'Unknown'}</dd></div>
        </dl>
      {/if}
      <div class="provenance"><h4>Source references</h4>{#each detail.sources as source}<p>{sourceLabel(source)}</p>{/each}</div>
      {#if detail.diagnosticIds.length > 0}<button class="text-button" type="button" onclick={ondiagnostics}>Open {detail.diagnosticIds.length} related diagnostics</button>{/if}
    {/if}
  </div>
</aside>
