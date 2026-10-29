# Spec Delta

## Purpose

Keep one release identity across the distributed application and provide a local maintainer workflow that detects and repairs version drift without publishing changes.

## ADDED Requirements

### Requirement: Check one canonical release version
The root package version SHALL own the release identity. A read-only check SHALL compare the root and internal web manifests, both npm lockfiles, the Cargo workspace version and the local crate lock entry. Missing fields, unsupported versions or drift SHALL cause a nonzero exit.

#### Scenario: Web version drifts
- **WHEN** the web manifest differs from the root version
- **THEN** the check reports the mismatched field, exits unsuccessfully and changes no files

### Requirement: Synchronize version bumps through package managers
The documented npm version command SHALL synchronize derived versions and regenerate lock data through npm and Cargo. It SHALL preserve dependency resolutions when no dependency requirements changed. Stable and canonical prerelease versions SHALL use the same spelling across manifests.

#### Scenario: Promote an alpha release
- **WHEN** npm promotes `0.1.1-alpha.1` with a patch version command
- **THEN** all owned version fields become `0.1.1` and dependency versions remain unchanged

### Requirement: Repair partial synchronization explicitly
A repair command SHALL use the root version to synchronize derived fields. Repeating a successful repair SHALL make no further changes. Failure SHALL identify the problem and state that version files can be partially updated.

#### Scenario: Interrupted version hook
- **WHEN** the root version changed but a package manager failed during synchronization
- **THEN** the maintainer can correct the failure and rerun repair to restore consistency

### Requirement: Keep release publication manual
Version checks and synchronization SHALL not create commits, tags, pushes, npm publications or hosted releases. The documented project configuration SHALL disable npm's automatic version commit and tag behavior.

#### Scenario: Successful local bump
- **WHEN** the maintainer completes the documented version command
- **THEN** version files are available for review and no Git commit or tag has been created
