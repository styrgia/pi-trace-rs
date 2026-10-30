# Workbench verification

This document records the earlier offline workbench milestone and its tool/context follow-up. Its test counts and import-only behavior describe that milestone. See [realtime span-token verification](live-verification.md) for current watch behavior, exact accounting, final checks and separate package acceptance.

The workbench was verified on 2026-10-30 on Linux x64 with Rust 1.99.1, Node.js 26.13.0, npm 11.20.0, and local Chrome. The root package remains version 0.1.0. The added behavior is listed under Unreleased. No commit, tag, push, or publication is part of this verification.

The application imports existing offline JSONL recordings. Rust owns occurrences, relationships, timing, usage, queries, and limits. Svelte presents application views. The external Pi collector is a separate program; this change does not implement capture or live updates.

## Correctness and contracts

Run `npm run check` from the repository root. It checks version synchronization, Node tests, Rust formatting, Clippy, Rust tests, Svelte/TypeScript, the production web build, and strict validation of all three OpenSpec changes. The implemented suite contains 48 Node tests and 40 Rust tests. Browser and package acceptance are separate from this command.

All recording fixtures are fabricated. `crates/trace-core/tests/fixtures/README.md` explains the fourteen-observation reference, edge records, expected measurements, usage, and physical source lines. Rust regenerates the contract examples through the importer and serializers. The browser decoders consume both the core examples and the four workspace lifecycle variants.

| Scenario group | Verification |
| --- | --- |
| Root and nested children, missing/unreadable roots, escaping links, aliases, source changes | `trace-core` import unit tests and `tests/semantics.rs`; failed-import browser repair |
| Exact and over-limit bytes, physical record size/count, sources, discovery entries | Parameterized semantic tests with small explicit limits and the production defaults inspected in `ImportLimits` |
| Repeated sessions, turn attempts, step occurrences, tool identifiers, delayed tool completion | Semantic tests preserve distinct occurrences and diagnostics |
| Explicit children finishing in reverse order, missing/ambiguous links | Semantic tests and an actual unlinked-child browser recording |
| Headers versus generation, unknown TTFT, backward clocks, unavailable bars | Rust views, serialized invalid/unknown examples, and inspector checks |
| Step/summary deduplication, fallback, partial/cache-only usage, legitimate zeros, unknown cost | Semantic tests and serialized partial/fallback examples |
| Failed tool with successful interaction, incomplete final generation, unsupported recovery links | Reference import, semantic tests, and actual browser selection |
| Metadata search/filter, ancestor closure across more than 200 rows, cursor binding, byte budgets | Semantic tests plus actual 131-row browser pagination |
| Missing/truncated content, valid UTF-8, upstream UTF-16 truncation evidence, inert markup | Semantic/decoder tests and explicit browser reveal |
| Workspace states, immutable publication, failed-source retry, concurrent conflict | Service tests and actual missing-root repair/retry |
| Snapshot/node/path/query errors, Host/Origin checks, no-store, body-free metadata | Service tests, an independent malformed-header/path probe, and actual HTTP views |
| Source arguments, path spaces, npm invocation directory, Pi session directory, process ownership | Launcher/CLI/extension-host tests and real native/packaged launches |
| Shared selection, collapse, filtered selection, continuation, diagnostic navigation | Actual Rust-backed browser investigation |
| Safe view links, hidden content after reload, stale links, service loss/reconnect | Decoder/link tests and actual browser restart sequence |
| Keyboard, themes, narrow layout, focus during delayed requests | Native keyboard runs at 1440 and 375 pixels in both themes; delayed actual HTTP responses |

Independent reviews found correlation, partial-usage, final-outcome, truncation, traversal, caller-directory, and delayed-focus defects during implementation. Each accepted finding was fixed and checked again. The final service review passed in an isolated copy. MIT attribution in `THIRD_PARTY_NOTICES.md` is byte-for-byte unchanged.

## Browser observations

The same-origin production page and Vite development page both loaded fourteen observations from the actual Rust importer. Vite rewrites the proxy Host header. Neither page prefetched content or requested an external asset.

The reference investigation covers a failed tool, a later successful interaction, waterfall/list parity, collapsed and filtered selection, metadata-only search, explicit input/output reveal, safe view links, backend loss, stale-snapshot recovery, no-source mode, ready-empty mode, and failed-source repair/retry. A separate fabricated recording adds 131 observations, continuation, malformed/unknown/incomplete-tail diagnostics, an unlinked child, long multibyte output, and inert markup. Source hashes remained unchanged. Available-service runs had no uncaught browser errors.

The four native keyboard runs use no pointer input for selection, reveal, filters, view switching, inspector tabs, diagnostic navigation, or view links. Each traverses 79 tab stops. Interactive controls retain visible focus. Body text is 14 px and selection labels are 13 px at both widths. The inspector stacks at 375 px, and the page has no horizontal overflow.

The focus regression holds actual Rust detail responses before delivery. Selecting an observation moves focus immediately. Tab movement after selection remains intact when delayed responses arrive, and an obsolete selection does not replace newer details or move focus.

The app-scope Hallmark render sweep passes in both themes with zero Floor failures, zero open Reflex findings, and zero warnings. The accepted compact spacing waiver remains. Eight static warnings about dynamic button-label length are dismissed against the global `white-space: nowrap` rule and actual narrow/desktop controls. Revealed/default captures also pass direct text-contrast checks.

A 110,597,160-byte fabricated recording with 120 unknown records verifies a real importing state. Health and the shell both return 200 during import. The browser shows importing, then a ready empty snapshot with 120 diagnostics. Workspace polling stops after publication. This is load-state evidence, not a throughput claim.

## Bounded workload observations

These workload observations precede the tool/context inspection extension. They are not repeated performance evidence for the extension. The separate release workload contains one source, 20,012 physical records, 10,000 tools, and 10,005 observations. Five new release processes import the same fabricated input with a warm filesystem cache. Each run checks the ready state, exact counts, all 51 observation pages at limit 200, unique IDs, the slow filter, no-store responses, no content leakage, and the serialized byte limit.

The local results record spawn-to-ready time, complete pagination time, maximum response bytes, and process peak RSS. These include process startup and local HTTP overhead. They exclude browser work. They establish successful execution of this declared workload on this host. They do not compare implementations or establish a capacity, latency, or speedup guarantee.

## Package acceptance

The local package check runs `npm pack`, inspects its manifest, extracts it to a fresh temporary directory, and starts it from a different directory containing spaces. It supplies only Node on `PATH`; Cargo, npm, web build tools, and the source checkout are unavailable to the launched package. It verifies source selection, fourteen imported observations, no-argument empty mode, the health version, and the hashes of served JavaScript/CSS against the extracted assets.

The archive must include the matching Linux x64 executable, `web/dist`, and runtime Rust sources. Both crate test trees, synthetic recordings, local sessions, traces, agent audit files, and dependencies are excluded. Packaging verification is local and does not publish the archive.

## Local evidence and reproduction

Detailed request metadata, screenshots, workload samples, package metadata, and the decision trail live under the ignored `.audit/svelte-workbench` directory. They are working artifacts and are excluded from the npm package. If the external collector is used for a separately requested Pi session, keep its raw recordings outside the repository. Its configured output directory is `~/.pi/agent/traces`; verify the actual session path after launch.

The local acceptance helpers are:

```sh
node .audit/svelte-workbench/reference-capture.mjs
node .audit/svelte-workbench/browser-check.mjs
node .audit/svelte-workbench/importing-check.mjs
node .audit/svelte-workbench/workload-check.mjs
node .audit/svelte-workbench/package-check.mjs
```

`browser-check`, `importing-check`, `workload-check`, and `package-check` each require port 4317 to be free and manage their own service. `keyboard-check` and `focus-check` use an already running reference service. Vite acceptance uses port 5173. Run each port-owning helper sequentially.

## Tool and context inspection follow-up

The extension adds Rust-owned request composition before content-preview truncation, bounded descriptor/section lists, explicit tool evidence, and generation navigation. The full local check passes 48 Node tests and 40 Rust tests, with Svelte reporting zero errors or warnings and all three OpenSpec changes valid. Synthetic cases cover missing/null/unsupported and incomplete function records, nested catalogues, metadata retention, exact scalar/UTF-8/UTF-16 domains, canonical decimal original lengths through u64::MAX, and no body leakage.

Actual browser investigation verifies Context discovery, input/cache accounting, named descriptor sizes, tool argument/result reveal, hidden bodies after reload, both themes, 375-pixel stacking, text contrast, and no external requests or uncaught errors. Delayed actual generation-query responses preserve a newer explicit selection and keyboard focus. A separate synthetic recording reaches all 205 generations through three pages with ancestor context and exercises missing, unsupported, plain-text, and empty request evidence. The initial investigation regression also passes with the new views.

The separately requested real Pi recording stays outside Git. Its detailed analysis and screenshots also stay outside the repository; only metadata-only verification evidence lives in the ignored audit folder. A collector shutdown/startup appended during session restoration is preserved as another occurrence. Verification reads do not modify the recording. Exact component token attribution is unavailable; the UI labels serialized JSON bytes, Unicode scalar characters, recorded usage and cumulative totals separately.

The local extension helpers are `.audit/tool-context/real-api-check.py`, `browser-check.mjs`, `race-check.mjs`, `pagination-check.mjs`, `regression-browser-check.mjs`, and `package-check.mjs`. Real-recording checks require the explicitly selected outside-Git recording and a running service. Pagination uses the separate fabricated recording generated by `make-browser-recording.mjs`. Port-owning helpers run sequentially. The contrast probe excludes unpainted native disclosure content, and expanded disclosure states receive their own captures.
