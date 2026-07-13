# Goals

Define the internal Beryl transcript presentation system above Syndic-owned durable history.

Keep transcript residency, presentation records, scroll state, renderer demand, resource admission, selection, copy, quote, context menus, diagnostics, and shell interaction separated from durable storage and backend execution.

## Non-goals

- Defining Syndic canonical history, turn DAG flattening, storage schema, resource byte layout, or provider execution policy.
- Defining user-visible transcript behavior that belongs in `doc/features/transcript/design.md`.
- Allowing GPUI render code to call Syndic storage or backend history APIs directly.
- Providing adapters from current transcript presentation code to obsolete transcript data structures.

# Decisions

## Documentation Set

- `renderer-architecture.md` is the normative supplemental architecture for transcript residency, presentation data, realized frame windows, renderer demand reporting, scrolling, geometry, selection, and nested-widget boundaries.
- `shell-boundary.md` is the normative supplemental shell-facing host boundary for activation, rendering, scrolling, diagnostics, selection, quote, menus, media actions, and provider demand.
- `doc/features/transcript/design.md` owns user-visible transcript behavior.
- `doc/systems/syndic-conversation-history/design.md` owns durable history, canonical items, transcript views, Markdown projections, resources, and replay.

## System Boundary

- Beryl keeps only the transcript presentation stack above Syndic: transcript host, transcript residency, presentation data, scroll controller, renderer, nested widgets, and diagnostics.
- Transcript residency is the only Beryl transcript layer that requests Syndic data.
- The renderer constructs GPUI elements only from resident presentation snapshots.
- Renderer code, nested widgets, and shell commands must not call `syndic-storage`, CAS history APIs, backend process memory, or raw provider protocols directly.
- Provider responses carry Syndic transcript-view cursor pages, immutable branch-context envelopes, projection records, resource metadata, resource ranges, revisions, and typed rejection or stale-result state.

## Presentation Records

- Presentation records adapt resident Syndic data into Beryl render records such as rows, chunks, fallbacks, live affordances, context-menu targets, copy spans, widget descriptors, and presentation revisions.
- Every transcript content presentation record carries Syndic provenance sufficient for selection, copy, quote, context menus, diagnostics, invalidation, branch/edit proof, and resource demand.
- A selected branch-discussion context envelope may contribute one synthetic context group at the immutable branch boundary immediately after its source turn and before the first branch-local submitted turn.
- That group is keyed independently of whether the immutable envelope currently belongs to the first draft or its transitioned submitted turn, so first submission does not remove, duplicate, or visibly relocate it.
- A synthetic context group is Beryl presentation derived from durable Syndic provenance. It is not canonical history, a transcript projection item, a Syndic turn, a DAG edge, or CAS input.
- Large context groups use bounded stable chunks inside the existing realized-frame and measurement system. Unresident chunks retain virtual extent and never create an independent scroll surface.
- A missing, stale, or invalid envelope produces one stable unavailable-context group at the descriptor's exact insertion boundary. The host never searches transcript text for a substitute.
- Beryl-local records such as synthetic context, carets, budget fallbacks, loading affordances, and transient UI state must declare that they are local presentation state rather than Syndic-authored transcript content.
- Missing data, pending data, stale data, rejected data, and loading state are not transcript content.

## Residency And Demand

- The panel, scroll controller, and nested widgets report demand facts to the transcript host rather than loading data themselves.
- Demand facts include visible presentation range, overscan range, missing leading or trailing range, current semantic anchor, measured geometry, manual scroll direction, explicit navigation target, live-tail intent, resource range demand, active selection pin, open menu pin, media preview pin, copy or quote pin, obsolete resident range, and stale measurement or revision observations.
- The host evaluates demand under residency policy and decides what to load, retain, pin, evict, release, reject, cancel, or retry.
- Rejections become stable fallback or clamp state rather than unbounded memory growth.

## Scroll And Activation

- Manual scrolling remains exact pixel displacement and never snaps to turns, rows, chunks, prompts, final answers, or transcript boundaries.
- Semantic placement is reserved for selected-view activation, live-turn autoscroll policy, explicit navigation, and saved-position restore.
- Activation publishes selected transcript content and initial viewport state atomically when a coherent seed is available.
- A previous coherent transcript may remain visible until a new coherent seed is ready.

## Resource Bounds

- Large code, table, generated-image, attachment, and comparable resources are represented by Syndic resource metadata and range-readable resource data.
- Beryl admits only resource ranges needed for the current viewport, nested-widget viewport, copy action, or active UI pin.
- Transcript presentation budgets resident projection data, presentation records, resource slices, decoded or uploaded media resources, measured geometry, widget state, and active UI pins.
- Synthetic context bytes remain bounded by the accepted branch-context limit, and their rendered chunk count remains bounded by the realized frame rather than total passage height.
- Visual fallbacks are required when content cannot be admitted or rendered within Beryl resource policy.
