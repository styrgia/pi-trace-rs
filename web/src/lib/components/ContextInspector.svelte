<script lang="ts">
  import { sourceLabel } from '../presentation'
  import type { ContextInspection, TokenAccounting } from '../trace-api'
  import { quantity, tokens } from '../presentation'
  let { context, usage }: { context: ContextInspection; usage: TokenAccounting } = $props()
  const labels = {
    systemInstructions: 'System instructions', developerInstructions: 'Developer instructions', user: 'User messages',
    assistantHistory: 'Assistant history', toolCalls: 'Function calls', toolOutputs: 'Model-visible function outputs',
    toolDefinitions: 'Advertised tool definitions', opaqueReasoning: 'Opaque reasoning', other: 'Other / unclassified',
  }
</script>

<div class="context-inspection">
  <h4>Selected generation context</h4>
  <p class="muted">Request usage belongs to this generation. Component token usage is not recorded.</p>
  <dl class="metadata context-facts">
    <div><dt>Recorded input tokens</dt><dd>{tokens(usage.inputTokens)}</dd></div>
    <div><dt>Recorded cache read tokens</dt><dd>{tokens(usage.cacheReadTokens)}</dd></div>
    <div><dt>Recorded cache write tokens</dt><dd>{tokens(usage.cacheWriteTokens)}</dd></div>
    <div><dt>Recorded output tokens</dt><dd>{tokens(usage.outputTokens)}</dd></div>
    <div><dt>All input tokens</dt><dd>{tokens(context.contextInputTokensExact)}{context.contextInputTokensExact.state === 'exact' ? ' (derived, generation-local)' : ''}</dd></div>
    <div><dt>Composition coverage</dt><dd>{context.coverage}</dd></div>
    <div><dt>Inspection retention</dt><dd>{context.retention}</dd></div>
    <div><dt>Request shape / field</dt><dd>{context.shape} / {context.requestField ?? 'None'}</dd></div>
  </dl>
  {#if context.retention === 'retentionLimited'}
    <p class="truncation">Inspection metadata reached its retention budget. Composition measurements are unavailable for this request. Recorded content still uses explicit bounded reveal.</p>
  {:else if context.coverage === 'missing'}
    <p class="muted">Request evidence is missing or explicitly null. No context composition can be measured.</p>
  {:else}
    <dl class="metadata context-facts">
      <div><dt>Whole selected payload</dt><dd>{quantity(context.payloadSize?.compactJsonBytes ?? null)} compact JSON bytes</dd></div>
      <div><dt>Recognized text</dt><dd>{quantity(context.payloadSize?.textChars ?? null)} Unicode scalar characters</dd></div>
      <div><dt>Recognized message items</dt><dd>{quantity(context.counts?.messages ?? null)}</dd></div>
      <div><dt>Advertised tool definitions</dt><dd>{quantity(context.counts?.toolDefinitions ?? null)}</dd></div>
      <div><dt>Advertised skill descriptors</dt><dd>{quantity(context.counts?.skillDescriptors ?? null)}</dd></div>
      <div><dt>Unclassified items</dt><dd>{quantity(context.counts?.unclassifiedItems ?? null)}</dd></div>
    </dl>
    {#if context.coverage === 'unsupported'}<p class="muted">This request shape is unsupported. Its selected JSON size is measurable; message, tool, and skill counts are unknown.</p>{/if}
    {#if context.coverage === 'partial'}<p class="muted">Only recognized parts are classified. Text counts cover recognized text only. Related diagnostics explain malformed sections.</p>{/if}
    {#if context.shape === 'plainText'}<p class="muted">The recorded request is plain text. Its role, message count, and advertised catalogues are unknown.</p>{/if}
    {#if context.categories.length}
      <h4>Context components</h4>
      <p class="muted">Component JSON bytes exclude envelope overhead. They do not sum to the whole selected payload.</p>
      <table class="inspection-table"><thead><tr><th scope="col">Category</th><th scope="col">Count / compact JSON bytes</th><th scope="col">Recognized text characters / coverage</th></tr></thead><tbody>
        {#each context.categories as category}<tr><th scope="row">{labels[category.kind]}</th><td>{quantity(category.count)} / {quantity(category.size.compactJsonBytes)}</td><td>{quantity(category.size.textChars)} / {category.textCoverage}</td></tr>{/each}
      </tbody></table>
    {/if}
    {#if context.sections.length}
      <h4>Recognized instruction sections</h4>
      <p class="muted">Nested spans include delimiters and overlap. Do not add parent and child character counts.</p>
      <table class="inspection-table"><thead><tr><th scope="col">Section / parent section</th><th scope="col">Unicode scalar characters</th></tr></thead><tbody>
        {#each context.sections as section}<tr><th scope="row">#{section.id + 1} {section.kind}<span class="muted"> / {section.parentId === null ? 'root' : `#${section.parentId + 1}`}</span></th><td>{quantity(section.textChars)}</td></tr>{/each}
      </tbody></table>
    {/if}
    {#if context.largestItems.length}
      <details class="inspection-disclosure"><summary>Other component sizes</summary>
      <table class="inspection-table"><thead><tr><th scope="col">Component ordinal / category</th><th scope="col">Compact JSON bytes</th></tr></thead><tbody>
        {#each context.largestItems as item}<tr><th scope="row">{item.ordinal + 1}. {labels[item.kind]}</th><td>{quantity(item.size.compactJsonBytes)}</td></tr>{/each}
      </tbody></table>
      </details>
    {/if}
    {#each [{ title: 'Largest tool definitions', list: context.largestTools, omitted: context.omitted.tools, unit: 'compact JSON bytes', tools: true }, { title: 'Largest skill descriptors', list: context.largestSkills, omitted: context.omitted.skills, unit: 'Unicode scalar characters', tools: false }] as group}
      {#if group.list.length}
        <h4>{group.title}</h4>
        <p class="descriptor-preview">{group.list.slice(0, 3).map(item => item.name ?? 'Name not recorded').join(' · ')}</p>
        <details class="inspection-disclosure"><summary>Show {group.list.length} largest {group.tools ? 'tool definitions' : 'skill descriptors'}</summary>
        <p class="muted">Advertised capabilities. Sizes measure whole definitions or descriptors.</p>
        <table class="inspection-table"><thead><tr><th scope="col">Name / source ordinal</th><th scope="col">{group.unit}</th></tr></thead><tbody>
          {#each group.list as item}<tr><th scope="row">{item.name ?? 'Name not recorded'}{item.nameTruncated ? '… (name truncated)' : ''}<span class="muted"> #{item.ordinal + 1}</span></th><td>{quantity(group.tools ? item.size.compactJsonBytes : item.size.textChars)}</td></tr>{/each}
        </tbody></table>
        </details>
      {/if}
      {#if group.omitted !== null && group.omitted > 0}<p class="truncation">{quantity(group.omitted)} more {group.tools ? 'tool definitions' : 'skill descriptors'} omitted from the bounded list.</p>{/if}
    {/each}
    {#if context.omitted.sections === null && context.coverage === 'partial'}<p class="muted">The complete section count is unknown because delimiters could not be recognized safely.</p>{/if}
    {#if context.omitted.items !== null && context.omitted.items > 0}<p class="truncation">{quantity(context.omitted.items)} more components omitted from the bounded list.</p>{/if}
    {#if context.omitted.sections !== null && context.omitted.sections > 0}<p class="truncation">{quantity(context.omitted.sections)} more sections omitted from the bounded list.</p>{/if}
  {/if}
  <p class="muted">JSON sizes use compact reserialization, not wire bytes. Characters are Unicode scalars, not UTF-16 units or tokens.</p>
  {#if context.source}<p class="source">Request source {sourceLabel(context.source)}</p>{/if}
</div>
