<script lang="ts">
  import { getHealth, type HealthResponse } from './lib/health'

  type ConnectionState =
    | { kind: 'idle' }
    | { kind: 'checking' }
    | { kind: 'connected'; health: HealthResponse }
    | { kind: 'unavailable' }

  let connection = $state<ConnectionState>({ kind: 'idle' })

  async function checkConnection() {
    connection = { kind: 'checking' }

    try {
      const health = await getHealth()
      connection = { kind: 'connected', health }
    } catch {
      connection = { kind: 'unavailable' }
    }
  }
</script>

<svelte:head>
  <meta
    name="description"
    content="Pi Trace is a local trace explorer for Pi coding agents, with a Rust engine and a Svelte interface."
  />
</svelte:head>

<div class="workspace">
  <header class="topbar">
    <a class="brand" href="/" aria-label="Pi Trace home">
      <span class="brand-mark" aria-hidden="true">π</span>
      <span>Pi Trace</span>
    </a>
    <span class="stage">Bootstrap</span>
  </header>

  <main>
    <div class="page-intro">
      <p class="eyebrow">Local workspace</p>
      <h1>Pi Trace workspace</h1>
      <p class="intro">A local trace explorer for Pi coding agents.</p>
    </div>

    <div class="panels">
      <section class="connection-panel" aria-labelledby="connection-heading">
        <div class="panel-heading">
          <span class="section-index" aria-hidden="true">01</span>
          <h2 id="connection-heading">Engine connection</h2>
        </div>
        <p>Check that the Svelte interface can reach the local Rust API.</p>

        <dl class="connection-details">
          <div><dt>Engine</dt><dd>Rust</dd></div>
          <div><dt>Interface</dt><dd>Svelte</dd></div>
          <div><dt>API address</dt><dd><code>127.0.0.1:4317</code></dd></div>
        </dl>

        <button type="button" onclick={checkConnection} disabled={connection.kind === 'checking'}>
          {connection.kind === 'checking' ? 'Checking connection…' : 'Check connection'}
        </button>

        <div
          class="connection-status"
          class:connected={connection.kind === 'connected'}
          class:unavailable={connection.kind === 'unavailable'}
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          {#if connection.kind === 'idle'}
            <p>Connection not checked yet.</p>
          {:else if connection.kind === 'checking'}
            <p>Waiting for the local Rust API.</p>
          {:else if connection.kind === 'connected'}
            <p>Connected to {connection.health.name} v{connection.health.version}.</p>
            <p class="status-hint">The API reports the bootstrap stage.</p>
          {:else}
            <p>The local Rust API is unavailable.</p>
            <p class="status-hint">
              Start the Rust API at <code>127.0.0.1:4317</code>, then try again.
            </p>
          {/if}
        </div>
      </section>

      <aside class="scope-panel" aria-labelledby="scope-heading">
        <div class="panel-heading">
          <span class="section-index" aria-hidden="true">02</span>
          <h2 id="scope-heading">Project status</h2>
        </div>
        <p>This is the initial project setup. Trace import and analysis are planned.</p>
        <h3>Planned inspection</h3>
        <ul>
          <li>Timeline and generation durations</li>
          <li>Tool calls, errors, and token usage</li>
          <li>Subagent relationships</li>
        </ul>
        <p class="scope-note">No traces are loaded or sent by this screen.</p>
      </aside>
    </div>
  </main>

  <footer>Pi Trace <span aria-hidden="true">/</span> Open source, local first</footer>
</div>
