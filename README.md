# Pi Trace

A local web explorer for Pi coding agent traces, with analysis in Rust and an interface built with Svelte. The first release is being designed to help you find slow generations, failed tools and expensive turns in existing `pi-trace-extension` recordings.

**Status: development scaffold.** The server, Svelte app and connection check run today. Trace import, analysis and the timeline are planned in [the first OpenSpec change](openspec/changes/bootstrap-trace-explorer/proposal.md).

- **Local development.** The Rust server listens on `127.0.0.1:4317`. The starter makes no requests to a hosted service.
- **Rust + Svelte.** Rust will own trace semantics; Svelte will own navigation, filters and presentation.
- **Open source.** MIT license, with attribution to [pi-trace-extension](https://github.com/npxcnency-ux/pi-trace-extension).
- **Spec first.** Proposal, design, behavior scenarios and implementation tasks live in OpenSpec.

## Run the scaffold

Install stable Rust with Cargo and Node.js 24 LTS with npm. Clone the repository, build the web app, then start the Rust server:

```sh
git clone https://github.com/styrgia/pi-trace-rs.git
cd pi-trace-rs
npm --prefix web ci
npm --prefix web run build
cargo run -p pi-trace --locked
```

Run the server from the repository root so it can find `web/dist`. Open [127.0.0.1:4317](http://127.0.0.1:4317) and select **Check connection**. The page will report the Rust server's version and bootstrap status. Stop the server with Ctrl+C.

For hot reload, keep the Rust server running and use another terminal:

```sh
npm --prefix web run dev
```

Open [127.0.0.1:5173](http://127.0.0.1:5173). Vite proxies `/api` to Rust. Both processes bind to loopback.

## Planned first release

- Import `events.jsonl` and child traces recorded by the upstream Pi extension.
- Inspect a synchronized hierarchy, timeline and details panel.
- Filter slow calls, tool failures and subagents; search bounded text previews.
- View token usage and cost without counting step and turn summaries twice.
- See incomplete traces, malformed lines and unknown timing as diagnostics.

The Rust application will initially import existing recordings. Pi extensions run as TypeScript, so this project does not yet replace the collector or provide live capture. Raw traces can contain secrets and private source text. Use synthetic data for contributions.

## Development checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm --prefix web run check
npm --prefix web run build
npx --yes --package @fission-ai/openspec@1.15.0 openspec validate --all --strict --no-interactive
```

The current scaffold has no unit tests. Verification includes compilation, static checks and a browser check of the real health request. Behavior tests for the importer and analyzer are part of the tracked work.

OpenSpec is configured for Codex and Pi. Start with [the proposal](openspec/changes/bootstrap-trace-explorer/proposal.md), [design](openspec/changes/bootstrap-trace-explorer/design.md) and [tasks](openspec/changes/bootstrap-trace-explorer/tasks.md). Contributors can use `$openspec-apply-change` in Codex or `/opsx-apply` in Pi after reviewing the change.

## Layout

```text
crates/pi-trace/   Rust local HTTP server
web/              Svelte 5 + TypeScript + Vite
openspec/         Behavior contracts and active changes
docs/             Architecture and compatibility notes
```

## License and upstream

[MIT](LICENSE). This is an independent rewrite inspired by `pi-trace-extension`, not an official Pi project. The compatibility baseline is upstream commit `5441bcca041e3b3a203aee91c68e8ea809e6aa27`. See [third-party notices](THIRD_PARTY_NOTICES.md) and [compatibility notes](docs/upstream-compatibility.md).
