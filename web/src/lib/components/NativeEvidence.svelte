<script lang="ts">
  import type { NodeDetail } from '../trace-api'
  import { sourceLabel, tokens } from '../presentation'
  let { detail }: { detail: NodeDetail } = $props()
</script>

{#if detail.nativeUsage}
  <section class="native-evidence" aria-label="Native evidence">
    <h4>Native evidence</h4>
    <p class="muted">Rust selects contributions and normalizes provider counters. Reconciliation, inherited evidence and included breakdowns are not additional contributions.</p>
    {#if detail.selectedContext !== undefined}<p>Recorded selected context: {detail.selectedContext ?? 'Unavailable'}</p>{/if}
    {#each detail.nativeUsage as evidence}
      <details class="inspection-disclosure"><summary>{evidence.role.kind} · {evidence.accepted ? 'accepted contribution' : 'not added'} · {evidence.authority}</summary>
        <dl class="metadata"><div><dt>Evidence ID</dt><dd>{evidence.id}</dd></div><div><dt>Role</dt><dd>{evidence.role.kind === 'contribution' ? evidence.role.category : evidence.role.kind === 'reconciliation' ? evidence.role.scope : evidence.role.kind === 'inherited' ? evidence.role.origin : evidence.role.includedIn}</dd></div><div><dt>Source</dt><dd>{sourceLabel(evidence.source)}</dd></div></dl>
        <h5>Native counter breakdown</h5>
        <dl class="metadata">{#each evidence.nativeCounters as counter}<div><dt>{counter.name}</dt><dd>{tokens(counter.value)}</dd><dd class="measurement-note">{sourceLabel(counter.source)}</dd></div>{/each}</dl>
        {#each evidence.normalization as rule}<div class="native-rule"><h5>{rule.component} · {rule.rule}</h5><p>Recorded normalization operands</p><ul>{#each rule.operands as counter}<li>{counter.name} = {tokens(counter.value)} · {sourceLabel(counter.source)}</li>{/each}</ul></div>{/each}
      </details>
    {/each}
    {#if detail.timingEvidence?.length}<details class="inspection-disclosure"><summary>Recorded timing evidence</summary><dl class="metadata">{#each detail.timingEvidence as timing}<div><dt>{timing.meaning}</dt><dd>{timing.value} ({timing.unit})</dd><dd class="measurement-note">{sourceLabel(timing.source)}</dd></div>{/each}</dl></details>{/if}
    {#if detail.relationships?.length}<details class="inspection-disclosure"><summary>Native relationships</summary><ul>{#each detail.relationships as relation}<li>{relation.kind} · {relation.target} · {relation.execution ? 'execution evidence' : 'session or context relationship'}<p class="source">{sourceLabel(relation.source)}</p></li>{/each}</ul></details>{/if}
  </section>
{/if}
