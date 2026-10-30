# Proposal

## Why

Users need one local view of saved and running coding-agent sessions across projects, without exporting a collector recording or reopening files after each update. Pi Trace currently opens one offline collector recording, and the active realtime change does not discover native Codex, Pi Agent, Claude Code, or OpenCode sessions.

## What Changes

- Add an explicit `system` mode that discovers the current OS user's saved and newly created sessions from all four providers. Include history and active sessions across projects, with provider and project filters and startup-only source-root overrides.
- Add Rust adapters for native Codex, Pi Agent, Claude Code, and OpenCode storage. Retain the existing collector `events.jsonl` reader and existing offline invocation forms.
- Maintain a paginated session catalog independently of bounded trace loading. Follow new sessions and changes to existing sessions without a browser reload. A missing installation or one damaged session does not disable other providers.
- Normalize recorded messages, model execution, tools, explicit parent links, timing, and usage into Rust application views. Identify each provider's evidence limits. Preserve unknown values and diagnose unsupported records instead of fabricating spans, durations, activity, or token counts.
- Preserve original JSONL file and physical-line references, and encoding-qualified decoded lines for compressed history. Add typed database provenance where native storage has database rows, without inventing a line number.
- Add a session browser and selection-scoped live investigation. Preserve selection and filters across revisions, distinguish new activity from a confirmed running process, and keep prompts and tool bodies behind deliberate reveal.
- Bound discovery, indexing, selected-session following, retention, and retries. Report partial discovery, stale data, unavailable roots, and unsupported formats explicitly.
- Verify all four providers with synthetic native storage, a real local service and browser, and an extracted npm package before distribution.

The scope assumption is all saved and active sessions readable through the current user's supported local stores. Other OS accounts, remote hosts, cloud history, provider credentials, session capture, and control of agent processes are outside this proposal. An agent that does not persist an event cannot provide that event through this mode.

## Capabilities

### New Capabilities

- `system-session-catalog`: Provider-root discovery, session identity, historical and new-session indexing, catalog pagination, freshness, isolation, and resource limits.
- `native-session-adapters`: Provider-specific native storage, normalized evidence, source provenance, branches, relationships, timing, usage, and update handling.
- `system-session-workbench`: Explicit system startup, session catalog API, selected-session live views, browser continuity, local request boundaries, and installed-package behavior.

### Modified Capabilities

None. `openspec list --specs --json` reports no published capabilities. This change builds on the application views in `implement-svelte-trace-workbench` and the shared reducer, exact accounting, live revisions, and browser refresh planned in `add-realtime-span-tokens`.

The latter change explicitly excludes native session readers and arbitrary filesystem selection. This proposal adds native readers in a separately selected mode with roots fixed at startup. It does not edit that active change or reinterpret its collector-only acceptance as native-provider support. Reuse its verified shared mechanisms, complete any missing prerequisites in the owning change, and keep collector-watch and system-mode acceptance distinct.

## Impact

Implementation affects `crates/trace-core` adapters, provenance, correlation and application views, `crates/pi-trace` session indexing and lifecycle, the Node launcher, Pi extension command forwarding, and the Svelte catalog and refresh state. Synthetic fixtures, contract examples, API and compatibility documentation, and packaging checks need corresponding coverage.

Native storage support adds bundled SQLite and zstd decoding dependencies for verified OpenCode and Codex formats. It must work without a provider executable, provider daemon, network connection, or system `sqlite3` command. Source stores remain read-only, and no persistent copy of private sessions is required.

Existing offline commands and collector-watch contracts retain their meanings. New catalog routes and typed native provenance are additive application contracts. The root package continues to own the release version and MIT notices remain intact. Creating these planning artifacts implements no runtime behavior and authorizes no commit, publication, or release.
