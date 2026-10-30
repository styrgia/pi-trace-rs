<script lang="ts">
  import { sourceLabel } from '../presentation'
  import type { SpanUsageDetail } from '../trace-api'
  import { tokens } from '../presentation'
  let { usage, native = false }: { usage: SpanUsageDetail; native?: boolean } = $props()
  const labels = [
    { key: 'inputTokens', label: 'Input' }, { key: 'outputTokens', label: 'Output' },
    { key: 'cacheReadTokens', label: 'Cache read' }, { key: 'cacheWriteTokens', label: 'Cache write' }, { key: 'totalTokens', label: 'Total' },
  ] satisfies { key: 'inputTokens' | 'outputTokens' | 'cacheReadTokens' | 'cacheWriteTokens' | 'totalTokens'; label: string }[]
  const provenance = { none: 'No recorded contribution', recordedGeneration: 'Recorded generation', turnSummaryFallback: 'Turn-summary fallback', aggregate: 'Aggregate contributions', mixed: 'Mixed contributions', nativeResponse: 'Recorded native model response', nativeAuxiliary: 'Recorded native auxiliary contribution', derivedCumulative: 'Derived comparable cumulative difference' }
</script>

<section class="span-accounting" aria-label="Own and inclusive token accounting">
  <h4>Token accounting</h4>
  <p class="muted">Own usage belongs to this span. Inclusive usage adds verified descendants. Nested totals are non-additive.</p>
  <div class="accounting-table-wrap">
    <table class="accounting-table">
      <thead><tr><th scope="col">Tokens</th><th scope="col">Own</th><th scope="col">Inclusive</th></tr></thead>
      <tbody>{#each labels as component}<tr><th scope="row">{component.label}</th><td>{tokens(usage.own[component.key])}</td><td>{tokens(usage.inclusive[component.key])}</td></tr>{/each}</tbody>
    </table>
  </div>
  <dl class="metadata">
    <div><dt>Own provenance</dt><dd>{provenance[usage.own.provenance]}</dd></div>
    <div><dt>Inclusive provenance</dt><dd>{provenance[usage.inclusive.provenance]}</dd></div>
    <div><dt>Generation coverage</dt><dd>{usage.inclusive.observedGenerations}/{usage.inclusive.totalGenerations} observed</dd></div>
    <div><dt>Summary fallbacks</dt><dd>{usage.inclusive.fallbackContributions}</dd></div>
    <div><dt>Child attribution</dt><dd>{usage.childAttribution}</dd></div>
  </dl>
  {#if !native && usage.own.totalTokens.state === 'notApplicable'}<p class="muted">This format records model usage on generations and eligible turn summaries. Tools and other containers have no independent model contribution.</p>{/if}
  {#if usage.evidence.length}
    <details class="inspection-disclosure"><summary>Contributor evidence ({usage.evidence.length} of {usage.contributorCount})</summary>
      <ol class="usage-evidence">{#each usage.evidence as entry (entry.contributionId)}<li><span>{sourceLabel(entry.source)}</span><span>{provenance[entry.accounting.provenance]} · {tokens(entry.accounting.totalTokens)} tokens</span><span>Reported total: {tokens(entry.reportedTotal)}</span></li>{/each}</ol>
      {#if usage.omittedCount}<p class="truncation">{usage.omittedCount} contributors omitted from this bounded sample. Totals include every accepted contribution.</p>{/if}
    </details>
  {/if}
</section>
