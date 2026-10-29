# Tasks

## 1. Repository foundation

- [x] 1.1 Create the Rust server scaffold and verify formatting, Clippy and Cargo test commands succeed with the lockfile.
- [x] 1.2 Create the Svelte 5 web scaffold and verify a clean npm install, Svelte/TypeScript checks and production build.
- [x] 1.3 Connect the starter to `/api/v1/health` and verify connected, unavailable and retry states in a real browser.
- [x] 1.4 Add MIT attribution, compatibility notes, contributor guidance and CI; verify README commands and strict OpenSpec validation.
- [x] 1.5 Publish the initial commit to the public GitHub repository and verify its visibility and remote commit.

## 2. Import recordings

- [ ] 2.1 Add `trace-core` with one import entry point and bounded source reading; verify happy-path, oversized and escaping-symlink synthetic cases.
- [ ] 2.2 Model epochs and turn attempts; verify restarted and retried indices do not overwrite or cross-link occurrences.
- [ ] 2.3 Preserve file/line provenance and diagnostics; verify malformed middle records, truncated tails, unknown events and missing ends separately.
- [ ] 2.4 Correlate tools and explicit child links; verify parallel child completion in reverse order and unresolved ambiguous links.
- [ ] 2.5 Document accepted input and limits; verify the documented import command against a synthetic trace directory.

## 3. Analyze snapshots

- [ ] 3.1 Build the immutable occurrence graph and query views; verify selection and parent relationships across sessions, turns, generations and tools.
- [ ] 3.2 Implement timing provenance; verify header latency, full generation timing, unavailable TTFT and negative derived intervals.
- [ ] 3.3 Aggregate usage once with summary-only fallback; verify repeated totals, partial generation coverage and missing cost.
- [ ] 3.4 Compute final outcomes separately from tool errors; verify recovered failures and incomplete endings.
- [ ] 3.5 Document timing and usage semantics with synthetic examples; verify the examples against analyzer output.

## 4. Serve and explore

- [ ] 4.1 Add the source-selecting CLI and versioned query endpoints with bounded responses; verify only the selected snapshot is accessible and missing IDs return errors.
- [ ] 4.2 Enforce Host/Origin and loopback access policy before returning trace data; verify allowed production/dev origins and rejected unrelated origins or hosts.
- [ ] 4.3 Render the hierarchy, timeline and details with one selection; verify selecting a tool in either view updates the other view and details.
- [ ] 4.4 Add bounded-text search and error/tool/slow filters; verify a filtered-out selection retains its details and a visible explanation.
- [ ] 4.5 Add explicit content reveal with truncation; verify fake secrets stay hidden by default and markup is rendered as inert text.
- [ ] 4.6 Add empty/loading/failure states and keyboard navigation; verify retry, visible focus and stacked panels at a narrow viewport.
- [ ] 4.7 Document the completed investigation workflow; verify each command and UI action against a synthetic trace.

## 5. Integrated verification

- [ ] 5.1 Run a complete synthetic import-to-browser investigation, including parallel subagents and a recovered tool error; verify no external network requests and matching metrics.
- [ ] 5.2 Measure import and view interaction on a fixed synthetic workload; record environment and results without claiming performance beyond the measurement.
- [ ] 5.3 Review all behavior scenarios against the implementation and run the full CI checks; verify no planned behavior is advertised as complete.

## Workflow follow-up

- Archive this change only after its tracked implementation and verification tasks are complete.
- Release packaging, embedded assets, report export and live capture require later changes.
