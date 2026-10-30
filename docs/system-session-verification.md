# System-session verification

This record covers the synthetic acceptance of `add-system-session-realtime`. The source checkout is the authoritative integration base, including its existing collector realtime and branding changes. No real provider home, prompt, credential, agent process registry or personal session was read. Release publication and cross-platform qualification are outside this verification.

## Reproduce

Use Node 26.13.0 for the acceptance producers. The package runtime requires Node 24 or newer; the source-only producer additionally uses Node's built-in SQLite writer and zstd compression. Install Playwright 1.64.1 in a separate local tooling directory and point `PI_TRACE_PLAYWRIGHT_MODULE` at its `index.mjs`. The recorded browser is system Chrome 155.0.8059.79 on Linux x86-64.

Build production assets and the release executable:

```sh
npm --prefix web run build
cargo build --release --locked -p pi-trace
```

The source-only drivers live under `tests/system-session/`. They fabricate their own stores outside the checkout. They are excluded from the npm archive. Set absolute paths for the package, binary and artifacts:

```sh
export PI_TRACE_VERIFY_PACKAGE="$PWD"
export PI_TRACE_VERIFY_BINARY="$PWD/target/release/pi-trace"
export PI_TRACE_VERIFY_ARTIFACTS="$PWD/.audit/system-session/workload-verified"
export PI_TRACE_PLAYWRIGHT_MODULE=/absolute/tooling/node_modules/playwright/index.mjs
node tests/system-session/verify-workload.mjs
```

The service listens on its normal port, 4317. If another session owns that port, preserve it and run a driver in a private network namespace with the same mapped uid:

```sh
unshare --user --map-current-user --keep-caps --net -- \
  /bin/sh tests/system-session/in-network.sh \
  node tests/system-session/verify-workload.mjs
```

`in-network.sh` enables loopback and executes the supplied command. Keeping the current uid avoids changing SQLite ownership behavior. This requires Linux user/network namespace support and `ip`; it does not change the product's networking policy. Every driver starts its own service, waits for that child's readiness marker and checks its exit. It never attaches to or stops another listener.

Run `verify-browser.mjs` and `verify-backlog.mjs` the same way, with separate artifact directories. The package driver requires `PI_TRACE_VERIFY_PACKAGE` to name an extracted package. It derives its packaged native binary itself. Its child processes receive a PATH containing only a Node symlink, so provider executables, Cargo, npm and external SQLite cannot supply missing runtime dependencies.

## Synthetic fixture identity and expected values

`fixture-store.mjs` creates Codex legacy rollout JSONL, Pi version-3 session trees, Claude transcript records and OpenCode version-2 SQLite tables. A separate Node writer keeps the database in WAL mode while the native service reads it. Tool evidence uses matching native call/result identifiers and fabricated markup. Provider pins, older profiles, paginated history, fork boundaries, cumulative authority and other semantic cases are enumerated in `crates/trace-core/tests/fixtures/NATIVE.md` and [upstream compatibility](upstream-compatibility.md#native-compatibility-profiles).

The workload starts with 249 historical sessions per provider. Four newly created sessions raise the catalog from 996 to 1,000. The same fabricated native session ID is deliberately used across providers; opaque catalog IDs must remain distinct. Each changing session begins with 130 tokens. The producer commits one response every 100 ms for 30 seconds, rotating Codex, Pi, Claude and OpenCode with respective contributions of 130, 140, 150 and 160 tokens.

| Provider | Final non-cached input | Final output | Cache read | Cache write | Final total | Accepted contributions |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Codex | 6,080 | 2,280 | 1,520 | 0 | 9,880 | 76 |
| Pi Agent | 6,830 | 2,280 | 1,520 | 0 | 10,630 | 76 |
| Claude Code | 7,580 | 2,280 | 1,520 | 0 | 11,380 | 76 |
| OpenCode | 8,330 | 2,280 | 1,520 | 0 | 12,130 | 76 |

Codex's recorded cached-input subset is subtracted from input, and its `cache_write_input_tokens: 0` remains an exact zero. OpenCode's 25 visible output tokens and five reasoning tokens normalize to 30 output tokens. These component checks supplement the visible total and prevent a matching grand total from hiding a component mapping error.

## Evidence boundaries

Core tests establish provider semantics and source locators. HTTP tests serialize actual Rust views and exercise guards, limits and snapshot binding. Controlled browser-model tests establish cancellation and stale-response behavior. A passing build alone establishes none of the runtime acceptance below.

The workload opens four independent visible browser contexts. A DOM observer records selected session, run, snapshot and stable span, plus row and inspector snapshot IDs, exact token values and states. A frame must belong to the selected session and have equal row/inspector values from the same snapshot. The measurement comparator rejects deliberately swapped session, snapshot, counter and coverage values before the real run.

Every committed response must be acknowledged by a coherent visible total that includes it. A later cumulative frame may acknowledge intervening records only when its exact value proves inclusion. Latency is render wall time minus producer commit completion. A frame recorded between a completed write and its Node callback has zero latency, with the producer's pre-write timestamp used as the lower observation boundary. This is persisted-evidence freshness, not provider request or token-stream latency.

Catalog appearance uses the producer's new-session creation timestamp. Initial historical sweep completion is measured separately after owned-service readiness. CPU is native-process CPU time divided by the observed wall interval, expressed relative to one core. RSS is sampled separately from the conservative input-equivalent retention charge. Decoder read counters exclude discovery/header probes, metadata, SQLite scalar checks and continuity probes; they are not total filesystem I/O.

## Functional browser acceptance

The real service and Chrome scenarios cover all four providers, call/result updates, exact usage updates, deliberate body reveal, literal untrusted markup, body hiding after a revision, a held old-session response during switching, selection outside provider filters, independent clients, Pi rewrite/run reset, malformed Codex middle records, Claude retention deletion/restoration, OpenCode schema failure/recovery and new-session discovery after recovery. Screenshots cover light and dark themes at 1440 and 375 pixels. Keyboard focus and horizontal overflow are checked separately in the workload driver.

The final functional run is `.audit/system-session/browser-final-verified/`. Its `result.json` records all fourteen scenarios, zero browser errors, zero external requests and clean shutdown; `network.json` records actual requests. The directory contains 32 screenshots, including tool/result and recovery states in both themes at both viewport widths. The workload adds 16 screenshots and visible keyboard-focus checks.

## Declared workload and resource acceptance

The final release run is `.audit/system-session/workload-verified/`. It passed all 300 committed-response samples over the declared 30 seconds. Selected rows and inspector agreed in every observed frame. Maximum selected latency was **779 ms**; empirical p50, p95 and p99 were 396, 670 and 741 ms. All four new sessions appeared within **985 ms**. Browser HTTP failures, browser errors, external requests and implicit content requests were all zero. The service exited with code zero.

The first post-readiness sweep probe already found all 996 historical sessions complete; that probe took 1 ms. This does not measure total startup or indexing time before readiness. The resulting catalog contained all 1,000 sessions with complete coverage. The exact final values matched the table above.

Machine: AMD Ryzen 7 7800X3D, 16 logical CPUs, 66,587,029,504 bytes RAM, Linux 6.8.0-139-generic, Node 26.13.0 and Chrome 155.0.8059.79. Initial load averages were 1.65, 2.54 and 2.65. No implementation build ran during this measurement. This is one local threshold check, with no baseline speedup or universal latency claim.

| Native-process observation | Recorded result |
| --- | --- |
| Idle CPU, 2,009 ms wall interval | 40 ms CPU, 1.99% of one core |
| Active CPU, 30,312 ms wall interval | 2,310 ms CPU, 7.62% of one core |
| Idle RSS / active end RSS | 18,169,856 / 57,679,872 bytes |
| Idle decoder records / bytes | 7 / 2,954, unchanged across the idle interval |
| Idle catalog revision and parse-job count | Stable revision; five jobs before and after |
| Final conservative retained charge | 479,833 bytes, below the 268,435,456-byte global limit |
| Loaded analyses / maximum simultaneous parse jobs / database connections | 4 / 2 / 1 |
| Final actual decoder work | 3,157 records and 1,235,021 bytes |

RSS values are samples, not measured peaks. OpenCode rereconciles the selected session's row set transactionally as it changes, so its decoder-work count can exceed the 300 newly committed records. The 996 cold histories were not semantically decoded on each poll. Deterministic runtime tests separately exercise concurrent reservations, large-to-small SQLite replacement with an old retained snapshot, global connection exhaustion, active-capacity admission, eviction, input limits and snapshot retirement.

Receipts include `result.json`, producer `commits.json`, `latencies.json`, `browser-frames.json`, `network.json`, `responses.json`, `status-samples.json` and screenshots. The measured release binary SHA-256 is `98e81c79624253a3dad491ff04caf0dae838bcc5c5ace51cba744a31f2ea9f22`. Failed intermediate attempts remain `not_verified` in separate directories and are not counted as acceptance.

The separate backlog driver adds 2,000 historical descriptors per provider, for 8,004 total sessions across eight stores. File headers carry 48 KiB of ignored synthetic metadata to exercise bounded header I/O. All four selected writers must advance before that first historical sweep completes, and only selected histories may enter the semantic decoder. The accepted run, `.audit/system-session/backlog-acceptance/result.json`, completed its initial sweep in 659 ms from process launch. Claude, Pi, Codex and OpenCode acknowledged their updates in 105, 208, 260 and 320 ms while the sweep was still incomplete. It retained all 8,004 descriptors, loaded four analyses, and decoded only eleven selected records, totaling 5,027 bytes. It used two database connections and at most one simultaneous parse job. This is a separate fairness check, not the declared 1,000-session latency workload. Header probes and their synthetic padding are outside semantic decoder counters.

## Extracted package

The candidate archive passed `verify-package.mjs`; its receipts are `.audit/system-session/package-candidate/result.json` and `.audit/system-session/pack-candidate/inspection.json`. It extracted outside the checkout into a path containing spaces and started from another cwd. All four providers returned exact 130-token totals. Codex used actual zstd history; OpenCode used SQLite/WAL data. A newly created Pi session appeared and then updated to 260 tokens.

The package check compared static database, WAL, SHM, file bytes and modes, rejected an independent port conflict without stopping the original owner, reused one launcher's canonical aliases and reordered roots, rejected mode/configuration changes, and shut down owned workers. Empty, offline and collector-watch starts also passed. Child PATH contained only Node; no provider executable, Cargo, npm, external SQLite or source checkout supplied runtime files. The packaged native binary hash matched the measured source release binary.

The final archive is written beneath `.audit/system-session/pack-verified/`; its manifest and extracted-root identity are in `inspection.json`. The matching extracted run writes `.audit/system-session/package-verified/result.json`. Reproduce the driver with that extracted root and another artifact directory:

```sh
PI_TRACE_VERIFY_PACKAGE="/absolute/extracted package/package" \
PI_TRACE_VERIFY_ARTIFACTS="$PWD/.audit/system-session/package-rerun" \
unshare --user --map-current-user --keep-caps --net -- \
  /bin/sh tests/system-session/in-network.sh \
  node tests/system-session/verify-package.mjs
```

The allowlist includes runtime scripts, Rust sources, bundled native executable, production assets, documentation and licenses. It excludes source-only tests, fixtures, generated stores, `.audit`, build tooling dependencies and raw traces. Bundled font and third-party notices are preserved. Dynamic-library inspection showed no external SQLite or zstd library requirement on this bundled Linux x86-64 build.

## Limits and corrections found during acceptance

Acceptance found and fixed canonical Codex cache-write naming, a sticky stale state after an unchanged transcript returned, native request bursts across multiple tabs, and unbuffered byte-at-a-time header I/O. Regression tests and real reruns qualify the fixes. The global eight-handler policy remains in force. Native requests are serialized per browser tab with at most eight queued requests; cancellation removes queued work, and the existing ten-second request timeout includes queue wait. This reduces normal request bursts without promising unlimited concurrent clients.

Native mode follows persisted records. Unsaved provider streams and process liveness remain unavailable. Supported file continuity checks observe inode, truncation, metadata and the consumed tail; they cannot prove absence of arbitrary earlier edits preserving those observations. SQLite WAL reading requires usable existing sidecars. The retained-data budget counts conservative decoded-input equivalents and is not an RSS cap. See [the API reference](trace-api.md#scheduling-and-retained-data-limits) for exact bounds.

The final `npm run check` exited zero: 113 Node tests, 137 Rust tests, Svelte with zero errors and warnings, production build, synchronized version 0.1.0 and strict validation of all five changes. The final check and reconciliation receipts are recorded in `.audit/system-session/final-check.json` and `.audit/system-session/preservation.json`. The preservation audit compares 164 unowned paths against the pre-integration snapshot: 146 existing files retain their hashes and 18 already absent paths remain absent. All matched at the recorded check. OpenSpec completion is tracked in the change tasks and verified through refreshed apply instructions. Only a matching successful final check and final package receipt establish completion. No commit, version bump, tag, archive, push or publication is part of this apply operation.
