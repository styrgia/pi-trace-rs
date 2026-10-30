<script lang="ts">
  import { sourceLabel } from '../presentation'
  import type { NodeDetail } from '../trace-api'
  import { quantity } from '../presentation'
  let { detail }: { detail: NodeDetail } = $props()
</script>

{#if detail.toolInspection}
  {@const tool = detail.toolInspection}
  <dl class="metadata">
    <div><dt>Tool name</dt><dd>{detail.metadata.toolName ?? 'Unknown'}</dd></div>
    <div><dt>Original serialized result length</dt><dd>{quantity(tool.result.originalLength?.value ?? null)} {tool.result.originalLength?.unit === 'utf16CodeUnitsOfSerializedResult' ? 'UTF-16 code units' : 'unit unspecified'}</dd></div>
    <div><dt>Recorded result preview</dt><dd>{quantity(tool.result.previewUtf8Bytes)} UTF-8 bytes</dd></div>
    <div><dt>Source result</dt><dd>{tool.result.sourceTruncation === 'knownTruncated' ? 'Truncated by collector' : tool.result.sourceTruncation === 'knownComplete' ? 'Known complete' : 'Completeness unknown'}</dd></div>
  </dl>
  {#if tool.retention === 'retentionLimited'}<p class="truncation">Optional tool metadata reached its retention budget. Recorded previews and source references remain available.</p>{/if}
  <details class="inspection-disclosure"><summary>Tool capture details</summary>
  <dl class="metadata">
    <div><dt>Tool call ID</dt><dd>{tool.callId ?? 'Unknown'}{tool.callIdTruncated ? '… (display truncated)' : ''}</dd></div>
    <div><dt>Inspection retention</dt><dd>{tool.retention}</dd></div>
    <div><dt>Duration meaning</dt><dd>{detail.observation.duration.meaning}</dd></div>
    <div><dt>Relationship</dt><dd>{detail.relationship.state}</dd></div>
  </dl>
  {#each [{ name: 'Arguments', evidence: tool.arguments }, { name: 'Recorded result', evidence: tool.result }] as entry}
    <h4 class="evidence-title">{entry.name} evidence</h4>
    <dl class="metadata">
      <div><dt>Preview availability</dt><dd>{entry.evidence.availability}</dd></div>
      <div><dt>Whole recorded selected value</dt><dd>{quantity(entry.evidence.selectedSize?.compactJsonBytes ?? null)} compact JSON bytes</dd></div>
      <div><dt>Retained reveal preview</dt><dd>{quantity(entry.evidence.previewUtf8Bytes)} UTF-8 bytes</dd></div>
      <div><dt>Source truncation</dt><dd>{entry.evidence.sourceTruncation === 'knownTruncated' ? 'Known truncated' : entry.evidence.sourceTruncation === 'knownComplete' ? 'Known complete' : 'Unknown'}</dd></div>
      <div><dt>Local preview byte cap</dt><dd>{entry.evidence.localTruncated ? 'Truncated at 8,192 UTF-8 bytes' : 'No local truncation'}</dd></div>
      {#if entry.evidence.source}<div><dt>Evidence source</dt><dd>{sourceLabel(entry.evidence.source)}</dd></div>{/if}
    </dl>
  {/each}
  <dl class="metadata">
    <div><dt>Recorded result field</dt><dd>{tool.result.recordedField ?? 'Not recorded'}</dd></div>
  </dl>
  <p class="muted">Original result length describes the collector's serialized result. Model-visible function output is measured separately in the generation Context. Missing length does not prove completeness.</p>
  </details>
{/if}
