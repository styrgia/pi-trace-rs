# Tasks

## 1. Common distribution and Pi entry point

- [x] 1.1 Add the root package manifest and explicit resources; verify version checking and passive Pi extension discovery.
- [x] 1.2 Add owned build/start/stop lifecycle with source and bundled executable paths; verify concurrency, collision, cancellation and cleanup behavior tests.
- [x] 1.3 Pack Rust and web into one archive; inspect contents and verify an installed copy starts outside the checkout without build tools.
- [x] 1.4 Verify the real Pi loader discovers the extracted package and registers its command without starting resources.

## 2. Release version consistency

- [x] 2.1 Add read-only version checks and synchronization hooks; verify drift, repair, idempotence and missing-field behavior tests.
- [x] 2.2 Verify stable bumps, prerelease increments and promotion with real npm and Cargo fixtures, preserving dependency resolutions and avoiding Git actions.
- [x] 2.3 Document the manual release workflow and partial-failure repair; verify the documented commands match the root scripts and npm configuration.

## 3. Documentation and local workflow

- [x] 3.1 Remove workflow definitions and add one root local-check command; verify the full command completes successfully.
- [x] 3.2 Shorten the README to approximately half its former length and remove frontend framework branding from authored descriptions; verify the word count and current command names.
- [ ] 3.3 Review package and specification consistency, then commit and push; verify the final working tree and remote head.
