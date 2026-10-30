# Investigate a local recording

Start Pi Trace with an offline recording directory containing `events.jsonl`.

```sh
pi-trace-rs serve "./recording directory"
```

The service imports one configured source. The browser cannot upload files or change the source. Starting without a directory opens a workspace that explains source selection. A successfully imported recording with no observations has a separate empty-report state.

## Read the report

The summary shows Rust's snapshot counts, duration and cumulative token coverage. Open **Session info** for the snapshot ID, source reference, model, input/output token breakdown and usage coverage. Unknown values stay unknown. The interface does not borrow totals from the design reference.

Select an observation in **Execution** to open its inspector. Native selection buttons work with Enter or Space. Up/Down and Home/End move between the visible selection buttons. Branch disclosure buttons expand or collapse the hierarchy independently of selection. Selection moves keyboard focus to the inspector heading. Tab reaches its controls and the next report region.

**Waterfall** and **List** share one selection. Rust supplies intervals and the global timeline domain; the browser only places those intervals on the scale. A row without a valid interval remains selectable and says that timing is unavailable. The duration column appears once in each row.

Search checks display metadata, with a limit of 256 Unicode characters. It does not search hidden prompts, arguments or results. The **Errors**, **Tools**, **Generations**, and **Over 5s** filters use Rust's matching set. The slow filter means a comparable generation/tool duration greater than 5,000 ms. Matching ancestors remain visible as context even if their branches were collapsed. Counts distinguish matches, context and loaded rows. When more results remain, **Load more observations** appends the next server page. A selected observation outside the current results or a collapsed branch keeps its independently loaded details and shows a notice.

The inspector separates duration, request-to-headers latency and first-token time. Each measurement retains Rust's recorded, derived, unknown or invalid state and meaning. Request-to-headers latency is not TTFT. Usage retains complete, partial, fallback or unknown coverage. A turn-summary fallback is labeled; absent cost is unknown. The relationship field reports root, linked, unresolved or ambiguous relationships. Only an explicitly supplied execution parent has an **Open parent** action. A failed tool and successful final interaction can coexist without a claimed retry relationship.

Open **Metadata** for occurrence IDs, model, session, structural indices and usage coverage. Source references preserve source-relative files and physical line numbers. **Diagnostics** shows structural problems and their source lines. Its **Inspect observation** buttons appear only when Rust supplies an observation target. Related diagnostics are marked; continuation may be needed to reach a later related entry.

## Inspect tool calls and model context

Use **Tools** to find actual executions. Select a tool and open **Details** for its tool name, display-only call ID, argument/result availability, selected-value size, retained-preview size, source references, and truncation evidence. A truncated call ID affects only its displayed label. Exact internal IDs still correlate starts and completions.

The selected-value size measures compact JSON bytes of the complete value recorded in that field, before Pi Trace's preview limit. The retained preview measures available UTF-8 bytes. Source truncation and local truncation are independent. Original serialized result length uses the supported collector's UTF-16 code units, or an explicitly unspecified unit for foreign provenance. That length does not describe the text sent back to the model.

Use **Inspect context** in the summary to open the **Generations** filter and select a matching generation on the **Context** tab. Ancestor rows remain context, never generation choices. **Load more observations** continues through the recorded matches and their ancestors. Open **Context** on a session, interaction, turn, or tool and choose **All recorded generations** for the same trace-wide choices. This action does not imply that every result belongs to the selected container.

On **Context**, read the selected generation's recorded input, cache-read, cache-write, and output tokens separately. **All input tokens** is a checked Rust sum only when every input constituent is available. Session totals are cumulative across requests and can repeatedly count cached context. Exact token usage per skill, tool schema, or instruction section is not recorded.

Read **Context components** for classified component JSON bytes and recognized text character counts. **Largest tool definitions** lists explicit advertised tool names ranked by whole-definition JSON bytes. **Largest skill descriptors** lists explicit advertised skill names ranked by whole-descriptor character spans. These catalogue entries do not establish actual use or relevance. Descriptions, schema bodies, skill locations, arguments, and reasoning stay outside these summaries.

**Recognized instruction sections** shows fixed system/developer delimiters. Parent and child spans overlap, include their delimiters, and must not be added. Unknown request shapes, missing evidence, partial text recognition, malformed sections, omitted list rows, and retention-budget exhaustion have separate visible states. JSON bytes use compact Rust reserialization. Characters are Unicode scalars; neither metric is a token estimate or an original wire size.

## Reveal content

**Reveal input / output**, or **Reveal arguments / recorded result** for a tool, explicitly requests recorded bodies. Tool previews are labelled **Arguments** and **Recorded result**. Selecting an observation, changing the inspector tab, searching and filtering do not fetch content. The response returns at most 8,192 UTF-8 bytes per field, identifies unavailable fields and labels truncation. All recorded text is rendered as text, including markup. Revealed previews stay in memory and disappear on reload, selection change or snapshot change.

## Restore an investigation

**View link** uses the current snapshot, selected occurrence, structural filter, execution view, inspector tab and theme. The address updates with those fields. It does not contain search text, a source path or recorded bodies. Opening the link resets search and keeps content hidden. Links from a previous service snapshot show a stale-snapshot notice. **Open current workspace** explicitly discards the old snapshot and selection.

A failed import offers **Retry import** after the configured source has been repaired. Retry cannot choose another path. While importing, the interface polls recording state and publishes no partial report. Failed or ready states stop that polling. A network failure offers **Reconnect**, which reads the current workspace. A changed snapshot is explained before an old selection is reused.

Use **Dark** or **Light** in the project bar to switch the local view theme. On narrow screens the inspector stacks below execution. All report controls use native buttons, links, search inputs or disclosures, with visible keyboard focus and status announcements.

## API and bounds

The interface consumes the following local JSON endpoints. Snapshot views include `snapshotId`; node and snapshot IDs are meaningful within the current process snapshot.

| Request | Response |
| --- | --- |
| `GET /api/v1/workspace` | `empty`, `importing`, `ready` or `failed` recording state |
| `POST /api/v1/retry` | Accepted failed-source retry and its attempt ID |
| `GET /api/v1/snapshots/{snapshot}/overview` | Summary, counts, availability and global timeline domain |
| `GET /api/v1/snapshots/{snapshot}/observations` | Ordered metadata rows, match/context counts and continuation |
| `GET /api/v1/snapshots/{snapshot}/nodes/{node}` | Metadata, measurements, sources and diagnostic references |
| `GET /api/v1/snapshots/{snapshot}/nodes/{node}/content` | Explicit bounded input/output previews |
| `GET /api/v1/snapshots/{snapshot}/diagnostics` | Structural diagnostics and continuation |

Observation query keys are `q`, `filter`, `cursor` and `limit`. Filters are `error`, `tool`, `generation`, and `slow`. Pages default to 100 items and allow at most 200. Opaque continuation tokens bind to their snapshot and query. Serialized view responses are bounded to 1 MiB. Inspection keeps at most 16 largest components, tool definitions, and skill descriptors, 32 sections, 64 KiB per node, and 16 MiB retained metadata per snapshot. Exhaustion limits inspection without rejecting an otherwise valid recording. Labels and diagnostic messages are bounded to 512 UTF-8 bytes; a truncated label carries its own indicator.

Errors are JSON `{ "error": { "code": "…", "message": "…", "source": null } }`, with an optional non-null `{ "file": "…", "line": 1 }` source. Unknown snapshots and nodes have distinct 404 codes. Invalid queries use 400. Unready views and conflicting retries use 409. The client validates responses at the network boundary and reports incompatible responses visibly.

Trace responses use `Cache-Control: no-store`. The service binds to loopback and checks Host and present Origin headers. Read clients may omit Origin. Retry requires an approved Origin and accepts no payload or source parameter. For a local command-line retry, use an approved origin explicitly.

```sh
curl -X POST -H 'Origin: http://127.0.0.1:4317' http://127.0.0.1:4317/api/v1/retry
```

Metadata can still be sensitive. Source selection, import limits and application views remain Rust's responsibility. The browser does not reconstruct upstream JSONL semantics.
