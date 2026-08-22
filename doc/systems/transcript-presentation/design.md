# Goals

Define the internal Beryl transcript presentation system above Syndic-owned durable history.

Keep transcript residency, presentation records, scroll state, renderer demand, resource admission, selection, copy, quote, context menus, diagnostics, and shell interaction separated from durable storage and backend execution.

## Non-goals

- Defining Syndic canonical history, turn DAG flattening, storage schema, resource byte layout, or provider execution policy.
- Defining user-visible transcript behavior that belongs in `doc/features/transcript/design.md`.
- Allowing GPUI render code to call Syndic storage or backend history APIs directly.
- Compatibility adapters across transcript presentation boundaries are prohibited.

# Decisions

## Documentation Set

- [Renderer architecture](renderer-architecture.md) is the normative supplemental architecture for transcript residency, presentation data, realized frame windows, renderer demand reporting, scrolling, geometry, selection, and nested-widget boundaries.
- [Shell boundary](shell-boundary.md) is the normative supplemental shell-facing host boundary for activation, rendering, scrolling, diagnostics, selection, quote, menus, media actions, and provider demand.
- `doc/features/transcript/design.md` owns user-visible transcript behavior.
- `doc/systems/syndic-conversation-history/design.md` owns durable history, canonical items, transcript views, Markdown projections, resources, and replay.
- `doc/systems/cas-live-syndic-transcript/design.md` owns normalized live-event routing and durable CAS capture; this system consumes only its exact target-scoped presentation facts.
- `doc/systems/bounded-resource-dataflow/design.md` owns the risk-based page, working-set,
  backpressure, decoded-media, layout, and GPU constraints that this system consumes.

## System Boundary

- Beryl keeps only the transcript presentation stack above Syndic: transcript host, transcript residency, presentation data, scroll controller, renderer, nested widgets, and diagnostics.
- Transcript residency is the only Beryl transcript layer that requests Syndic data.
- The renderer constructs GPUI elements only from resident presentation snapshots.
- Renderer code, nested widgets, and shell commands must not call `syndic-storage`, CAS history APIs, backend process memory, or raw provider protocols directly.
- Provider responses carry Syndic transcript-view cursor pages, immutable branch-context envelopes, projection records, resource metadata, resource ranges, revisions, and typed rejection or stale-result state.

## Presentation Records

- Presentation records adapt resident Syndic data into Beryl render records such as rows, chunks, fallbacks, live affordances, context-menu targets, copy spans, widget descriptors, record-status provenance, and presentation revisions.
- Every transcript content presentation record carries Syndic provenance sufficient for selection, copy, quote, context menus, diagnostics, invalidation, branch/edit proof, resource demand, and any repair-pending, repaired, or incomplete status it presents.
- A selected branch-discussion context envelope may contribute one synthetic context group at the immutable branch boundary immediately after its source turn and before the first branch-local submitted turn.
- That group is keyed independently of whether the immutable envelope currently belongs to the first draft or its transitioned submitted turn, so first submission does not remove, duplicate, or visibly relocate it.
- A synthetic context group is Beryl presentation derived from durable Syndic provenance. It is not canonical history, a transcript projection item, a Syndic turn, a DAG edge, or CAS input.
- Large context groups use bounded stable chunks inside the existing realized-frame and measurement system. Unresident chunks retain virtual extent and never create an independent scroll surface.
- A missing, stale, or invalid envelope produces one stable unavailable-context group at the descriptor's exact insertion boundary. The host never searches transcript text for a substitute.
- Beryl-local records such as synthetic context, carets, budget fallbacks, loading affordances, and transient UI state must declare that they are local presentation state rather than Syndic-authored transcript content.
- Missing data, pending data, stale data, rejected data, and loading state are not transcript content.

## Live Tail

- The exact routed live-turn stream may contribute one bounded transient suffix to the active transcript-visible item while durable CAS capture independently coalesces that same ordered text into Syndic.
- The suffix is process-local presentation state keyed by exact thread, turn, item, kind, and logical-text frontier. It never becomes canonical history, a recovery source, a second item, or authority for history commands.
- The transcript host publishes each arrived bounded text fragment of one normalized delta on the
  next GUI frame that consumes it while retaining the delta's exact observation identity and order.
  It keeps no paced character-reveal queue and does not replay original timestamps; multiple
  fragments already pending at one frame boundary may naturally publish together. Fragmentation is
  transport only and never invents another provider delta or durable source event.
- Resident durable projection and transient suffix form one visible record. As Syndic projection catches up, the host removes only the exact matching prefix from transient ownership; a mismatch, gap, stale identity, or reversed frontier fails closed rather than guessing or duplicating text.
- The host retains no second whole-item live model. Its transient suffix and pending fragment
  channel stay bounded independently of total response size, while already reconciled text belongs
  only to ordinary resident Syndic projection data.

## Repair Publication

- Transcript presentation learns repair state and replacement content only from bounded Syndic
  provider responses naming the exact selected transcript-view generation, turn, projection
  revisions, source provenance including snapshot-backed authority when repaired, and `repair
  pending`, `repaired`, or `incomplete` disposition. The repair adapter, CAS history snapshot, live
  stream, outage buffer, GUI text, and partial repair records are never presentation inputs for
  canonical replacement.
- A repair-pending Syndic generation may remain the coherent visible source until Syndic publishes
  either the complete repaired projection or explicit incomplete convergence. Presentation may add
  the generation-owned pending provenance, but it does not construct speculative repaired records.
- Successful repair selects one complete snapshot-backed Syndic transcript generation. The
  replacement host generation atomically selects the affected turn's Syndic generation, compact
  head, source authority, and repaired provenance; it does not make the complete turn resident.
  Residency prepares only the bounded pages and indexed source or resource ranges needed for the
  coherent realized window, and later ranges load on demand under that same selection. No
  presentation snapshot may mix records, projections, statuses, or resources from two authority
  generations.
- Explicit incomplete convergence likewise advances through one Syndic-backed host generation. It
  publishes only the durable content and incomplete provenance selected by Syndic and admits no
  partial repair snapshot.
- The atomic host-generation switch disposes every transient live suffix and other turn-local
  provisional presentation evidence owned by the superseded generation. After anchor rebasing and
  the switch, it releases the old generation's projection pages, presentation records,
  measurements and layout, widget state, resource slices, pins, and capacity charges. Selection
  ranges, menu targets, and demand facts are rejected rather than retaining stale provenance.
- Record-status provenance is immutable within a host generation. It never advances independently
  in renderer or shell state.

## Residency And Demand

- The panel, scroll controller, and nested widgets report demand facts to the transcript host rather than loading data themselves.
- Demand facts include visible presentation range, overscan range, missing leading or trailing range, current semantic anchor, measured geometry, manual scroll direction, explicit navigation target, live-tail intent, resource range demand, the bounded focus pin, active selection pin, open menu pin, media preview pin, copy or quote pin, obsolete resident range, and stale measurement or revision observations.
- The host evaluates demand under residency policy and decides what to load, retain, pin, evict, release, reject, cancel, or retry.
- Rejections become stable fallback or clamp state rather than unbounded memory growth.

## Async Request Identity And Disposal

- Every asynchronous transcript page, projection, context, and resource-range request is keyed by
  the exact transcript-host identity, activation generation, selected authority generation
  including any repair-publication generation, requested logical or byte range, and one unique
  request id. A retry uses a new request id and cannot make an earlier result current.
- A result is accepted only while that complete key still names one outstanding current demand.
  Host replacement, activation change, authority-generation change, range supersession, request
  cancellation, or id mismatch makes it stale. Stale results are discarded and immediately release
  every returned page, range, reservation, pin, and response-buffer charge.
- Materialized projection pages, presentation records, measurements and layout, widget state,
  resource slices, and pins retain their host, authority-generation, and demand ownership. Last-
  demand removal, eviction, supersession, or host disposal releases those values and every
  associated item, byte, page, and pin charge; invalidation alone is never their terminal state.
- Demand removal cancels its outstanding work when no other current fact needs the same keyed
  range. Retirement of the exact transcript-provider service generation cancels and joins its
  remaining requests, closes their response sinks, and releases in-flight capacity. When the host
  survives recovery, that retirement may transfer its one bounded last coherent resident snapshot
  to the surviving host as inert presentation under the same existing charges; it transfers no
  request or loading authority and admits no new resident state until replacement publication.
- Transcript-host disposal or owning-window close cancels and joins every request and releases all
  resident and in-flight capacity. Replacement publication cancels old-generation demand and
  releases its materialized state after the atomic switch. No completion may recreate demand after
  its host, window, service generation, or authority generation is disposed.

## Focus Pin

- Each transcript host has exactly one focus-pin slot for its one transcript view. The slot is
  either absent or contains one demand fact naming the selected transcript-view identity, stable
  presentation-record identity, presentation revision, owning host/repair-publication generation
  identity, and, only when focus is inside a nested widget, that widget's stable
  identity and revision. Replacing the occupied slot is one transfer; focus never creates a pin per
  nested control or realized chunk.
- The host accepts or continues to honor the pin only while every named identity and revision
  matches the current coherent resident snapshot. A missing field, stale presentation or widget
  revision, mismatched transcript view, record replacement, or mismatched
  host/repair-publication generation rejects the pin and transfers focus through the transcript
  feature's fallback chain.
- An accepted pin retains only the focused presentation record and the minimum resource range and
  nested-widget state needed to keep that exact focus target valid. It never retains an entire turn,
  transcript page set, resource, nested model, or offscreen collection solely because focus exists.
- The host releases the pin on focus transfer outside the transcript, nested-widget blur or
  transfer, removal or replacement of the focused content without exact identity continuity,
  selected-thread switch, window close, or host disposal. A focus move to another transcript target
  replaces the old fact atomically rather than occupying a second slot.
- The one-slot capacity is the transcript view's contribution to the small configured focus-pin
  allowance owned by `doc/systems/bounded-resource-dataflow/design.md`; no window or view may expand
  that capacity in response to content size or focus depth.
- Focus-pin diagnostics are content-free and expose only pin presence and target state plus accept,
  reject, release, and transfer counts. They do not expose focus target identities, transcript
  content, target labels, resource bytes, or nested-widget content.

## Scroll And Activation

- Manual scrolling remains exact pixel displacement and never snaps to turns, rows, chunks, prompts, final answers, or transcript boundaries.
- Semantic placement is reserved for selected-view activation, live-turn autoscroll policy, explicit navigation, and saved-position restore.
- Activation publishes selected transcript content and initial viewport state atomically when a coherent seed is available.
- A previous coherent transcript may remain visible until a new coherent seed is ready.

## Resource Bounds

- Large code, table, generated-image, attachment, and comparable resources are represented by Syndic resource metadata and range-readable resource data.
- Beryl admits only resource ranges needed for the current viewport, nested-widget viewport, copy action, or active UI pin.
- Transcript presentation owns practical item and byte budgets for resident projection data,
  presentation records, resource slices, measured geometry, widget state, and active UI pins.
  Decoded and uploaded media additionally consume the shared coarse media and GPU cache budgets.
- Synthetic context bytes remain bounded by the accepted branch-context limit, and their rendered chunk count remains bounded by the realized frame rather than total passage height.
- Visual fallbacks are required when content cannot be admitted or rendered within Beryl resource policy.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers: none
