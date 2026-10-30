# Realtime span-token verification

`add-realtime-span-tokens` implements explicit `watch <directory>` following of external append-only Pi recordings. Rust owns decoding, occurrence identity, relationships, completion, diagnostics and exact usage. The browser consumes immutable views and presents coherent replacements. Capture remains external. This document records local verification on 2026-10-30; it does not claim publication, cross-platform acceptance or a general latency guarantee.

## Verified behavior

The core compares settled live and offline overview, rows, details, content and diagnostics after normalizing snapshot and source-anchored identity fields. The synthetic recording includes duplicate completion, partial evidence suppressing fallback, eligible fallback, a late child, exact integers above JavaScript's safe range and recorded timing. Source bytes remain unchanged. Run:

```sh
cargo test -p trace-core --test follow settled_live_and_offline --locked
node --test tests/trace-api.test.mjs tests/workspace-poller.test.mjs tests/snapshot-view.test.mjs
npm run check
```

The source check passed with exit 0, 88 Node tests and 87 Rust tests. It includes version synchronization, Rust formatting, strict Clippy, Svelte checking with zero errors or warnings, the production build and strict validation of all five current OpenSpec changes. The shared checkout also contains separate system-session work; that work is outside this change's acceptance claims.

Independent scoped client and service reviews passed after their accepted race findings were fixed. Client review reran the 22 transport, polling and coherent-frame tests and inspected the saved controlled browser run. Service review separately checked shutdown/retry ownership, failure before a reset's first publication, and stopped-state publication before a failed resume acknowledgment.

## Browser protocol and fixtures

The production web build runs against the actual local Rust service at `http://127.0.0.1:4317`. Browser automation uses owned, isolated Chromium instances. The initial interactive run used the named agent-browser session `pi-trace-realtime-a663b8f6264a`; it was closed after verification. The default user browser and profile were not used.

The fabricated initial source is `.audit/live-browser/trace/events.jsonl`, with a child under `subagents/a/events.jsonl`. Its first six records create session, interaction, turn and generation starts, a request containing `SYNTHETIC_HIDDEN_BODY`, and a 150-token summary fallback. Appending a generation completion with 120 tokens replaces that partial fallback. A later explicitly linked child contributes 40 tokens. Expected generation own usage is 120, generation inclusive usage is 160, child/tool inclusive usage is 40, and overview is 160. Selected Turn 3 and search-input focus survive both updates. Metadata makes no content request.

A separate generation records 9007199254740993 input tokens and zero output/cache counts. The exact row, inspector and generation-local context input preserve that integer. Ordinary `read` has not-applicable own and inclusive model usage. Containers, generations and linked child tools all show a value or an explicit state. The inspector labels nested inclusive values non-additive and retains unknown component values.

The interactive command sequence is:

```sh
./target/debug/pi-trace watch "$PWD/.audit/live-browser/trace"
agent-browser --session pi-trace-realtime-a663b8f6264a open http://127.0.0.1:4317/
agent-browser --session pi-trace-realtime-a663b8f6264a snapshot -i
agent-browser --session pi-trace-realtime-a663b8f6264a set viewport 375 812
agent-browser --session pi-trace-realtime-a663b8f6264a tab new http://127.0.0.1:4317/
```

Use fresh snapshot refs for selection and controls. Append fabricated complete JSONL records through a separate writer. Pause in one tab, append a backlog, and bring the second tab forward. Both tabs stay on revision 5 and report the shared pause. Resume advances both to revision 6. Keyboard Enter activation preserves a visible two-pixel focus outline on the same button as its label changes. The button retains focus while a pending-action guard rejects duplicate controls. Pause does not stop the writer.

The repeatable controlled-response helper is local and ignored:

```sh
node .audit/live-browser/browser-check.mjs
```

It uses an already running watch service on the initial synthetic directory and resets only that owned fixture. CDP holds actual HTTP responses from the service. Assertions cover:

- No replacement view requests for an unchanged revision and no implicit body request.
- Old diagnostics released after a new frame cannot replace the new diagnostics.
- First opening diagnostics during a held refresh produces populated diagnostics for the replacement frame.
- A status GET started before Pause cannot undo its acknowledged response. The equivalent held old-run response cannot restore the retired run after Retry.
- Explicit reveal renders `<script>window.untrustedExecuted=true</script>` as text. It does not execute. A later revision hides the preview and never requests content automatically.
- Selected detail remains available outside the Tools filter. Filter choice, collapsed stable spans and input focus survive appends.
- Source truncation stops ingestion and retains the last valid frame. Retry creates a new run, clears selection and revealed bodies, and preserves list view and theme.
- Both themes at 1440 and 375 pixels have no page-level horizontal overflow. The inspector stacks below the execution view at 375 pixels.

The controlled run passed nine scenario groups with 69 requests, no uncaught errors and no external requests. Its request log is in `browser-evidence.json`. The interactive run's request evidence is `initial-updates.har` and `controls-reveal.har`. These HARs contain only fabricated bodies and stay outside Git and the npm archive.

Screenshots under `.audit/live-browser` include `initial-light.png`, `child-light.png`, `revealed-light.png`, `context-dark-375.png`, `resumed-dark-375.png`, `list-light-1440.png`, `list-light-375.png`, `list-dark-1440.png`, `list-dark-375.png`, `stopped-dark-375.png` and `reset-dark-375.png`. Revealed/default captures are separate. Exact large values use locale grouping, so verification compares digits rather than assuming commas.

## Declared latency workload

Run the port-owning helper with port 4317 free:

```sh
npm run build
node .audit/live-browser/workload-check.mjs
```

This is one successful acceptance run of the declared workload, with a release executable and a visible headless Chromium page. It is not a comparison or throughput benchmark. The writer is intentionally limited to ten records per second. Native following polls every 250 ms and browser status polls every 500 ms; that scheduling contributes to observed latency. No profiler is attached during the accepted run.

The machine is Linux x86_64, kernel 6.8.0-139-generic, AMD Ryzen 7 7800X3D, 16 logical CPUs, Node 26.13.0. Load averages before the run were 0.59, 1.20 and 1.31. The workload starts with five records and appends 300 records over 30 seconds, alternating generation starts and completions. It finishes with 21,447 input bytes, 305 physical records, 154 spans, an exact total of 150 tokens and zero derived diagnostics. It remains below 10 MiB and 10,000 spans.

The writer records a wall-clock timestamp after each newline write completes. Thirty samples, one per ten records, record the expected total. A browser read obtains its own timestamp and checks the selected parent's row and inspector counter state together. Every sample appeared within two seconds. Observed latency was 79.4–674.9 ms, median 130.6 ms. The browser observed 62 distinct publications. API errors and timed-region request failures were zero, and no content route was requested. Browser-generated favicon misses are outside API acceptance.

Linux process and per-thread CPU counters, RSS and peak RSS are retained with timestamps. CPU percentages use one logical core as 100%. The whole service, including ingestion and HTTP handling, used 0.0% during the five-second initial idle window, 1.01% during active ingestion and 0.20% during the five-second final idle window. These small idle values have 10 ms tick resolution. RSS rose from 5,520 KiB to 8,532 KiB and remained 8,532 KiB after the final idle interval. This is observed process memory, not a memory bound. Revision identity remained unchanged during final idle. An old publication returned `unknown_snapshot`, consistent with bounded retention.

After the timed region, the helper adds 2,100 generations. The separate pagination check reaches 2,254 spans, loads 2,000 retained rows, appends another record and preserves both the bound and selected Turn 3. The UI explains the retained-view limit. It does not retain every revision or indefinitely append old pages.

`workload-timed-evidence.json` holds the timed result. `workload-evidence.json` adds pagination results. Both include machine metadata, flush/browser timestamps, exact work counts and resource samples. `workload-final.png`, `retained-rows-2000.png` and `workload-service.log` accompany them. The helper verifies source bytes against its writer's expected byte stream.

## Extracted package protocol

The extracted-package protocol passed with exit 0. Its manifest contains 50 files and no excluded raw, fixture, session or audit entries. Three external-cwd launches cover direct watch, direct serve and npm-prefix watch. A separate extracted launcher run verifies same-mode reuse and mode conflicts in both directions. Exact usage progresses from 9007199254740993 to 9007199254741000 after a paused seven-token append. Every owned native process exits after shutdown.

The local package helper is `.audit/live-browser/package-check.mjs`. It runs `npm pack`, inspects its complete manifest, extracts under a fresh path containing spaces, and starts the extracted application from another cwd with only Node on `PATH`. Cargo, npm and frontend build tools are unavailable on the launched application's `PATH`. The npm-prefix test invokes npm by its absolute path and explicitly selects `/bin/sh` for npm's lifecycle shell; the application still has only Node on `PATH`. The archive includes its matching Linux x64 executable and compiled assets. The helper verifies exact large counters, append/pause/resume, installed CLI and npm-prefix source resolution, same-mode process reuse, conflicting mode/source rejection and owned shutdown. It compares served asset bytes, licensing and recording bytes with their expected values.

Package acceptance is recorded separately in `package-evidence.json`, `pack-manifest.json` and `pack.log`. This local protocol does not publish the archive. Raw recordings, source fixtures, test trees, local sessions, dependencies and audit evidence must be absent from the manifest.

## Supported limits

Watch currently requires Unix file identity support. Other platforms fail explicitly. Supported observed discontinuities stop ingestion without publishing a rejected batch. Same-size rewrites need observable metadata changes or a differing sampled consumed boundary. Unsampled interior rewrites with preserved metadata, including interior rewriting combined with growth, cannot always be detected. External producers must remain append-only. Complete offline EOF and provisional live EOF remain distinct.

This change does not start or control capture, infer missing timestamps or parent links, or substitute unknown token components with zero. Shared pause controls ingestion only. Snapshot retention is two publications, API view concurrency is eight, view bodies are at most 1 MiB, contributor evidence is at most 16 entries, and the client retains at most 2,000 observation rows. These limits do not establish a universal total-RSS guarantee.

MIT attribution in `THIRD_PARTY_NOTICES.md` is unchanged, with SHA-256 `f73ee8be69af429d4939b97824f8a34b7bc145d79552c886d9ecd021b3bf11fc`. No upstream attribution was removed. Version 0.1.0 remains unchanged; no commit, tag, push or publication is authorized by this implementation workflow.
