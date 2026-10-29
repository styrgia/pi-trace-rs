# Contributing

The project is being initialized. Read the active OpenSpec change before selecting a task. Keep pull requests small enough to verify one behavior at a time.

Run `npm install`, `npm run build` and `npm run check` from the repository root, and describe the behavior you verified. UI changes need a real browser check. Trace interpretation changes need synthetic fixtures covering the affected event sequence. GitHub workflows are not used.

Use fabricated session IDs, prompts, paths and tool output in fixtures. Do not attach real agent traces to public issues. Commit lockfiles with dependency changes. New dependencies must have an OSS license compatible with MIT distribution.

Root `package.json` owns the release version. Follow [versioning](docs/versioning.md) for a bump and [packaging](docs/packaging.md) to inspect the common Rust and web archive. Keep `web` private; do not publish it separately.

Report bugs through [GitHub issues](https://github.com/styrgia/pi-trace-rs/issues). Include reproduction steps, expected behavior and a minimal synthetic input when relevant.
