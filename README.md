# Pi Trace

A local browser app for inspecting Pi coding agent traces, with a Rust engine. It is intended to help you find slow generations, failed tools and expensive turns in recorded sessions.

The server and connection check work. Trace import, analysis and the timeline are still planned in [OpenSpec](openspec/changes/bootstrap-trace-explorer/proposal.md). This version does not record or load traces.

## Use with Pi

Install Node.js 24 LTS and Pi. A source installation also needs stable Rust with Cargo:

```sh
pi install git:github.com/styrgia/pi-trace-rs
```

In Pi, build once, then start the app:

```text
/pi-trace build
/pi-trace
```

Open [127.0.0.1:4317](http://127.0.0.1:4317) and select **Check connection**. Stop it with `/pi-trace stop`; leaving the Pi session also stops its server.

The running app makes no requests to a hosted service.

The npm archive contains the Rust executable and web interface together. Its bundled platform runs without compilation. See [packaging](docs/packaging.md) for source builds and platform support.

## Development

Run from the repository root:

```sh
git clone https://github.com/styrgia/pi-trace-rs.git
cd pi-trace-rs
npm install
npm run build
npm start
```

Run `npm run check` before submitting changes. Use `npm version patch --no-git-tag-version` to bump the shared version, then review and commit it. [Versioning](docs/versioning.md) covers prereleases and repair.

See [contributing](CONTRIBUTING.md) and [compatibility notes](docs/upstream-compatibility.md). Use synthetic fixtures: real recordings can contain credentials, prompts and private code.

[MIT licensed](LICENSE). Pi Trace is an independent project.
