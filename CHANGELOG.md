# Changelog

## Unreleased

## [0.2.0] - 2026-10-30

- Browse saved Codex, Pi Agent, Claude Code, and OpenCode sessions with `system`, configurable provider roots, live persisted updates, and source diagnostics.
- Build and start from source with `npm run launch -- system`.
- Add a shorter README with a screenshot of a synthetic recording and remove unused design prototypes from the repository and package.

- Follow external append-only recordings with `watch <directory>`, shared workspace pause/resume, stopped-source retry, immutable revisions and stable span selection.
- Show exact own and inclusive span token accounting, fallback provenance and bounded contributor evidence; keep generation context input local and hide revealed bodies on refresh.

- Inspect tool names, call IDs, argument/result availability, original result lengths, retained preview sizes, and separate source/local truncation evidence.
- Inspect complete recorded request composition before preview truncation, including advertised tool definitions, skill descriptors, nested instruction sections, history, and opaque reasoning sizes, with explicit unknown and retention states.
- Add generation filtering and Context navigation, preserve per-request cache/input accounting, and label session token totals as cumulative.

- Import existing Pi JSONL recordings into immutable Rust-analyzed snapshots with occurrence-safe relationships, timing provenance, usage coverage, and source diagnostics.
- Add the Svelte execution workbench with shared waterfall/list selection, bounded metadata queries, deliberate text-only previews, themes, and keyboard controls.
- Select a recording with `serve <directory>` through the installed CLI, npm, or Pi command. Retry repairs to the same failed source and reconnect with explicit stale-snapshot handling.

- Add a Pi package and commands to build, start and stop the local app.
- Ship the Rust executable and web assets together in one npm archive.
- Keep release versions synchronized across package metadata and Rust.
- Replace GitHub workflows with local checks.

## [0.1.0] - 2026-10-29

- Start the local Rust server and browser interface.
- Verify the engine connection through the health endpoint.
- Document the proposed trace importer, analyzer and explorer in OpenSpec.
