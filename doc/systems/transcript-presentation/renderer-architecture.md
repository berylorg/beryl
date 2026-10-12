# Transcript Renderer Architecture

## Status

This file is the normative supplemental renderer architecture governed by
`doc/systems/transcript-presentation/design.md`.

It defines only the renderer-side architecture within that system boundary. User-visible transcript
behavior remains owned by `doc/features/transcript/design.md`. Durable canonical history,
transcript views over the turn DAG, Markdown and block projections, resource references, and
range-readable heavy resources remain owned by `doc/systems/syndic-conversation-history/design.md`.

This supplement must satisfy those owning documents and does not redefine their contracts.

## Goals

Render large Syndic-backed transcript views with bounded memory, bounded render work, exact manual scrolling, stable selection behavior, and no requirement for total transcript height.

Keep storage access, projection ownership, Markdown parsing, and resource byte loading out of GPUI render code.

## Non-Goals

- Compatibility adapters to alternate transcript presentation systems are prohibited.

## Terms

`Transcript view` means the ordered user-visible view over Syndic's turn DAG that the transcript renderer consumes.

`Resident Syndic data` means Syndic-derived projection records, immutable branch-context envelopes, metadata, resource slices, presentation records, and measurement state currently admitted into Beryl memory by transcript residency.

`Presentation data` means the ordered Beryl-side render records available to the scroll controller and renderer. Records may represent rows, streamed chunks inside a large row, synthetic context groups, stable fallbacks, local live affordances, or nested-widget descriptors.

`Realized frame window` means the subset of the presentation data currently laid out around the viewport, including bounded overscan.

`Transcript chunk` means a bounded render unit derived from resident Syndic projection data. Chunks are atomic to the outer transcript renderer, though they may contain nested widgets with their own bounded rendering.

`Streaming history frontier` means a coherent revision-bound range of captured Syndic history and
its projection chunks published while capture continues. It uses the ordinary residency path;
CAS item completion does not define its rendering boundaries.

`Repair publication generation` means one immutable host generation prepared from a single selected
Syndic transcript-view generation after repair-pending, complete snapshot-backed repair, or explicit
incomplete publication. It is the atomic unit of renderer-visible repair-state change.

`Focus pin` means the transcript view's single bounded host demand fact for preserving one exact
focused presentation target across ordinary frame realization changes. It is not a second focus
model and does not authorize the renderer or a nested widget to retain data independently.

## Layer Boundary

Beryl keeps only the transcript presentation stack above Syndic.

Transcript residency is the only Beryl transcript layer that requests Syndic data. It admits
transcript-view cursor pages, projection records, resource metadata, and resource ranges into its
configured working set. It owns transcript-local item and byte budgets, priority, pins, eviction,
preload, cancellation, stale-result handling, and diagnostics.

Presentation data turns resident Syndic data into Beryl render records. It owns stable presentation identity, row and chunk ordering, synthetic-context insertion, fallback records, widget descriptors, copy spans, context-menu targets, and presentation revisions. It does not perform storage IO or raw Markdown parsing.

The scroll controller owns the semantic anchor, realized frame window, manual pixel scroll integration, autoscroll state, explicit navigation placement, anchor rebasing, and missing-content clamp behavior.

The renderer constructs GPUI elements only from the current resident presentation snapshot. It never calls Syndic or `syndic-storage` directly. It may report viewport facts, measured geometry, visible ranges, adjacent-range demand, resource-range demand, and obsolete resident ranges to transcript residency.

Resource-backed nested code, table, and media widgets render from resident resource slices and report range demand through the same residency channel. Inline Tables consume their admitted projection model. Neither form fetches storage directly.

Resource-backed Code/Table descriptors and projected cell fragments follow Syndic's
[indexed structural-query contract](../syndic-conversation-history/design.md#indexed-text-resource-structure-queries).
The selected-history provider transports the exact completed companion binding and continuation;
residency admits and charges those values together with their source ranges. Inner viewport changes
report line/fragment or cell-rectangle demand even when the outer realized window is unchanged.
Companion absence and local syntax fallback remain explicit resident states. The renderer does not
derive missing structure from resource bytes, and companion readiness does not replace outer
measured-candidate publication.

## Pipeline

The target pipeline is:

```text
CAS capture
-> bounded saved Syndic history chunks and coherent projection frontiers
-> Syndic transcript view
-> transcript residency
-> resident Syndic data
-> presentation data + applicable synthetic context at an exact branch boundary
-> realized frame window
-> renderer

Syndic repair generation + repair provenance
-> transcript residency
-> replacement host generation selecting the turn generation, head, and provenance
-> atomic renderer snapshot switch
```

The renderer is a consumer of resident presentation data and a producer of demand facts. The residency controller is the owner of load, retain, pin, release, and reject decisions.

## Source Provenance

Every transcript content presentation record must carry Syndic provenance sufficient for selection, copy, quote, context menus, diagnostics, invalidation, and resource demand.

Required provenance includes the owning turn or transcript-view position when known, source item or block identity, resource identity when applicable, source range or resource range when applicable, projection revision, presentation revision, and copy-source span when applicable.

Beryl-local records such as synthetic discussion context, live carets, budget fallbacks, and transient affordances must declare that they are local UI state rather than Syndic-authored transcript content.

Streaming records carry the exact Syndic source range, projection revision and capture provenance
of their saved chunks. The host does not append raw CAS text to a resident record. Completion and
history-command eligibility remain separate facts from chunk persistence and visibility.

Repair provenance carries the selected Syndic transcript-view generation, exact turn and projection
revisions, snapshot-backed source identity when repaired, and one pending, repaired, or incomplete
disposition. It is immutable within a host generation and cannot be inferred from visible text.

A synthetic discussion-context group retains exact Syndic envelope provenance and one immutable insertion parent but does not claim authored-turn provenance or become a turn-number source.

## Repair Publication

Transcript residency accepts repair replacement only as a complete bounded Syndic provider result
for one selected transcript-view generation. It does not consume a CAS repair response, raw live text,
outage buffer, GUI record, or partial repair page as canonical repair input.

The residency and presentation layers prepare one new host generation that selects the affected
turn's exact Syndic generation, compact head, snapshot-backed source authority, repair disposition,
and presentation provenance together. The selected complete turn remains paged and range-backed:
only the bounded records and resource descriptors needed for the coherent realized window become
resident, and later pages or ranges load on demand under the selected generation. The renderer
continues consuming the prior coherent generation until that replacement selector and its initial
resident window are ready, then consumes only the new immutable snapshot. No frame contains a
partial splice or records from both repair generations.

The switch invalidates old-generation geometry, selection, focus, menu anchors, nested-widget state,
and demand facts by generation identity. After the scroll controller rebases the semantic anchor,
the switch releases the superseded projection pages, presentation records, measurements and layout,
widget state, resource slices, pins, and every associated capacity charge. It does not transfer
record provenance or stale geometry.

Any turn-local presentation state attached to the superseded generation
is disposed in the same switch. It is never appended to or copied into the
snapshot-backed repaired generation. An incomplete publication follows the same generation boundary
and exposes no partial snapshot content.

## Transcript Projection Boundary

Syndic provides the parsed and indexed transcript projections. Beryl's transcript path does not rediscover Markdown block structure from raw assistant text and does not discover code blocks, tables, or media by reparsing raw Markdown in the render path.

Recognized inline Tables preserve their typed block and complete bounded source through the
[inline preparation boundary](../syndic-conversation-history/design.md#inline-table-structure-preparation).
Their prepared cell model belongs to projection residency and measured presentation, with shared
Table interaction and a direct source adapter. Inline source residency may cover offscreen selected
cells; grid paint retains the bounded visible rectangle. It introduces no resource demand, companion
root or independent residency cache.

Beryl may adapt resident Syndic projection records into presentation records, split very large projection records into render-budget chunks, and create stable local fallbacks. These adaptations must preserve Syndic source identity, ordering, copy semantics, and invalidation revisions.

Beryl may likewise adapt one immutable branch-context envelope into bounded synthetic-context chunks. The group remains at the branch boundary across first-draft submission and never enters the Syndic transcript projection.

Operational records that are not transcript narrative remain in Syndic canonical history and may feed activity, diagnostics, search, replay, or export projections. They are simply not admitted into transcript narrative presentation data unless a later product decision promotes a specific summary or item class.

## Residency And Demand

Renderer-driven residency is indirect.

The scroll controller, renderer, and nested widgets report demand facts such as current anchor,
realized range, visible range, scroll direction, measured fill, missing adjacent range, needed
resource range, the bounded focus pin, active selection pin, open context-menu pin, and ranges no
longer needed.

Transcript residency evaluates those facts under its practical page, byte, record, resource, and
pin limits, then performs the actual load or release work. It may reject a demand because of a
working-set limit, stale revision, cancellation, or feature policy. Rejections produce explicit
fallback or clamp state rather than unbounded memory growth.

Every asynchronous load carries the exact host identity, activation generation, selected authority
generation including any repair-publication generation, requested logical or byte range, and one
unique request id.
Residency accepts a result only while that full key remains outstanding and current. Superseded,
cancelled, mismatched, or post-disposal results are discarded and release all returned pages,
ranges, reservations, pins, and response buffers immediately. Materialized pages, presentation
records, measurements and layout, widget state, resource slices, and pins retain their exact demand
and generation ownership; last-demand removal, eviction, supersession, or host disposal releases
them and their complete capacity charges.

Retirement of the exact transcript-provider service generation cancels and joins its outstanding
demand and releases in-flight capacity. A surviving host may retain only its one bounded last
coherent resident snapshot as inert presentation under the same transferred charges; it gains no
request authority and admits no replacement data until coherent publication. Host disposal or
owning-window close releases that snapshot and every other resident value. No late completion may
recreate disposed demand.

Resident Syndic data can be released only when doing so preserves the current semantic scroll anchor, visible content, active selection contract, and active UI pins.

### Focus-Pin Validation

Each transcript view has one host-owned focus-pin slot, absent or occupied. An occupied fact carries
the selected transcript-view identity, stable presentation-record identity, presentation revision,
owning host/repair-publication generation identity, and, if focus is inside a nested widget, the
stable nested-widget identity and nested-widget revision. A direct record target omits
the nested fields. Moving focus between targets replaces the fact atomically and cannot increase
the slot capacity above one per transcript view and owning window.

Residency validates every carried identity and revision against the current coherent snapshot both
when accepting the fact and before retaining or revealing its target. Invalid or stale facts are
rejected, release any retention they established, and transfer focus through the fallback chain
owned by `doc/features/transcript/gui.md`. A whole-turn repair switch therefore cannot preserve
focus merely because replacement content looks equivalent: host/repair-publication generation, record
identity, presentation revision, and any nested-widget revision must still match exactly.

The accepted pin retains only the named presentation record, the smallest resource range needed by
the focused target, and the minimum retained state of the named nested widget. It cannot retain a
whole turn, page set, complete resource, nested collection, or unrelated geometry. Its capacity
counts against the shared small focus-pin allowance defined by
`doc/systems/bounded-resource-dataflow/design.md`.

Focus transfer outside the transcript, nested-widget blur or transfer, focused-content removal or
replacement without exact identity continuity, selected-thread switch, owning-window close, and
host disposal release the fact. A transfer to another target in the same transcript releases the
old nested target and installs the destination fact as one atomic slot transfer.

Focus-pin diagnostics expose only absent, direct-record, or nested-widget state and cumulative
accept, reject, release, and transfer counts. They contain no transcript content, target labels,
focus target identities, record or widget payloads, resource bytes, or focused subcontrol content.

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

The renderer publishes newly admitted bounded Syndic projection chunks available to a frame as
one current snapshot while preserving source order and provenance. It neither delays them behind
a fixed-rate reveal cursor nor replays their characters or original arrival timestamps; frame
scheduling is the only permitted visual coalescing of already admitted chunks.

Successive coherent history frontiers preserve authored record identity and reading anchors.
They never briefly render duplicate ranges or clear the record between them.

For authored projection chunks with saved item and source-range provenance, presentation identity
uses the selected view, saved item and source-block start. Fenced Code carries Syndic's saved
Markdown block identity, derived from the owning saved item and opening-fence start; its presentation
identity uses that block identity and the selected view. The code-body start is an exact read boundary,
not the block identity: it can move while an unfinished opening info string gains bytes or its line
ending. It does not use the replaceable immutable
projection/resource snapshot or transcript generation. Exact snapshot identities and revisions
remain in provenance and continue to fence reads, actions and measurement facts. A Code panel may
retain its mount across a successor projection for that same saved block; its indexed binding must
still prove source-prefix compatibility before restoring the reading anchor. Unrelated source
replacement retains the existing reset behavior.

## Large Content

Very large transcript records and synthetic context groups render through bounded chunks or stable fallbacks. A large row or context group may stream chunk presentation around the current anchor without providing continuous pixel geometry for unloaded chunks.

Every recognized fenced Code block uses Syndic resource metadata and explicit range-readable
resource data, including small, empty and unfinished blocks, as specified by the
[projection storage contract](../syndic-conversation-history/concepts.md#markdown-projection).
Inline code spans remain part of their surrounding narrative block. Large Tables, generated
images, attachments and comparable heavy resources also use resource data; small inline Tables
use their admitted projection model. Resource-backed presentation admits only the ranges needed
for the current viewport, nested-widget viewport, admitted Copy/Save source action or active UI pin.

Code and table panels own their internal visible-range rendering, selection, copy affordances, and local fallbacks. The outer transcript renderer treats the panel shell as one bounded presentation record with measured outer geometry.

Syndic's range-readable projections reduce the need for coarse full-turn fallbacks, but they do not
remove Beryl-side working-set policy. Transcript hosts bound resident projection data, presentation
records, resource slices, measured geometry, widget state, and active UI pins; shared media and GPU
caches bound decoded or uploaded resources across windows.

Visual fallbacks remain necessary when content cannot be admitted or rendered within Beryl's resource policy. Images are the clearest case because an oversized raster cannot be made safe by an inner lazy scroller. Pathological inline layout, unsupported resources, decode failures, and stale or rejected resource ranges also need stable local fallbacks.

## Indexed No-Wrap Code Layout

Beryl presentation owns a font-environment-specific layout index for range-backed Code. Syndic's
structural companion remains independent of fonts and supplies exact logical lines, source ranges
and bounded Display windows. Layout input is exclusively admitted saved Beryl/Syndic content. CAS
items, raw capture text and painted strings are never layout or source-action authority.

The layout binding names the exact transcript host/activation, selected view, saved block,
immutable resource/root/source range, projection revision and measurement environment. The
environment includes resolved fonts and fallback generation, font size and shaping features,
line metrics, tab policy and the canonical segment policy. Paint-only color changes preserve
geometry. A change to any geometry input creates a new layout epoch; results from another epoch
cannot be published or used to clamp the current viewport.

### Construction And Ownership

The existing transcript host and residency own layout demand, source admission, cancellation,
capacity and publication. The selected-history provider continues to read the source through its
ordinary indexed/raw resource queries. There is no widget loader, independent source cache, new
CAS path or durable font-dependent Syndic companion. The index is disposable presentation state.

An admitted layout job advances in bounded work quanta outside render and prepaint. Source reads
stay within the existing 65,536-byte response ceiling; one canonical shaping segment contains at
most 8,192 UTF-8 bytes. Each job has configured segment-work, checkpoint-count, retained-byte and
semantic-item ceilings independent of source length and viewport size. Those limits include peak
coexistence with the previous coherent publication and share the host's presentation/resource
capacity. Input pages, decoder/grapheme carry, continuations, checkpoints and output fragments
are all bounded and charged. Capacity refusal leaves the prior coherent surface intact and exposes
local unavailability; it does not grow the limits or keep an unbounded pending queue.

Canonical segments are determined by source grapheme boundaries and the configured segment cap,
independently of storage pages, Display response sizes or arrival timing. Beryl uses the owned
GPUI canonical bounded streaming-layout boundary: each segment is an independent shaping context
and carries compact inline placement to its successor. Construction and painting use the same
segments, font inputs and tab policy; isolated arbitrary transport fragments cannot establish
cumulative geometry. The default Code tab policy uses stops at eight space advances of the resolved
primary body font, measured from the logical-line origin. A tab retains its exact one-byte source
span and receives the bounded advance to the next stop; continuation across source/segment edges
does not restart tab alignment. Its source-covering geometry is consumer-supplied to the canonical
layout boundary. A nonpositive or unrepresentable stop follows local layout fallback.
A grapheme exceeding the segment cap is scanned through bounded pages to
its exact end and represented by a bounded source-covering oversize-layout atom. That explicit
local fallback preserves source selection/Copy/Save and does not allocate the complete grapheme.
Malformed source, unsupported layout or unrepresentable geometry similarly produces a typed local
fallback rather than lossy authored text or a guessed pixel-to-byte conversion.

A sparse checkpoint contains the exact logical line, source byte position, canonical segment
position, cumulative inline advance and compact resumable shaping/placement state. It retains no
source text or glyph payload. The index keeps a configured fixed-capacity monotonic checkpoint set
and terminal aggregates, using deterministic compaction when needed. It does not retain an entry
or shaped result for every source line or fragment. Compaction preserves checkpoint facts; replay
between surviving checkpoints may take additional bounded work quanta. This bounds residency,
not total indexing time or worst-case target-resolution latency. A cache miss rebuilds derived
layout through the same owner and bounded jobs; no raw byte ratio substitutes for layout.

### Horizontal Queries And Publication

A horizontal target names a logical line and exact inline pixel displacement. Resolution chooses
a valid checkpoint at or before the target, resumes bounded canonical layout and returns only the
source-covering segments intersecting the viewport plus configured bounded overscan. Each result
includes exact source spans, inline placement, hit mapping and line identity. A later segment is
placed using its admitted cumulative advance, not at the line origin or as an additional row.
Source-byte Display demand follows the resolved source window and retains its exact immutable
binding and UTF-8 boundary rules. Target resolution never shapes an entire pathological line into
one allocation, and renderer work never scans its unseen prefix.

Indexing starts at an exact source origin or an index-produced checkpoint and publishes progress
without claiming completion. Terminal line widths and the resource-wide maximum width become
exact only after their corresponding scan completes. An incomplete prefix is not the complete
horizontal extent. Vertical logical-line extent continues to come from the structural companion.
Horizontal movement may record a desired target while layout is pending; the previous coherent
window remains painted with an explicit bounded loading/fallback state until the matching target
is ready. It must not turn an unresolved target into a blank viewport or silently discard the
desired offset by clamping to a prefix width. Exact complete extent alone authorizes terminal
horizontal clamping. Source/font replacement discards incompatible geometry; publication is
atomic for the exact current binding, epoch and target request.

The viewport retains a source anchor, intra-line source offset and visual displacement. A
same-source revision first resolves that anchor through the admitted successor binding, then
builds geometry for the new epoch; it cannot reuse old cumulative widths merely because bytes are
prefix-compatible. Panel identity, focus, wheel ownership and valid source selection survive this
reconciliation. Host replacement, obsolete demand, cancellation or unmount releases source pages,
layout state, shaped fragments, checkpoints and every associated charge. Late results cannot
recreate retired demand. Copy and Save always use exact admitted source ranges, independently of
checkpoint coverage, oversize atoms or realized glyphs.

Acceptance exercises real no-wrap horizontal traversal of a line larger than one Display fragment,
canonical segmentation independent of response boundaries, Unicode/font fallback and tabs,
oversize graphemes, partial readiness, source/font invalidation, exact terminal extent and clamp,
source-anchor preservation, stale-result refusal and disposal. Growth cases hold all work/residency
limits fixed. Content-free diagnostics expose layout epoch continuity, progress/completion kind,
checkpoint/fragment counts, charged capacity and pending/ready/refused target state, never text,
raw identities, source positions or copied content.

## Geometry And Remeasurement

The renderer does not need to know chunk heights ahead of realization. It does need measured geometry for realized records.

Geometry can change because of resource readiness, widget mode changes, font or theme changes, viewport width changes, chrome height changes, fallback replacement, or projection revision changes.

When geometry changes, the scroll controller preserves the active semantic anchor or detached manual position using realized identity and measured geometry. It must not fall back to raw historical pixel offsets that can point into unloaded content or virtual empty space.

## Measured Frame Preparation

The transcript host owns one private measured-frame preparation and one published coherent frame.
Preparation is keyed by exact host/view and activation identity, presentation and record revisions,
and measurement environment, including viewport allocation, font/rem metrics and theme appearance.
Only natural-layout observations for that current key may advance preparation. A changed key
cancels the obsolete preparation and releases its observations, elements and candidate ownership.

Each GUI frame performs at most 32 new record-layout observations for preparation. Refill and
shrink traversal obey the same limit as ordinary input. Traversal retains its current stable record,
viewport position and remaining exact displacement as bounded scalar state; crossed record elements
and measurements are released rather than accumulated. Candidate geometry covers only the eventual
viewport and bounded overscan plus the current traversal record. Ordinary resident admission bounds
still apply to shared candidate data; preparation does not read history or extend residency.

The renderer keeps the previously published coherent frame while preparation continues. It closes
transient geometry invalidated by the environment change and does not construct selection or command
targets from candidate content. With no prior frame, content remains absent until its complete measured
initial placement is ready. A frame containing guessed record heights cannot be published as measured.

Preparation measures adjacent resident records before integrating displacement or rebasing across
them. Reaching either coherent edge clamps even during zero-input remeasurement. If a clamp changes
the window, preparation qualifies all newly required rows before publication. It needs no total
resident or historical pixel extent. Short content starts at the leading edge; otherwise tail
placement puts the measured last record's bottom at the viewport bottom.

Wheel, touchpad and keyboard intent detaches tail following immediately. Exact pending displacement
is retained across bounded preparation steps and coherent same-view refresh, and retired on selected
authority replacement. New input can follow a current preparation without discarding already accepted
intent. Finishing preparation publishes its snapshot, actual geometry, semantic anchor, clamp and
demand facts once; provisional rows emit no committed measurements or residency decisions.

Selected attachment publication obeys the shell boundary's measured-publication gate. Candidate
content cannot become visible before its matching selected metadata and initial measured placement.
Host retirement, window close and authority replacement release both preparation and published-frame
ownership. Content-free diagnostics expose pending preparation, observation work and retained geometry
counts without content or resource bytes.

## Selection And Menus

Selection, Markdown-preserving copy, quote harvesting, and turn context menus operate only on rendered records whose provenance and geometry are stable.

Synthetic discussion-context chunks permit selection and copy when their geometry is stable but are never eligible for quote harvesting, branch creation, replacement edit, or turn context menus.

In streamed huge-content mode, transcript-level selection does not span through unrendered chunks. Nested widgets expose their own copy and selection contracts for resident resource ranges.

The transcript Copy owner captures one exact eligible selection and its source/revision binding,
then requests the feature-selected Markdown or stripped-text representation through admitted
source data. Both methods preserve that provenance; stripped text is a derived representation,
never text harvested from paint. Representation work observes the existing bounded worker and
residency ownership: widgets do not reparse Markdown, fetch storage or retain another complete
transcript model. Each clipboard publication checks that its captured selection and source remain
current. A later method request supersedes an earlier unfinished representation so late Markdown
completion cannot replace the requested stripped-text result. Failed or rejected representation
requests preserve the selection and do not publish a partial clipboard value.

For ordinary whole-record selection, the existing attachment worker derives the stripped source
representation before provider admission. Original and derived bytes both count toward residency;
the presentation owner transports them with the same source/revision binding. Contiguous Copy
reconstruction preflights each representation against its admitted bound. A continuation span
whose preceding delimiter context is unavailable keeps its source spelling, protecting literals
opened in unloaded source. GUI command routing consumes admitted representations without parsing.

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
- Streaming authored content is read from saved Syndic history chunks through ordinary residency;
  no parallel CAS-text suffix supplies renderer content.
- Repair publication consumes only a complete Syndic-selected generation with generation-owned
  pending, repaired, or incomplete provenance; CAS snapshots, live or outage state, GUI text, and
  partial repair data never enter renderer presentation directly.
- A repair publication atomically switches the affected turn's generation, compact head, source
  authority, and provenance while keeping its content paged and range-backed; it disposes the
  superseded presentation state and never renders a mixed-generation or blank intermediate turn.
- Every asynchronous residency result matches its complete host, activation, authority-generation,
  range, and request-id key or is discarded with all returned capacity released; host, window, and
  provider-service disposal cancel all outstanding demand.
- Last-demand removal, eviction, authority supersession, host disposal, and window close release
  materialized pages, records, layout, widget state, slices, pins, and their charges. Provider-
  service retirement may transfer only one bounded inert last coherent snapshot to a surviving host
  and transfers no loading authority.
- Record-status provenance is immutable within a host generation and cannot update independently in
  the renderer.
- Live text uses arrival-paced frame publication, never simulated character-by-character timing.
- Transcript rendering never requires total transcript pixel height.
- Render-path work does not parse raw Markdown, scan full history, compute residency totals, or build widgets for offscreen history.
- Transcript-view flattening over the Syndic turn DAG is not renderer-owned.
