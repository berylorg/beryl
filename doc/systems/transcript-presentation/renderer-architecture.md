# New Transcript Renderer

## Status

This is a supplemental target-state architecture note for Beryl's transcript presentation stack above Syndic.

It is not the authoritative durable conversation model. Syndic owns canonical history, transcript views over the turn DAG, Markdown and block projections, resource references, and range-readable heavy resources.

The authoritative contracts remain `doc/features/transcript/design.md`, `doc/systems/syndic-conversation-history/design.md`, and `doc/systems/transcript-presentation/design.md`. This note records the renderer-side shape those contracts imply.

## Goals

Render large Syndic-backed transcript views with bounded memory, bounded render work, exact manual scrolling, stable selection behavior, and no requirement for total transcript height.

Keep storage access, projection ownership, Markdown parsing, and resource byte loading out of GPUI render code.

## Non-Goals

- Defining adapters for legacy transcript systems. This note describes the clean target architecture for new code, not iterative adaptation of the old renderer.

## Terms

`Transcript view` means the ordered user-visible view over Syndic's turn DAG that the transcript renderer consumes.

`Resident Syndic data` means Syndic-derived projection records, immutable branch-context envelopes, metadata, resource slices, presentation records, and measurement state currently admitted into Beryl memory by transcript residency.

`Presentation data` means the ordered Beryl-side render records available to the scroll controller and renderer. Records may represent rows, streamed chunks inside a large row, synthetic context groups, stable fallbacks, local live affordances, or nested-widget descriptors.

`Realized frame window` means the subset of the presentation data currently laid out around the viewport, including bounded overscan.

`Transcript chunk` means a bounded render unit derived from resident Syndic projection data. Chunks are atomic to the outer transcript renderer, though they may contain nested widgets with their own bounded rendering.

`Transient live suffix` means the bounded exact text frontier received from the routed normalized live-turn stream but not yet covered by the resident Syndic projection. It is local presentation state attached to one authored record, not durable transcript content.

## Layer Boundary

Beryl keeps only the transcript presentation stack above Syndic.

Transcript residency is the only Beryl transcript layer that requests Syndic data. It admits
transcript-view cursor pages, projection records, resource metadata, and resource ranges into its
configured working set. It owns transcript-local item and byte budgets, priority, pins, eviction,
preload, cancellation, stale-result handling, and diagnostics.

Presentation data turns resident Syndic data into Beryl render records. It owns stable presentation identity, row and chunk ordering, synthetic-context insertion, fallback records, widget descriptors, copy spans, context-menu targets, and presentation revisions. It does not perform storage IO or raw Markdown parsing.

The scroll controller owns the semantic anchor, realized frame window, manual pixel scroll integration, autoscroll state, explicit navigation placement, anchor rebasing, and missing-content clamp behavior.

The renderer constructs GPUI elements only from the current resident presentation snapshot. It never calls Syndic or `syndic-storage` directly. It may report viewport facts, measured geometry, visible ranges, adjacent-range demand, resource-range demand, and obsolete resident ranges to transcript residency.

Nested code, table, and media widgets render from resident resource slices and report range demand through the same residency channel. They do not fetch storage directly.

## Pipeline

The target pipeline is:

```text
Syndic transcript view
-> transcript residency
-> resident Syndic data
normalized CAS live deltas
-> bounded transient live suffix
resident Syndic data + transient live suffix
-> presentation data + applicable synthetic context at an exact branch boundary
-> realized frame window
-> renderer
```

The renderer is a consumer of resident presentation data and a producer of demand facts. The residency controller is the owner of load, retain, pin, release, and reject decisions.

## Source Provenance

Every transcript content presentation record must carry Syndic provenance sufficient for selection, copy, quote, context menus, diagnostics, invalidation, and resource demand.

Required provenance includes the owning turn or transcript-view position when known, source item or block identity, resource identity when applicable, source range or resource range when applicable, projection revision, presentation revision, and copy-source span when applicable.

Beryl-local records such as synthetic discussion context, live carets, budget fallbacks, and transient affordances must declare that they are local UI state rather than Syndic-authored transcript content.

A transient live suffix carries exact routed thread, turn, item, kind, and logical-text-frontier identity. It may extend one resident authored record before durable projection catches up, but it cannot supply stable historical command provenance or survive as recovery authority.

A synthetic discussion-context group retains exact Syndic envelope provenance and one immutable insertion parent but does not claim authored-turn provenance or become a turn-number source.

## Transcript Projection Boundary

Syndic provides the parsed and indexed transcript projections. Beryl's transcript path does not rediscover Markdown block structure from raw assistant text and does not discover code blocks, tables, or media by reparsing raw Markdown in the render path.

Beryl may adapt resident Syndic projection records into presentation records, split very large projection records into render-budget chunks, and create stable local fallbacks. These adaptations must preserve Syndic source identity, ordering, copy semantics, and invalidation revisions.

Beryl may likewise adapt one immutable branch-context envelope into bounded synthetic-context chunks. The group remains at the branch boundary across first-draft submission and never enters the Syndic transcript projection.

Operational records that are not transcript narrative remain in Syndic canonical history and may feed activity, diagnostics, search, replay, or export projections. They are simply not admitted into transcript narrative presentation data unless a later product decision promotes a specific summary or item class.

## Residency And Demand

Renderer-driven residency is indirect.

The scroll controller, renderer, and nested widgets report demand facts such as current anchor, realized range, visible range, scroll direction, measured fill, missing adjacent range, needed resource range, active selection pin, open context-menu pin, and ranges no longer needed.

Transcript residency evaluates those facts under its practical page, byte, record, resource, and
pin limits, then performs the actual load or release work. It may reject a demand because of a
working-set limit, stale revision, cancellation, or feature policy. Rejections produce explicit
fallback or clamp state rather than unbounded memory growth.

Resident Syndic data can be released only when doing so preserves the current semantic scroll anchor, visible content, active selection contract, and active UI pins.

## Scrolling

Manual scrolling is exact pixel displacement derived from wheel, touchpad, keyboard, or smoothed input deltas.

Manual scrolling must not snap chunks, rows, turns, prompts, final answers, or transcript boundaries to viewport edges. Semantic placement is reserved for selected-thread activation, live-turn autoscroll policy, explicit navigation, and saved-position restore.

The scroll controller places one semantic anchor at its recorded viewport y-position, walks adjacent resident presentation records until the viewport plus bounded overscan is covered, and rebases only to another already realized record while preserving that record's current y-position.

When manual scrolling reaches the edge of resident coherent content, Beryl clamps at the resident edge, reports demand for the requested direction, and extends the frame only after coherent resident presentation records or stable terminal fallbacks are available.

Missing data is not transcript content. It is not selectable, copyable, quoteable, context-menu targetable, Markdown layout input, media layout input, or a visible transcript turn.

## Activation And Autoscroll

Existing transcript-view activation is tail-oriented unless another explicit navigation policy is requested. Activation publishes the selected transcript content and initial viewport state atomically.

Activation must not blank or replace the transcript with a full-region loading state when a previous coherent transcript can remain visible until the new coherent seed is ready.

Live-tail following and live-turn reading anchors are explicit viewport modes. Manual scroll intent detaches live automatic placement before residency work for that input is interpreted.

Autoscroll must be stateful and must not issue competing viewport corrections for every layout, stream, or resource event.

The renderer publishes all arrived bounded normalized-text fragments available to a frame as one
current snapshot while preserving their parent-delta identity and order. It neither delays them
behind a fixed-rate reveal cursor nor replays their characters or original arrival timestamps;
frame scheduling is the only permitted visual coalescing of already received fragments.

Durable reconciliation preserves the authored record identity and replaces only a transient prefix proven equal to the newly resident Syndic projection. It never briefly renders both copies or clears the record between them.

## Large Content

Very large transcript records and synthetic context groups render through bounded chunks or stable fallbacks. A large row or context group may stream chunk presentation around the current anchor without providing continuous pixel geometry for unloaded chunks.

Code blocks, tables, generated images, attachments, and comparable heavy resources are represented by Syndic resource metadata and explicit range-readable resource data. Beryl presentation records point at those resources and admit only the ranges needed for the current viewport, nested-widget viewport, copy action, or active UI pin.

Code and table panels own their internal visible-range rendering, selection, copy affordances, and local fallbacks. The outer transcript renderer treats the panel shell as one bounded presentation record with measured outer geometry.

Syndic's range-readable projections reduce the need for coarse full-turn fallbacks, but they do not
remove Beryl-side working-set policy. Transcript hosts bound resident projection data, presentation
records, resource slices, measured geometry, widget state, and active UI pins; shared media and GPU
caches bound decoded or uploaded resources across windows.

Visual fallbacks remain necessary when content cannot be admitted or rendered within Beryl's resource policy. Images are the clearest case because an oversized raster cannot be made safe by an inner lazy scroller. Pathological inline layout, unsupported resources, decode failures, and stale or rejected resource ranges also need stable local fallbacks.

## Geometry And Remeasurement

The renderer does not need to know chunk heights ahead of realization. It does need measured geometry for realized records.

Geometry can change because of resource readiness, widget mode changes, font or theme changes, viewport width changes, chrome height changes, fallback replacement, or projection revision changes.

When geometry changes, the scroll controller preserves the active semantic anchor or detached manual position using realized identity and measured geometry. It must not fall back to raw historical pixel offsets that can point into unloaded content or virtual empty space.

## Selection And Menus

Selection, Markdown-preserving copy, quote harvesting, and turn context menus operate only on rendered records whose provenance and geometry are stable.

Synthetic discussion-context chunks permit selection and copy when their geometry is stable but are never eligible for quote harvesting, branch creation, replacement edit, or turn context menus.

In streamed huge-content mode, transcript-level selection does not span through unrendered chunks. Nested widgets expose their own copy and selection contracts for resident resource ranges.

If virtualization, release, remeasurement, or missing data destroys stable selection geometry, the selection or quote affordance closes instead of pinning unbounded offscreen content.

## Invariants

- GPUI render code never calls Syndic or `syndic-storage` directly.
- Transcript residency is the only Beryl transcript layer that loads or releases Syndic data.
- Renderer and widgets report demand facts; they do not own load, release, pin, eviction, or byte-budget decisions.
- Manual scrolling is exact pixel movement and never semantic snapping.
- Missing data is never transcript content.
- Budget fallbacks are explicit Beryl UI records tied to Syndic provenance, not assistant-authored content.
- Synthetic discussion context is an explicit Beryl presentation group tied to Syndic envelope provenance, not a Syndic or CAS turn.
- Presentation records preserve Syndic provenance and revision identity.
- A transient live suffix is bounded non-authoritative presentation state and becomes durable only through exact Syndic-prefix reconciliation.
- Live text uses arrival-paced frame publication, never simulated character-by-character timing.
- Transcript rendering never requires total transcript pixel height.
- Render-path work does not parse raw Markdown, scan full history, compute residency totals, or build widgets for offscreen history.
- Transcript-view flattening over the Syndic turn DAG is not renderer-owned.
