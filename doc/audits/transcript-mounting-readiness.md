# Transcript Mounting Readiness

## Boundary And Authority

Inspected production source at `244ae86` on 2026-10-10 for root-plan phase 752.
This is a readiness record, not acceptance of mounted transcript behavior. The controlling
[transcript feature](../features/transcript/design.md), [GUI composition](../features/transcript/gui.md),
[transcript view](../gui/widgets/transcript-view/spec.md), and presentation system's
[renderer architecture](../systems/transcript-presentation/renderer-architecture.md) and
[shell boundary](../systems/transcript-presentation/shell-boundary.md) already determine ownership,
variable-height realization, exact scrolling, measurement invalidation, demand, and disposal.
No new slot, widget contract, history route, or product decision is needed for the next boundary.
The App and bounded-resource contracts also apply. This review changes no production behavior
or external-effect authority; source inspection and author semantic review suffice for this record.
The next viewport behavior change requires independent semantic review and native evidence.

## Current Production Consumers

- `crates/beryl-app/src/main_window/shell/host/root.rs` passes the selected panel into the shell
  composition. `host/running_threads.rs` owns the panel entity, provider reader, cancellation and
  attachment custody. Initial attachment, runtime setup and same-home recovery publish into this
  same target panel boundary or replace its entity through their accepted selection fences.
- `host/running_threads/activation/attachment/presentation.rs` publishes selected attachment;
  `host/running_threads/transcript.rs::sync_running_transcript` prepares bounded refresh work off
  the GUI thread and checks request, reader publication, selected claim, panel and placement
  before publication. It publishes title and lineage with the coherent transcript attachment.
- `crates/beryl-app/src/transcript_provider.rs::prepare_attachment` reserves one pending read and
  checks home/publication identity. `transcript_provider/source.rs::read_bounded_activation`
  reads the selected current Syndic view with bounded cursor limits. This is an attachment/refresh
  path, not an implemented consumer of panel resource or adjacent-page demand.
- `crates/beryl-app/src/shell/syndic_transcript/host.rs` owns the resident core, frame controller
  and last frame. `panel.rs::render_measured` reports viewport and nested resource demand,
  realizes a bounded window and reconciles existing selection/menu/media facts against it.
  `render_frame_window` renders only that window and checks stable record identity.
- Resident presentation records and resource descriptors share immutable batches since `244ae86`.
  Text and range bytes are shared. This qualifies publication ownership and final-reference
  release, not all core diagnostic copies, layout costs, widgets or provider delivery.

## Remaining Contract Gaps

**Measured viewport and input.** `panel.rs::render_realized_record` absolutely positions and clips
every row to the controller's supplied height. The default is 72 pixels, and production does not
call `RealizedFrameScrollController::observe_record_measurement`. The existing canvas measures
the viewport allocation, not each record's natural content height. Long text can be clipped while
short text reserves an arbitrary extent. `frame/geometry.rs::record_height` can use observations,
but `prepare_for_snapshot` retains cached heights by record identity alone; width, style and
content revision must invalidate them before this becomes a production measurement cache.
`manual_scroll_delta` is callable, but no production wheel handler invokes it. The controller's
pixel/clamp tests therefore do not establish ordinary pointer or keyboard scrolling.

**Demand delivery and release.** `core.rs::push_demand_fact` records missing/adjacent/resource
facts but does not execute their loads. Production has no caller of `drain_demand_facts` and no
panel demand driver. Core request builders, response admission and obsolete release exist as
tested private primitives. Their request/response shapes do not by themselves establish the
complete asynchronous host, activation, authority-generation, range and unique-request identity
or provider retirement/cancellation/join guarantee. That must be qualified at the real driver.
Scrolling remains correctly clamped at a coherent resident edge until this separate boundary
extends it; attachment refresh must not become an alternate history-loading loop.

**Narrative and interactions.** `render_record_content` paints a plain text chunk rather than
projected Markdown structure. Resource rendering exposes bounded text/binary preview shells,
not mounted canonical code/table panels, image markers and previews. Pointer text selection,
range-based copy/quote actions and menus require real rendered text geometry; whole-record
fixture selections and a right-click target helper do not establish those interactions.
The single exact focus-pin fact and nested wheel owner are absent from current demand shapes.
Widget lifecycle, minimum range retention and fallback focus must accompany their mounted controls.

**Live and generation-owned presentation.** Current snapshot record shapes do not provide the
bounded routed transient suffix, exact durable-prefix reconciliation, synthetic context group,
or generation-owned per-record repair status required by the target. Selected attachment and
whole-refresh publication are useful seams, not proof of live continuity or repaired-generation
publication. Phase 465 stays blocked on its exact source evidence; no transcript work authorizes
CAS history fallback or labels incomplete content as repaired.

**Evidence.** Existing content-free resident/frame counters and render timing tools are useful
seams. Full widget diagnostics, focus/nested-widget state, native variable-height scrolling and
diagnostic-child coherent activation/media readiness remain unqualified. Do not interpret
element-tree or renderer counters as proof of visible paint.

## Next Observable Boundary

Mount a measured resident transcript viewport through the existing production panel and host.
Within the current admitted resident snapshot, records occupy their actual bounded content
height; wheel, touchpad and keyboard manual input move that viewport by exact pixels, detach tail
following before demand, and clamp at coherent edges. Stable realized identity preserves the
manual anchor across content, width, font, theme and resource-shell remeasurement. Initial
selected attachment still presents its content and tail placement together without a later
measurement correction. Geometry invalidation closes affected transient targets.

This is one viewport behavior boundary: measurement publication, cache invalidation, bounded
frame refill, native input and content-free diagnostics are its supporting work. It does not
accept resource delivery, Markdown, nested widgets, focus-pin mounting, live suffixes or the
whole Checkpoint 4 transcript item. Their owning authority is present; they remain separate
observable outcomes after the viewport has a qualified geometry foundation.

## Acceptance Seams And Required Evidence

- Extend `frame/mod.rs`, `frame/geometry.rs`, the host and the panel's canvas layout/prepaint
  boundary. Reject old host/view/record/revision and measurement-environment observations;
  bound measured state and element construction to realized content plus permitted overscan.
  Preserve shared snapshots and dispose obsolete measurements on replacement and host retirement.
- Extend `tests/syndic_transcript_realized_frame.rs` for variable heights, fractional pixel input,
  same-id content revision, width/style invalidation, tail placement, detached anchors, coherent
  edge clamps and stale observation rejection. Its current measurement rejection case supplies
  only a stale revision; it does not qualify actual GPUI height feedback.
- Extend `tests/unit/syndic_transcript_panel.rs::nested_body_paints_newest_tail_on_first_frame_and_preserves_manual_anchor`
  to inspect variable-height row bounds and dispatch real input. This existing GPUI test uses
  fixture content and calls manual scrolling directly; it proves neither native event routing
  nor variable-height measurement. Reuse the focused immutable/resident release and renderer
  provenance tests for unchanged guarantees, adding cases where geometry changes invalidate them.
- Obtain native Windows evidence through the ordinary shell composition and selected provider
  attachment: first visible tail, long wrapped record visibility, exact fractional wheel movement,
  keyboard movement, detached anchor preservation during resize/style/content changes, resident
  edge clamp and no shared scrollbar. Include a sufficiently large admitted fixture to demonstrate
  bounded realized counts and acceptable debug-build scrolling. Launch best-effort minimized;
  preserve any activation/focus actions necessary to verify the behavior.
- Verify replacement, window close and late observation cannot retain or recreate measurements.
  Keep diagnostics content-free. Use focused nextest targets plus App compilation, required native
  qualification and independent semantic review of the complete changed viewport boundary.

## Review Outcome

Source-backed readiness is complete for the measured viewport boundary. No target-authority gap
was found for that slice. Reused evidence is limited to phase 751's unchanged snapshot ownership
and disposal qualification: 117/117 cases across twelve targets, run
`5bd6271a-420b-4ab2-ae66-808c594ee6a4`, plus the default App check. No tests or native qualification
were rerun for this documentation-only review; that evidence does not qualify the proposed behavior.
The Checkpoint 4 full mounting and diagnostic-child items remain open.

## Implementation Qualification Correction

Phase 753's implementation and independent review invalidated the assumption that synchronous
panel measurement through the current frame helpers is sufficient for the next boundary.
[Measurement publication failure](../failures/transcript-measurement-publication.md) records
the anchor/shrink and final-clamp counterexamples, evidence limits and proposed clean correction.
Operator approved the bounded measurement transaction on 2026-10-10. The renderer architecture's
measured-frame preparation and the shell boundary's measured-publication gate resolve that
prerequisite at the actual selected attachment consumer. The source inventory and unchanged
snapshot evidence remain usable. The acceptance below qualifies the corrected viewport boundary;
no rejected provisional implementation or earlier native attempt supplies acceptance evidence.

## Measured Viewport Acceptance

Accepted on 2026-10-10 for phase 753. Production host/panel preparation measures at most 32 new
natural layouts per frame, carries a bounded cursor and exact displacement, and publishes only
fully qualified viewport/overscan geometry. Initial attachment, activation, ordinary refresh,
first-conversation setup and recovery consume the same ready gate. Selected title, lineage and
claim publish with the corresponding measured attachment. Preparation retains the previous coherent
output; width, font/style, appearance and record changes invalidate observations. Same-view input
survives refresh; changed selection and retirement release input and candidate state.

Recovery completes measured publication before reopening admission or releasing canonical workers.
The final gate checks the qualified Home generation, original cancellation, exact selected composer
and retained recovery ticket while the composer is still fenced. Terminal rejection retains only
the failure diagnostic, releasing the candidate payload. Every asynchronous consumer refreshes
current appearance before accepting prepared geometry. Independent semantic review accepted the
complete changed boundary after these corrections; no blocking source finding remains.

Verification uses stable Cargo with `.cargo/local.toml`, one build job, development/test debug
information zero and incremental compilation disabled:

- App compilation passes. Combined nextest run `e5b470ef-431e-4ef1-ab4b-91d91881ee8f`
  passes 84/84 cases across measured preparation, realized frame, activation, status facts,
  renderer selection, selection copy and source-boundary targets. The 16 preparation cases cover
  fractional geometry, bounded traversal, invalid observations, initial tail, both edge clamps
  and zero-input shrink without retaining crossed-row measurements.
- App library run `80cad152-00a1-45f1-b5bb-aafcc43360b9`, with `test-faults` and one test thread,
  passes 33/33 cases: seven GPUI panel cases, twenty provider cases and six committed
  first-conversation recovery cases. Actual GPUI layout qualifies multi-frame first publication,
  queued input, same-view content refresh, resize/rem changes, detached anchors, zero-input shrink,
  selection replacement and disposal. Recovery includes cancellation, stale selection, rejected
  widget release and retries.
- Native Windows ordinary-shell case
  `app_services::tests::startup_owner::running_exit_work::ordinary_commands::transcript_viewport::native_mounted_transcript_publishes_measured_tail_and_routes_fractional_input`
  passes in 6.13 seconds, process 19100. Thirty-two admitted terminal history records include
  long wrapped text. It verifies the first published tail against actual transcript slot bounds,
  fewer than 32 realized records, transcript glyphs emitted in the panel paint callback, exact
  fractional wheel and keyboard movement, detached anchor preservation through native width resize
  and a large displacement clamped at the leading resident edge.
- Native ordinary claim recovery case
  `app_services::tests::startup_owner::ordinary_running_home::claim_activation::native_original_ordinary_committed_claim_recovers_exact_nonempty_target`
  passes in 6.78 seconds, process 4888. Both native binaries were launched best-effort minimized;
  required placement/activation assertions remained enabled. Processes exited and exact owned
  fixture directories were reclaimed after verification.

The native paint assertion observes glyphs during `content.paint`, before GPUI swaps and clears
its frame buffers. It proves transcript paint submission; it does not claim independent screen
pixel capture. Native input/resize evidence and actual GPUI font/content/retirement evidence
together qualify this scoped viewport boundary. The full Checkpoint 4 transcript, resource/adjacent
delivery, Markdown, nested widgets, focus pins, live suffixes and diagnostic-child readiness remain
open. This acceptance supplies their geometry foundation, not their implementation.

## Resident Demand Delivery Readiness

Reviewed on 2026-10-10 for phase 754 against the presentation system's residency, asynchronous
identity and disposal contracts, the transcript feature's resident-edge clamp and post-publication
resource behavior, the App worker boundary and the bounded-resource system. This is source-backed
qualification; it supplies no new runtime acceptance evidence.

Independent semantic review accepted the source inventory and adjacent implementation boundary
without blocking findings. No source changes or runtime checks were performed for this review.

The production entry is the selected shell's `SyndicTranscriptPanel` and its
`SyndicTranscriptHost`, with the selected `TranscriptProviderReader` supplying Syndic-only reads.
The measured viewport is accepted, but demand delivery is absent:

- `shell/syndic_transcript/host.rs::accept_measured_frame` commits measured facts and obsolete
  intervals. `panel.rs::report_nested_resource_demands` emits bounded resource requests for
  realized references. `demand.rs::DemandFactSink` retains at most 128 facts and drops the oldest
  on overflow. These facts are observations, not durable request custody.
- `core.rs::push_demand_fact` updates visible/overscan ranges and pins, but adjacent and resource
  facts perform no load. A scoped search of App production source finds only the core and host
  definitions of `drain_demand_facts`; no production consumer drains them. The host exposes no
  production dispatch owner for these observations.
- `transcript_provider.rs::prepare_attachment` owns one pending reservation, exact weak host,
  activation/request identity, Home/service generation, cancellation checks and observed-coherent
  publication. `source.rs::read_bounded_activation` reads at most 64 records and 65,536 bytes
  around initial placement. It returns exact view positions and edge flags, but both cursor fields
  are absent. Resource references validate durable metadata without admitting that metadata or
  resource bytes into the resident resource snapshot. Repeating attachment preparation is not
  an adjacent-demand delivery protocol.
- `provider.rs` requests and responses carry a local request id plus operation payload.
  `core.rs::ProviderRequestRecord` adds a resident generation; `finish_current_provider_response`
  checks that generation. Together these do not establish the full asynchronous host, activation,
  selected authority, service and range custody required by the normative system. Local request
  counters begin again in an empty core, so request-id equality alone cannot qualify a completion.
- `core.rs::release_obsolete_resident_data` can release unprotected records, projections and
  unreferenced resources while retaining protected identities. No production source invokes it.
  The next consumer must translate index observations to stable identities before resident edits
  and rebase the measured window after release; stored indices cannot authorize later eviction.
- `TranscriptProviderReader::retire` closes admission and reports whether its pending reservation
  has settled. `running_threads.rs::retire_source` also waits for retained workers. These existing
  attachment gates are reusable; they do not yet own cancellation, response-sink closure or join
  for adjacent/resource workers. `host.rs::retire` replaces resident state with an empty host.
  A new asynchronous owner must settle work before that terminal disposal, and recovery must keep
  only the already accepted inert coherent snapshot until replacement publication.

The normative architecture already decides the required identity, ownership, cancellation,
rejection, release and publication behavior. No new history mode, resource governor or product
decision is needed for adjacent delivery. Missing production mechanisms are implementation work,
not missing authority. The next bounded acceptance is ordinary selected-view adjacent-page
delivery through the host residency boundary, including its asynchronous custody and obsolete
release. It must use exact Syndic view generation/positions, preserve the current measured anchor,
clamp while pending and publish only after the replacement resident window is measured.

Resource delivery remains a separate consumer qualification. Syndic's
`read/range.rs::resource_text_range` provides bounded resource-relative text ranges with exact
continuation and explicit rejection of non-text backing; it is not a general media range source.
The panel's metadata-free preview default cannot prove a valid extent or backing. Qualify metadata
before dispatch, resource-relative offsets and revisions, text continuation, and the owning asset
route for non-text resources before scheduling resource delivery. Stable unsupported/unavailable
states remain permitted by the feature contract. Resource preparation must follow selected-view
publication and must not strengthen its readiness gate.

### Adjacent Delivery Acceptance Boundary

Implement one host-owned current-demand owner, bounded by the existing residency policy, with
bounded Syndic page/projection reads off the GUI thread. Give each dispatch exact host lifetime,
activation, Home/provider service, selected transcript authority, logical range and fresh request
identity. A response may publish only while that complete demand remains outstanding. Coalesce
replaceable observations from the current measured snapshot; dropped queue entries cannot leave
obsolete loads or pins permanently alive. Preserve current visible content, anchor and existing
selection/menu pins during obsolete release. Frame preparation and final publication must reject
any generation, range or environment change discovered while the response is pending.

Verify both leading and trailing extension beyond the initial seed, exact fractional scrolling
and detached-anchor continuity across prepend/release, terminal edges, repeated/coalesced demand,
capacity rejection and immutable prior snapshots. Exercise late responses after range supersession,
same-view authority replacement, thread switch, provider retirement, recovery and window close;
prove cancellation/join, returned-buffer release and no resurrected demand. Use focused core,
provider and GPUI tests, App compilation, native ordinary-shell scrolling across more than one seed,
and independent semantic review of the complete mounted boundary. Preserve separate gates for
resources, Markdown, nested widgets, focus pins, live suffixes and diagnostic-child readiness.
