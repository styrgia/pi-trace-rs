# Proposal

## Why

The project currently requires separate build commands and has no installable Pi entry point or shared release version. Users need one package containing the engine and interface, and maintainers need a predictable local release workflow.

## What Changes

- Add a root npm package containing the Rust executable, built web assets, Rust sources and Pi extension.
- Add explicit build, start and stop commands without building or starting a server during installation.
- Synchronize one release version across npm metadata and the Rust workspace.
- Remove GitHub workflows and expose local checks through the root package.
- Shorten the README and remove frontend framework branding from authored descriptions and documentation.

## Capabilities

### New Capabilities

- `pi-distribution`: Install one package and manage its local application through Pi commands.
- `release-versioning`: Check, bump and repair one consistent version across the project.

### Modified Capabilities

None. The trace importer and analyzer remain planned in their existing change.

## Impact

Root package metadata, packaging and launcher scripts, an extension entry point, release notes and local verification commands. The packed archive initially contains a binary for the build platform; other platforms use the included Rust sources. npm registry publication remains a separate maintainer action.
