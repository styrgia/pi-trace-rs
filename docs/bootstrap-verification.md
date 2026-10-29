# Bootstrap verification

Verified on 2026-10-29 with Rust 1.99.1, Node.js 26.13.0 and npm 11.20.0 on Linux. CI is configured to use Node.js 24 and stable Rust. Locked versions include Svelte 5.57.3 and Vite 8.3.5.

## Local checks

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo test --workspace --locked`: passed with zero tests. Importer and analyzer tests are future tasks.
- `npm --prefix web ci`: passed from the committed lockfile.
- `npm --prefix web run check`: zero errors and zero warnings.
- `npm --prefix web run build`: passed.
- `openspec validate --all --strict --no-interactive`: one change passed.

## Browser checks

The built Svelte page was served by Rust at `http://127.0.0.1:4317`. Selecting **Check connection** displayed `Connected to pi-trace v0.1.0` from the real API response.

Stopping Rust and checking again displayed the unavailable state and recovery hint. Restarting Rust and activating the button with Enter restored the connected state. A second browser check at `http://127.0.0.1:5173` confirmed that the Vite proxy reaches the same Rust health endpoint.

These checks validate the starter only. No trace import, timeline, subagent analysis, large-trace performance or narrow-window runtime behavior is claimed.

## Dependency audit

`npm --prefix web audit --audit-level=high` passed. The full audit reported two low-severity findings in the build dependency chain `@sveltejs/vite-plugin-svelte` → `deepmerge`, with no available automatic fix. The advisory is [GHSA-gcqx-w48m-w2h4](https://github.com/advisories/GHSA-gcqx-w48m-w2h4). No override or audit suppression was added. Recheck this chain when updating the Svelte plugin; build configuration is trusted project input.

## Review

An independent source review covered the authored code, configuration, CI, documentation, licensing and OpenSpec artifacts. It found no required corrections. A comment audit found no narration comments or suppressions; the Svelte configuration's JSDoc type annotation remains a type contract.

GitHub Actions results are available in the repository's Actions tab and are separate from these local checks.
