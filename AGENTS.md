# Working on Pi Trace

Pi Trace uses Rust for analysis and a local web interface. The repository is at the bootstrap stage. Keep current behavior distinct from proposed behavior.

- Read `openspec/config.yaml` and the active change before implementing product features.
- Rust owns upstream JSONL semantics, correlation, timing and usage. The UI consumes application views.
- Use synthetic trace fixtures. Raw traces, prompts, tool output, credentials and local session files must stay outside Git.
- Preserve source file and line diagnostics. Never silently invent missing timing, parent links or token usage.
- Preserve MIT attribution in `THIRD_PARTY_NOTICES.md` when porting upstream logic.
- Run `npm run check` for local verification. Before distributing, inspect `npm pack` output and verify the extracted package can start without the source checkout.
- Root `package.json.version` owns the release version. Use `npm version patch`, `minor`, `major` or a prerelease; run `npm run version:check` before committing. Hooks do not create commits, tags or publications.

Browser support follows Vite's default production target. Use native semantic controls, visible keyboard focus and text rendering for untrusted trace content.
