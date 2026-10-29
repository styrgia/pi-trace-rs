# Design

## Context

See `proposal.md` for the motivation. The application already has a loopback Rust server and built web assets. The root npm package is now the distribution and release boundary. The trace investigation features remain in `bootstrap-trace-explorer`.

Pi's Git installation installs production dependencies. Its extension factory also runs during discovery, before a session owns resources. npm tarballs exclude package lockfiles. These constraints rule out installation-time source builds that assume frontend development dependencies exist.

## Goals / Non-Goals

**Goals:** One npm archive containing engine and interface, passive installation, explicit process ownership and one checked version across source manifests.

**Non-Goals:** No registry publication, release automation, independently published web package, new trace capture or importer implementation.

## Decisions

### One package with compiled assets

Root `package.json` declares the Pi extension, CLI, files and local commands. `web/package.json` stays a private source-build unit, outside npm workspaces. `prepack` checks versions and builds the application. The archive contains `web/dist`, `native/<platform>-<arch>/pi-trace`, startup scripts and Rust sources. The build platform is initially Linux x64. A matching installed archive starts without Cargo or frontend build tools. Other platforms can explicitly compile the included Rust sources against the bundled web assets.

The source build installs web development dependencies, builds web assets, compiles Rust with the lockfile and copies the executable into the bundled native directory. In an extracted package without web source locks, build reuses `web/dist` and compiles Rust only. There is no `prepare`, `install` or `postinstall` hook. Generated assets remain outside Git.

We compared a command launcher with a packaged setup skill. The launcher is the base because it supplies deterministic start, stop and cleanup behavior. From the alternative we adopted npm version lifecycle checks and explicit repair. A skill-only package would not satisfy the requested common application artifact.

### One owner per Pi instance

`extensions/pi-trace.mjs` registers `/pi-trace`, `/pi-trace build` and `/pi-trace stop`. A shared launcher helper owns the active operation, cancellation, child process, startup readiness and shutdown. The package directory is resolved from the module location, so the user's active repository cannot become a build or asset directory accidentally.

Build and start are mutually exclusive. Repeated starts share readiness and the owned process. Readiness requires output emitted after the Rust listener binds, followed by a valid health response. A collision therefore cannot attach to another process that happens to answer on the same port. Linux cleanup signals the owned process group and escalates after a bounded wait. Session shutdown disposes the launcher. Browser opening is a separate convenience; failure reports the URL while leaving the server alive.

### Source versions follow root metadata

Root `package.json.version` is canonical. `preversion` runs a read-only consistency check. The npm `version` hook synchronizes the web package and Cargo workspace, then asks their package managers to regenerate lock metadata. Synchronization never edits dependency resolutions directly. `.npmrc` disables automatic version commits and tags.

`version:sync` is an idempotent repair operation. A failed hook can leave some version files changed; the error explains this and provides the repair command. The source checker supports full canonical SemVer with optional prerelease and rejects build metadata. Tests use disposable local packages and real offline npm/Cargo commands.

### Local verification and short documentation

GitHub workflow definitions are removed. `npm run check` is the local entry point for version checks, behavior tests, Rust checks, web checks and strict OpenSpec validation. The README explains the current bootstrap, installation, development and links to detailed versioning and contribution guidance. Authored descriptions do not brand the frontend framework.

## Risks / Trade-offs

- A binary is specific to its build platform → include sources, describe the bundled platform and test the extracted artifact on that platform.
- A Pi session can end during a build → cancel and await the owned process group, including subprocesses.
- npm version hooks are not a multi-file transaction → fail before Git actions and provide explicit repair.
- No hosted checks run automatically → keep one documented local check command and verify before pushing.

## Migration Plan

Existing source checkouts run root `npm install` and `npm run build`. Existing version fields start at `0.1.0`. Pi users install from Git and build explicitly; packed artifacts include the compiled application. Publishing an npm version remains a separate maintainer operation after reviewing the archive. No trace files or global Pi configuration are migrated.
