# Synthetic recording expectations

All data in this directory is fabricated. No captured session, local path, credential, or user prompt is included.

`reference/events.jsonl` follows the pinned collector shapes described in `docs/upstream-compatibility.md`. It contains exactly 14 observations: one session, one interaction, two turns, four generations, and six tools. The first generation starts on line 4. Its headers take 120 ms and completed generation takes 900 ms. The second generation takes 5,500 ms. Each generation records 170 total tokens from input 100, output 50, cache read 20, and cache write 0. Turn summaries overlap those generations, so the overview total is 680 rather than 1,020. Cost and TTFT are unknown.

The `bash` tool fails. A later completed generation and final `stop` summary complete the interaction successfully. No relationship identifies a retry of `bash`. Prompt, argument, and result strings begin with `FABRICATED_PRIVATE` to detect leaks through metadata. The result markup is inert text for browser reveal checks.

`edge/events.jsonl` adds a backward generation interval, a generation without completion, partial usage, a partial summary fallback, an unknown event, a malformed line, and an incomplete tail. Its generation input exceeds 8 KiB with multibyte text. This file supplies the unknown, invalid, partial, unavailable, and truncated DTO examples.

`tests/semantics.rs` constructs additional temporary recordings for source limits, repeated indices, delayed tool results, reverse child completion, ambiguous child IDs, symlink escapes, and 225 rows of ancestor context. Unit tests in `src/import.rs` mutate a synthetic source between reading and publication to verify source-change refusal. `src/lib.rs` regenerates the serialized contract examples through the actual importer.
