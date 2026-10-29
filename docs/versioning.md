# Versioning

Pi Trace has one release version. Root `package.json` owns it. The private web package and Rust workspace follow that value. Rust and the built web interface ship in one npm package. These versioning commands prepare the source checkout. They do not publish the package or a Rust crate.

`npm run version:check` checks these fields without writing files:

- Root `package.json.version`.
- Root `package-lock.json.version` and its root package entry.
- `web/package.json.version`.
- `web/package-lock.json.version` and its root package entry.
- `Cargo.toml` under `[workspace.package]`.
- The local `pi-trace` package in `Cargo.lock`.

The Rust crate must inherit `version.workspace = true`. Both npm lockfiles use format 3. The web source has a separate private package and lockfile for development. It is not an independently published package or an npm workspace. Pi's Git installation omits development dependencies, so installation must not depend on frontend build tools being present.

## Prepare a release

Start with a clean checkout and run the development checks documented in the README. Maintain `CHANGELOG.md` by hand. Add a release entry that describes behavior users can observe.

Run a version bump from the repository root:

```sh
npm version patch --no-git-tag-version
```

Use `minor`, `major`, or an explicit full version when appropriate. The repository's `.npmrc` also disables automatic version commits and tags. Keep the flag in copied commands so their behavior is explicit.

The `preversion` hook checks the old version. npm then changes the root version and its lockfile. The `version` hook updates the web package through npm, scopes the Cargo manifest edit to `[workspace.package]`, and asks Cargo to update workspace lock entries. Package manager commands run without lifecycle scripts or network access where applicable. They do not intentionally update dependency versions.

Review the manifest and lockfile diff. Run these checks:

```sh
npm run version:check
node --test tests/version.test.mjs
```

Commit the reviewed files and create a `vX.Y.Z` tag manually. Push the commit and selected tag through the normal Git workflow. The version scripts never commit, tag, push, publish, or create a GitHub release. No GitHub Actions workflow is required.

## Use prerelease versions

Create an alpha prerelease with npm:

```sh
npm version prepatch --preid alpha --no-git-tag-version
npm version prerelease --no-git-tag-version
```

Starting at `0.1.0`, these commands produce `0.1.1-alpha.0` and `0.1.1-alpha.1`. npm `version patch` then promotes that prerelease to `0.1.1`. It does not produce `0.1.2`.

Use canonical SemVer with three numeric components and an optional prerelease. Numeric components cannot have leading zeros. `0.2.0-rc.1` is valid. `v0.2.0`, `0.2`, and `0.2.0-rc.01` are invalid manifest values. The `v` prefix belongs only in Git tag names. This project rejects build metadata such as `0.2.0+build.1` so every supported release version has one identical representation across npm and Cargo.

## Repair a failed synchronization

An npm version lifecycle is not a transaction across files. If a package manager fails, the root version or some derived files can already have changed. No commit or tag is created by the documented command.

Fix the reported error and run:

```sh
npm run version:sync
npm run version:check
```

Synchronization reads the root version and repairs derived versions. It asks npm and Cargo to regenerate their own locks. It never edits dependency resolutions by hand. A second successful synchronization changes nothing.

The root package, both npm locks, and the Cargo files must exist before synchronization starts. These commands are for maintainers in the source checkout. An installed npm tarball omits source files and lockfiles and cannot run the versioning workflow. Missing target fields or an unexpected manifest layout fail before writes. If the root version is wrong, restore or correct `package.json.version` before repair. Review all changed version files before committing.

Do not use `npm version --ignore-scripts` for release preparation. That option bypasses the synchronization hook. `version:check` detects the resulting mismatch. Do not run version bumps inside `web`; root `package.json` is the release owner.

## Implementation limits

`scripts/version.mjs` locates the repository relative to its own path. The current Cargo editor supports one double-quoted version field inside the existing `[workspace.package]` table. It rejects missing or duplicate target fields. Update the script and its behavior tests when adding another independently versioned Rust package or changing the manifest layout.

The tests build disposable local npm and Cargo fixtures. They exercise real package manager commands without registry dependencies. They verify stable bumps, prereleases, promotion, read-only drift detection, repair, idempotence, and unchanged dependency versions.

See [npm version](https://docs.npmjs.com/cli/v11/commands/npm-version/) for lifecycle order and prerelease promotion. See [cargo update](https://doc.rust-lang.org/cargo/commands/cargo-update.html) for workspace lock updates and offline operation. [Pi package installation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md) documents Git installation and package resources.
