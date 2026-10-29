# Pi Trace execution workbench concept

An isolated, disposable HTML/CSS/JavaScript design prototype. It is not the application UI or a trace analyzer. All observations, source references, prompts, results and measurements are synthetic. No real trace files are included or read.

Open `index.html` directly, or serve this directory with a static HTTP server. There are no external assets, network requests, build steps or dependencies.

```sh
python3 -m http.server 4399 --bind 127.0.0.1 --directory design-concepts/trace-workbench
```

The screen explores the question "Where did this agent run go wrong, and did it recover?" The linked hierarchy, elapsed-time waterfall and inspector place a failed test beside its successful retry. A list offers a second view of types, offsets and outcomes. Light and dark themes expose the same data.

Controls include selection, search by name/type/ID, error/tool/slow filters retaining ancestor context, collapse/expand, keyboard navigation, input/output reveal, metadata, source diagnostics, selection-preserving filters, URL view state, theme toggle and copy actions. The footer previews empty, import-error and loading states. Loading resolves automatically; it does not import a file.

Session and turn elapsed intervals are labelled derived. Tool and generation durations are labelled recorded within the fixture. Cost and first-token time remain unknown. Generations and tools are siblings inside a turn. Usage totals are computed from generation views. Trace bodies use `textContent` and stay hidden until revealed.

## Design direction

- Hallmark Dose, dealt choice 1/3. Green-grey chrome and a green data signal, adapted to an app screen. Native system typography keeps this local prototype dependency-free.
- Taste Framed sheet, seed `2129534940`. A tinted frame holds a flat work surface. The hierarchy and timeline give the screen its identity.
- UI/UX Pro Max informed compact density, visible focus, contrast and reduced motion. Its generic landing-page recommendation was rejected because this is an investigation tool.

## References

- [Langfuse trace view](https://langfuse.com/changelog/2025-03-19-new-trace-view). Hierarchy, timeline parity, search and I/O inspection.

This concept independently implements presentation and interaction. It does not port upstream collection, rendering or correlation logic. Existing attribution remains in `THIRD_PARTY_NOTICES.md`.
