<script lang="ts">
  let { kind, message = '', code = '', source = '', onaction = () => {} }: {
    kind: 'loading' | 'empty' | 'importing' | 'failed' | 'unavailable' | 'stale'
    message?: string
    code?: string
    source?: string
    onaction?: () => void
  } = $props()
  const titles = {
    loading: 'Connecting to the local workspace',
    empty: 'No recording selected',
    importing: 'Importing the recording',
    failed: 'The recording could not be imported',
    unavailable: 'The local service is unavailable',
    stale: 'This snapshot is no longer available',
  }
</script>

<section class="load-state" aria-labelledby="load-title" aria-busy={kind === 'loading' || kind === 'importing'}>
  <h1 id="load-title">{titles[kind]}</h1>
  {#if kind === 'empty'}
    <p>Start Pi Trace with a recording directory to investigate an offline snapshot.</p>
    <pre>pi-trace-rs serve &lt;directory&gt;</pre>
  {:else if kind === 'importing'}
    <p>Rust is reading and analyzing the selected source. The report appears when the snapshot is complete.</p>
  {:else if kind === 'loading'}
    <p>Reading recording state from the local Rust service.</p>
  {:else if kind === 'stale'}
    <p>The link names a different snapshot. Open the current workspace to begin a new investigation.</p>
  {:else if kind === 'unavailable'}
    <p>{message || 'Start or restore the local service, then reconnect.'}</p>
  {:else}
    <p>{message}</p>
    {#if code}<p><code>{code}</code></p>{/if}
    {#if source}<p class="source">{source}</p>{/if}
    <p>Repair the configured source, then retry the import.</p>
  {/if}
  {#if kind === 'failed' || kind === 'unavailable' || kind === 'stale'}
    <button class="primary-button" type="button" onclick={onaction}>
      {kind === 'failed' ? 'Retry import' : kind === 'stale' ? 'Open current workspace' : 'Reconnect'}
    </button>
  {/if}
</section>
