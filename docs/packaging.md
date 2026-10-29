# One package for the application

`pi-trace-rs` is the only npm distribution. It contains the Rust executable, compiled web assets, Pi extension and startup scripts. The private `web/package.json` is used to build the interface from source and is not published separately.

## Build and inspect an archive

From a source checkout with Node.js 24, npm and stable Rust:

```sh
npm install
npm run check
npm pack
```

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

Reload Pi, then run `/pi-trace build` and `/pi-trace`. Git installation contains source files, so the explicit build installs web development dependencies and compiles Rust. A packed npm installation already carries assets and its matching executable.

The package declares one extension through `pi.extensions`. It imports no runtime dependency; Pi provides its extension host. Installation and resource discovery do not build or start the application. Only an explicit command starts work. `/pi-trace stop` and session shutdown clean up the process owned by that extension instance. Port conflicts do not attach to or stop another service.

`npm start` runs the same bundled application from a source checkout or installed package. The CLI resolves assets relative to its package directory, so the shell's working directory does not select files to serve. Browser opening from Pi is attempted on Linux; the local URL is always reported. Set `PI_TRACE_NO_OPEN=1` for a headless environment.

## Publication

Inspect the archive and test an installation outside the source checkout before publishing. The manifest supports a common npm package; registry publication is a separate maintainer action. After publishing a version, the Pi command is `pi install npm:pi-trace-rs@VERSION`.

There are no automatic publish, push or release workflows. Review the version diff, release notes and packaged files, then perform any publication and Git tagging explicitly.
