# Proposal

## Why

The HTML workbench provides a clearer trace report, but the Svelte application still displays a connection check and Rust serves only health and static assets. Users need that report to inspect an actual recording, with Rust supplying trustworthy observations, measurements, and diagnostics.

## What Changes

- Replace the Svelte starter with the workbench from `design-concepts/trace-workbench`, including the quieter summary, readable execution rows, linked waterfall and list, inspector, diagnostics, and both themes.
- Implement the offline import and analysis needed for the report in `crates/trace-core`. Reuse the measurement, identity, provenance, and uncertainty requirements already proposed in `bootstrap-trace-explorer`.
- Add bounded application-view endpoints to the Rust service. The browser receives normalized observations and summaries, never upstream JSONL or a copy of the prototype fixture.
- Select one recording directory explicitly at process startup. Preserve no-argument startup as an empty workspace, package-relative assets, and the launcher's ownership of its process.
- Provide real loading, empty, import-error, unavailable-service, and recovery states. Keep prompts, arguments, and result bodies behind an explicit bounded content request.
- Verify a synthetic recording through the real importer, HTTP API, and browser, followed by the existing package verification before distribution.
- Extend the accepted workbench with tool-call evidence and a request Context inspector. Rust measures complete request composition before preview truncation, while the UI distinguishes catalogue sizes, history, recorded request usage, and cumulative session usage. Arguments and result bodies retain their explicit reveal boundary.

## Capabilities

### New Capabilities

- `trace-workbench`: A readable, responsive investigation screen with one selection across views, deliberate content reveal, URL view state, and honest load and error states.
- `trace-view-api`: A local, bounded HTTP contract for a startup-selected recording, immutable snapshot identity, observation queries, details, content previews, and source diagnostics.

### Modified Capabilities

None. `openspec/specs/` has no published capabilities yet. The active bootstrap change already proposes `trace-import`, `trace-analysis`, and `trace-explorer`; their semantic requirements remain the baseline rather than being copied into competing deltas.

## Impact

Implementation affects `web/src/App.svelte`, `web/src/app.css`, new workbench components and typed API code, `crates/pi-trace`, the new `crates/trace-core` workspace member, launcher argument handling, the Pi command, synthetic fixtures, and integration checks. The installed application remains one root npm package. The HTML concept remains an isolated design reference.

This change supplies the concrete implementation checklist for the overlapping unfinished bootstrap work. Its design maps that overlap and records refinements to startup and API behavior. Existing bootstrap tasks stay unchecked until their scenarios are verified; creating this proposal does not implement or complete them.

The scope excludes capture, live updates, hosted services, accounts, a database, arbitrary filesystem browsing, static report export, and publication. Existing MIT attribution and root-owned versioning remain in force.
