# Goals

Render Syndic transcript views as a responsive parent conversation narrative with bounded memory, bounded render work, stable provenance, Markdown-preserving selection and quote behavior, transcript media, context menus, and exact manual scrolling.

Keep durable conversation history, Markdown/block projection, resource references, and heavy resource byte access owned below the transcript presentation stack.

## Non-goals

- Defining Syndic's canonical turn DAG, transcript-view flattening policy, storage schema, resource reference grammar, or resource byte layout.
- Parsing raw Markdown into durable semantic blocks in the GPUI transcript renderer.
- Rendering operational activity, tool logs, raw reasoning, diagnostics, or hidden chain-of-thought as transcript narrative.
- Providing adapters from new transcript code to obsolete transcript data structures.

# Decisions

## Documentation Set

- `renderer-architecture.md` is the supplemental transcript renderer architecture for the Syndic-backed target state.
- The renderer architecture supplement is normative for transcript residency, presentation data, realized frame windows, renderer demand reporting, scrolling, geometry, selection, and nested-widget boundaries.
- `shell-boundary.md` is the supplemental shell-facing transcript host boundary for the Syndic-backed target state.
- The shell-boundary supplement is normative for the state, inputs, outputs, demand facts, diagnostics, and invariants exposed between Beryl shell code and the transcript presentation stack.
- This feature entry point owns the user-visible transcript behavior and cross-layer Beryl presentation contract. Syndic feature docs own the durable conversation model and projection source semantics.

## Transcript Model

- The transcript is the stable parent conversation narrative for the selected Syndic transcript view.
- Transcript-view flattening over Syndic's turn DAG is not renderer-owned. The transcript feature consumes an already selected ordered view.
- Transcript narrative includes user-authored input fragments, transcript-visible user media markers, assistant commentary, assistant final answers, assistant text that Syndic marks transcript-visible, and generated media intended as assistant output.
- Transcript narrative excludes operational records that are not parent narrative, including command execution details, tool calls, protocol records, file-change records, raw reasoning, token accounting, lifecycle/status events, and subagent internals unless a later product decision promotes a specific summary or item class.
- Excluded operational records remain in Syndic canonical history and may feed activity, diagnostics, search, replay, or export projections.
- Missing resident data, loading state, pending range fetches, and budget rejection state are not transcript content and must not become selectable, copyable, quoteable, context-menu targetable, Markdown layout input, media layout input, or synthetic transcript turns.

## Layer Boundary

- Beryl keeps only the transcript presentation stack above Syndic: transcript residency, presentation data, scroll controller, renderer, and nested widgets.
- Transcript residency is the only Beryl transcript layer that requests Syndic data. It admits transcript-view cursor pages, projection records, resource metadata, and resource ranges into bounded resident memory.
- Presentation data adapts resident Syndic data into Beryl render records such as rows, chunks, fallbacks, live affordances, context-menu targets, copy spans, widget descriptors, and presentation revisions.
- The renderer constructs GPUI elements only from current resident presentation snapshots. It never calls Syndic or `syndic-storage` directly.
- The renderer, scroll controller, and nested widgets may report demand facts to transcript residency, including visible range, adjacent range demand, resource range demand, measured geometry, active pins, and obsolete resident ranges.
- Transcript residency owns load, retain, pin, evict, release, cancellation, stale-result handling, rejection, and diagnostic decisions under policy.

## Provenance And Identity

- Every transcript content presentation record carries Syndic provenance sufficient for selection, copy, quote, context menus, diagnostics, invalidation, and resource demand.
- Required provenance includes owning turn or transcript-view position when known, source item or block identity, resource identity when applicable, source or resource range when applicable, projection revision, presentation revision, and copy-source span when applicable.
- Beryl-local records such as carets, budget fallbacks, and transient affordances declare that they are local UI state rather than Syndic-authored transcript content.
- Budget fallbacks are explicit Beryl UI records tied to Syndic provenance and must not masquerade as assistant-authored content.

## Rendering And Residency Bounds

- Transcript rendering never requires total transcript pixel height.
- Render-path work does not parse raw Markdown, scan full history, compute residency totals, or build widgets for offscreen history.
- Very large transcript records render through bounded chunks, nested widgets, or stable local fallbacks.
- Syndic's range-readable projections reduce the need for coarse full-turn fallbacks, but Beryl still budgets resident projection data, presentation records, resource slices, decoded or uploaded media resources, measured geometry, widget state, and active UI pins.
- Visual fallbacks remain necessary when content cannot be admitted or rendered within Beryl's resource policy, including oversized images, unsupported resources, decode failures, pathological inline layout, and stale or rejected ranges.
- Resident Syndic data may be released only when doing so preserves the current semantic scroll anchor, visible content, active selection contract, and active UI pins.

## Scrolling And Activation

- Manual scrolling is exact pixel displacement derived from wheel, touchpad, keyboard, or smoothed input deltas.
- Manual scrolling must not snap chunks, rows, turns, prompts, final answers, or transcript boundaries to viewport edges.
- Semantic placement is reserved for selected-view activation, live-turn autoscroll policy, explicit navigation, and saved-position restore.
- The scroll controller owns the semantic anchor, realized frame window, manual pixel integration, anchor rebasing, autoscroll state, explicit navigation placement, and missing-content clamp behavior.
- When manual scrolling reaches the edge of resident coherent content, Beryl clamps at the resident edge, reports demand for the requested direction, and extends the frame only after coherent resident presentation records or stable terminal fallbacks are available.
- Existing transcript-view activation is tail-oriented unless another explicit navigation policy is requested. Activation publishes selected transcript content and initial viewport state atomically.
- Activation must not blank or replace the transcript with a full-region loading state when a previous coherent transcript can remain visible until the new coherent seed is ready.

## Large Resources And Nested Widgets

- Code blocks, tables, generated images, attachments, and comparable heavy resources are represented by Syndic resource metadata and explicit range-readable resource data.
- Beryl presentation records point at those resources and admit only ranges needed for the current viewport, nested-widget viewport, copy action, or active UI pin.
- Code and table panels own internal visible-range rendering, selection, copy affordances, and local fallbacks. The outer transcript renderer treats the panel shell as one bounded presentation record with measured outer geometry.
- Media rendering is bounded by resource admission, decode/upload budgets, path and permission policy, and terminal fallback states. Oversized raster media cannot rely on an inner lazy scroller to become safe.

## Selection, Copying, Quote, And Menus

- Selection, Markdown-preserving copy, quote harvesting, and turn context menus operate only on rendered records whose provenance and geometry are stable.
- In streamed huge-content mode, transcript-level selection does not span through unrendered chunks.
- Nested widgets expose their own copy and selection contracts for resident resource ranges.
- If virtualization, release, remeasurement, or missing data destroys stable selection geometry, the selection or quote affordance closes instead of pinning unbounded offscreen content.
- Context menus target rendered transcript content with stable Syndic provenance. Menus do not open for empty space, missing data, operational activity, or transient non-content paint state.
