<script lang="ts">
  import { sourceLabel } from '../presentation'
  import type { Overview } from '../trace-api'
  import { quantity, timing, tokens } from '../presentation'
  let { overview, oncontext }: { overview: Overview; oncontext: () => void } = $props()
</script>

<section class="intro" aria-labelledby="recording-title">
  <div class="title-line">
    <h1 id="recording-title">{overview.title}</h1>
    <span class="pill success">Snapshot ready</span>
  </div>
  <div class="trace-meta">
    <dl class="stats">
      <div><dt>Duration</dt><dd>{timing(overview.duration)} <span class="muted">{overview.duration.state}</span></dd></div>
      <div><dt>Observations</dt><dd>{quantity(overview.nodeCount)}</dd></div>
      <div><dt>Generations / tools</dt><dd>{overview.generationCount} / {overview.toolCount}</dd></div>
      <div><dt>Recording tokens</dt><dd>{tokens(overview.tokenAccounting.totalTokens)} <span class="muted">{overview.tokenAccounting.provenance}</span></dd></div>
      <div><dt>Diagnostics</dt><dd>{quantity(overview.diagnosticCount)}</dd></div>
    </dl>
    <div class="summary-actions">
    <button class="quiet-button" type="button" onclick={oncontext}>Inspect context</button>
    <details class="session-info">
      <summary>Session info</summary>
      <dl class="metadata session-info-panel">
        <div><dt>Snapshot</dt><dd>{overview.snapshotId}</dd></div>
        <div><dt>Source</dt><dd>{overview.source ? `${sourceLabel(overview.source)}` : 'Unknown'}</dd></div>
        <div><dt>Model</dt><dd>{overview.model ?? 'Unknown'}</dd></div>
        <div><dt>Sessions / sources</dt><dd>{overview.sessionCount} / {overview.sourceCount}</dd></div>
        <div><dt>Physical records</dt><dd>{quantity(overview.recordCount)}</dd></div>
        <div><dt>Input / output tokens</dt><dd>{tokens(overview.tokenAccounting.inputTokens)} / {tokens(overview.tokenAccounting.outputTokens)}</dd></div>
        <div><dt>Cache read / write tokens</dt><dd>{tokens(overview.tokenAccounting.cacheReadTokens)} / {tokens(overview.tokenAccounting.cacheWriteTokens)}</dd></div>
        <div><dt>Usage coverage</dt><dd>{overview.tokenAccounting.observedGenerations}/{overview.tokenAccounting.totalGenerations} observed generations, {overview.tokenAccounting.fallbackContributions} summary fallbacks</dd></div>
        <div><dt>Summary fallback contributions</dt><dd>{overview.tokenAccounting.fallbackContributions}</dd></div>
        <div><dt>Cost</dt><dd>{overview.usage.cost === null ? 'Unknown' : overview.usage.cost}</dd></div>
        <div><dt>Duration meaning</dt><dd>{overview.duration.meaning}</dd></div>
      </dl>
    </details>
    </div>
  </div>
</section>
