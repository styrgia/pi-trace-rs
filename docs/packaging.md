# One package for the application

`pi-trace-rs` is the only npm distribution. It contains the Rust executable, compiled web assets, Pi extension and startup scripts. The private `web/package.json` is used to build the interface from source and is not published separately.

## Build and inspect an archive

From a source checkout with Node.js 24, npm and stable Rust:

```sh
npm install
npm run check
npm pack
```

To install web dependencies, build both parts, and start from source in one command, run `npm run launch -- system`. Use `npm run launch -- serve "recordings/my session"` or `npm run launch -- watch "recordings/current session"` for collector recordings. With no arguments, `npm run launch` opens an empty workspace. This command rebuilds on each invocation. Installed packages use `pi-trace-rs` directly and need no build tools on the bundled platform.

The `prepack` hook checks the shared version and builds the application. The resulting `pi-trace-rs-VERSION.tgz` includes:

- `native/PLATFORM-ARCH/pi-trace`, with `.exe` on Windows.
- `web/dist`, containing the interface and its local assets.
- `extensions`, `scripts`, the root manifest and release notes.
- Rust sources, Cargo manifests and the Cargo lockfile for an explicit source build.
- Project licensing and upstream attribution.

It excludes traces, environment files, development dependencies, build caches, development-only agent skills and GitHub workflows. npm excludes package lockfiles from tarballs, so installed packages do not run the source-checkout versioning workflow.

The archive contains the executable for the machine that built it. The initial verified build is Linux x64. A matching archive starts without Cargo or frontend build tools. On a different platform, `/pi-trace build` can compile the included Rust sources while reusing the bundled web assets; other platforms are not verified yet. There are no separate platform npm packages.

## Pi installation

The current source installation is:

```sh
pi install git:github.com/styrgia/pi-trace-rs
```

Reload Pi, then run `/pi-trace build` and `/pi-trace serve <directory>` for offline import or `/pi-trace watch <directory>` for realtime following. Git installation contains source files, so the explicit build installs web development dependencies and compiles Rust. A packed npm installation already carries assets and its matching executable.

The package declares one extension through `pi.extensions`. It imports no runtime dependency; Pi provides its extension host. Installation and resource discovery do not build or start the application. Only an explicit command starts work. `/pi-trace stop` and session shutdown clean up the process owned by that extension instance. Port conflicts do not attach to or stop another service.

Browser opening from Pi is attempted on Linux. The local URL is always reported. Set `PI_TRACE_NO_OPEN=1` for a headless environment.

## Select an offline recording

The installed command and `npm start` run the same application. Select one recording directory at startup:

```sh
pi-trace-rs serve "recordings/my session"
npm start -- serve "recordings/my session"
```

Relative recording paths resolve from the caller's working directory. Assets and the executable resolve from the package directory. To run an installed package or source checkout from another directory, use its path:

```sh
cd /path/to/recordings
node /path/to/pi-trace-rs/scripts/start.mjs serve "my session"
npm --prefix /path/to/pi-trace-rs start -- serve "my session"
```

The npm launcher uses npm's `INIT_CWD` for this package's `npm start` lifecycle when npm runs in the package directory. Direct CLI calls and other npm scripts use their actual working directory. It passes the resolved source to Rust as one argument without a shell. Do not pass multiple directories. Omit `serve` and its directory to start an empty workspace.

In Pi, use `/pi-trace serve recordings/my session`. Use one space after `serve`. The command treats the entire remainder after that separator as one path relative to the Pi session's working directory, including leading and trailing spaces. Spaces need no quotes in Pi. A repeated start for the same resolved path and mode shares the owned process, including while it is starting. Selecting another path requires `/pi-trace stop` followed by a new start. Offline and watch modes cannot share an owned process. Switching modes requires stop and a new start. Empty-workspace mode counts as a different source from a selected recording.

A healthy listener can report an import failure. Repair the configured directory, then choose **Retry import** in the workbench to read that same source again. Retry cannot change the source. If the server is unavailable, restart it and choose **Reconnect**. Old snapshot links can become stale after restart. The workbench offers the current workspace instead of assigning an old selection to another snapshot.

## Follow a recording directory

An external collector must append complete JSONL records. Pi Trace does not start capture or modify recording files. Select watch mode explicitly:

```sh
pi-trace-rs watch "recordings/current session"
npm start -- watch "recordings/current session"
cd /path/to/recordings
node /path/to/pi-trace-rs/scripts/start.mjs watch "current session"
npm --prefix /path/to/pi-trace-rs start -- watch "current session"
```

The same caller-directory and path-space rules apply to both modes. In Pi, use `/pi-trace watch recordings/current session` without shell quotes. Watch requires one directory. `serve` still imports once, including its supported newline-free final record; watch waits for a complete newline before accepting that record.

Pause and resume apply to the shared workspace across tabs. Pause does not stop the collector, and resume consumes its backlog in bounded batches. Source replacement, truncation, disappearance, and observable rewriting stop ingestion. The last valid snapshot remains visible. **Retry live run** rereads the same configured directory under a new run identity and clears the prior selection. Source selection through HTTP is unsupported.

Watch currently requires Unix file identity support. Same-size rewrites are detected when observable metadata changes or a sampled consumed boundary differs. Interior rewrites with preserved metadata, or an unsampled interior rewrite combined with growth, cannot always be detected. Producers must remain append-only. See [compatibility](upstream-compatibility.md) and [live verification](live-verification.md) for the supported contract and evidence.

## Publication

Inspect the archive and test an installation outside the source checkout before publishing. The manifest supports a common npm package; registry publication is a separate maintainer action. After publishing a version, the Pi command is `pi install npm:pi-trace-rs@VERSION`.

There are no automatic publish, push or release workflows. Review the version diff, release notes and packaged files, then perform any publication and Git tagging explicitly.


## Start native system mode

The same package exposes the native catalog through the CLI and Pi extension:

```sh
pi-trace-rs system
npm start -- system
pi-trace-rs system --root "codex=./saved Codex home" --root "pi=./saved Pi sessions" --disable-provider claude --disable-provider opencode
```

Repeat `--root <provider>=<path>` for multiple distinct stores. One provider's explicit roots replace its defaults. Providers are `codex`, `pi`, `claude` and `opencode`. Canonical aliases deduplicate, and a provider cannot be both configured and disabled. [Native source configuration](upstream-compatibility.md#native-source-configuration) documents environment precedence and OpenCode file boundaries.

From an unrelated directory, invoke the installed package script directly:

```sh
cd /path/to/fabricated-stores
node /path/to/pi-trace-rs/scripts/start.mjs system --root "pi=project sessions" --disable-provider codex --disable-provider claude --disable-provider opencode
```

Relative roots resolve before the launcher enters the package directory. The package's `npm start` lifecycle retains the caller-directory rule described above. Node passes sorted, fully resolved roots for every enabled provider and explicit disable flags to Rust without a shell. Assets still resolve from the package, and the native listener remains `127.0.0.1:4317`.

In Pi, use `/pi-trace system` with no arguments. It inherits supported provider settings and shares the extension's existing owned-process lifecycle. The extension does not parse root options or register capture hooks. Use the terminal CLI for explicit roots. `/pi-trace stop`, session shutdown and interrupted CLI startup dispose owned work. Different modes or effective provider configurations produce an explicit conflict; the launcher does not reuse or terminate an unrelated listener.

The archive's native executable includes bundled SQLite and zstd support. Native reading does not require a provider executable, daemon, external `sqlite3` command or network service. Startup selects an existing bundled executable or an already-built source executable; it does not silently compile. A matching extracted package therefore needs its packaged executable and web assets, with no source-checkout fallback required by design. The existing bundled-platform qualification still applies; native system mode adds no claim of verified cross-platform behavior.

Before distributing a package with system mode, inspect `npm pack` output and run it from an unrelated directory against fabricated stores for all four providers. Include compressed Codex history, SQLite WAL updates, new-session discovery, selected-session updates, configuration conflicts, source immutability and owned-worker shutdown. Confirm that no build tool, provider process or source checkout supplies missing runtime files. The existing `prepack` build is not evidence that this extracted-package check passed.

Real browser, workload and extracted-package results are recorded in [system-session verification](system-session-verification.md). Documentation of implemented commands does not authorize publication, a version change, a commit or a tag.
