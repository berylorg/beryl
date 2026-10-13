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
accept resource delivery, Markdown, nested widgets, focus-pin mounting, streaming history publication or the
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

## Adjacent Delivery Acceptance

Accepted phase 755 on 2026-10-10. The ordinary selected shell now dispatches bounded adjacent
Syndic page/projection reads from the host's current committed measured-frame demand. The host
derives a replaceable exact view, activation/presentation revision, provider revision, direction,
logical range and protected-record identity key directly from current state; an overflowing
observation queue cannot retain obsolete request authority. Shell custody adds exact selected
window/binding, weak host lifetime, fresh checked request counter and the current reader/provider
service. Prepared source results retain the existing exact Home generation and observed-coherent
publication election. Reads admit at most 64 records and 65,536 bytes, with backward reads ending
at the requested leading boundary rather than returning a disconnected prefix.

Extensions remain private candidates until measured publication. Before admitting a page, the
candidate resolves obsolete indices against their original stable identities and releases only
the unprotected opposite edges, retaining a contiguous window containing the current anchor,
visible/overscan content and existing pins. Admission checks record capacity before mutation;
refusal keeps a stable resident-edge clamp. The previous immutable snapshot remains unchanged.
The measured controller rebases by stable anchor identity and preserves exact fractional input.
Input, pin changes, candidate disposal and changed authority revoke obsolete preparation. Final
measurement also qualifies the exact predicted transcript body allocation, appearance, viewport,
scale, rem and text environment. Normal refresh under unchanged history authority retains the
extended window; changed selected title/lineage metadata still publishes coherently.

Adjacent work uses the existing tracked worker and returned-output lifetime guards. Removing demand
cancels its request; publication rechecks the complete current demand, reader, selection and
cancellation before electing the observed Home state. The existing suspension/drain path settles
work and returned buffers before provider/source retirement. Selected-thread replacement,
recovery and ordinary close already share that production path. A surviving host retains its
bounded coherent snapshot as inert presentation; host disposal releases it and rejects later work.
Independent semantic review accepted the complete mounted boundary and verification without
remaining blockers or scoped acceptance gaps.

Verification used stable Cargo with `.cargo/local.toml`, one build job, development/test debug
information zero and incremental compilation disabled:

- Combined nextest run `da399f9a-2156-4f97-b09b-d5f1c30bb261` passes 119/119 cases across the
  App library and seven focused integration targets: activation, measured preparation, realized
  frame, status facts, renderer selection, selection copy and source boundary. New core cases
  verify capacity refusal, changed authority rejection and immutable prior snapshots.
- Corrected App library run `a703c48d-f1f6-4d87-89dd-fba725657ee2`, with `test-faults` and one
  test thread, passes 33/33 provider and GPUI panel cases. Adjacent-specific cases cover exact
  range/direction, empty or oversized range refusal, late authority/lifetime/retirement rejection,
  reservation release, repeated demand, measured prepend, anchor/fractional continuity, actual
  obsolete trailing-record release, input cancellation and allocation/pin invalidation.
- App compilation passes in 15.84 seconds with the existing 342 warnings.
- Native ordinary-shell case
  `app_services::tests::startup_owner::running_exit_work::ordinary_commands::transcript_viewport::native_mounted_transcript_scrolls_across_adjacent_seeds_in_both_directions`
  passes in 52.82 seconds, process 30404. It seeds 160 admitted terminal history records, traverses
  multiple resident seeds in both directions, verifies opposite-edge eviction and stable anchor
  coordinates, and reaches the trailing terminal boundary. With an exact adjacent task outstanding,
  it establishes the existing close interaction gate, invokes production suspension, waits for
  tracked worker/output settlement and source retirement, preserves the inert snapshot, disposes
  the panel and verifies no resurrection. This tests the common lifecycle used by selection
  replacement, recovery and close; it does not claim a separate OS-level window-close experiment.
- The original native measured viewport/fractional input/glyph-paint case passes in 4.93 seconds,
  process 4908. The original ordinary committed-claim recovery case passes in 6.88 seconds,
  process 1412. All three programs requested best-effort minimized startup while retaining native
  activation and placement assertions.

Two native fixture failures supplied no acceptance evidence: extending repeated-byte test IDs
collided with an existing fixture identity, and accepting the initial empty frame as readiness
reached a content assertion before attachment publication. The corrected fixture uses separate
tagged item/draft identity domains and waits for nonempty measured content plus a coherent selected
title. Production gates were not weakened. The aborted process's exact retained Home directory
was reclaimed; accepted processes exited and their exact fixture and task directories were reclaimed
after review. An initial combined filter spelling was rejected before execution; the corrected
binary-substring filter produced the accepted 119-case run.

This accepts ordinary adjacent page/projection delivery only. Resource metadata/range delivery,
resource rendering, Markdown, nested widgets, focus pins, live suffixes, complete Checkpoint 4 and
diagnostic-child readiness remain separate gates.

## Resource Demand Consumer Qualification

This source-backed qualification covers post-publication delivery to the selected measured host,
under the transcript-presentation, Syndic-history, bounded-resource and image-asset authorities.
It accepts no production resource delivery or rendering. The existing adjacent acceptance remains
reusable for worker/output custody and measured publication; resource-specific identity and
admission still require their own implementation evidence.

### Current Consumer And Admission Gaps

- `panel.rs::report_nested_resource_demands` visits exact realized record identities and emits
  resource-range facts and media pins. `resource_demand_range` defaults to a preview-sized range
  when metadata is absent. An empty/unusable explicit preview or zero-length metadata also reaches
  that default; absent preview with positive length instead derives a bounded `0..byte_len`. A
  queued fact is therefore an observation, not proof of an admissible byte extent. Derive current
  demand from the committed measured window and protected records, first requesting metadata.
  The bounded drop-oldest fact queue cannot own outstanding work.
- `core.rs::request_resource_range` checks reference, fallback, pending work and slice policy,
  but does not require metadata or prove its requested extent against resource length. Its
  observed revision falls back to the selected provider revision. Metadata and range admission
  overwrite that selected revision with a resource revision. These distinct identities must be
  kept separate before mounting; a resource result cannot advance selected history authority.
- `core.rs::handle_provider_response` settles a locally current request id before admitting the
  response. Resource admission checks that the resource remains referenced, but does not establish
  complete request-kind, resource, backing, range, revision and byte-count agreement. The mounted
  boundary must validate these facts before mutation. Resident coverage must require the same
  resource revision; numeric interval coverage alone cannot make stale bytes current.
- `source.rs::read_bounded_activation` point-reads resource metadata to establish item/projection
  ownership, but publishes only resource-reference descriptors. The mounted reader has no resource
  dispatcher. Resource preparation follows selected-view publication; missing resource bytes never
  hold selected title, lineage or recovery admission closed.

### Text Extents And Continuation

`syndic-storage/src/record/projection/resource.rs::ResourceMetadataRecord::new` derives text byte
length from its immutable backing range and validates preview and table-header extents relative
to that length. The source range of the projection itself is a different coordinate space.
`read/range.rs::resource_text_range` accepts only `ResourceBacking::TextRange`, checks the requested
half-open interval against resource length, adds the backing start with checked arithmetic and
reads the indexed source. Its payload limit is positive and at most 65,536 bytes. Returned start
and exact continuation are resource-relative; an empty request returns no bytes and no continuation.

For example, a code resource backed by source bytes `1000..91000` has logical bytes `0..90000`.
A demand for resource bytes `200..80000` reads source bytes beginning at `1200`. At a 65,536-byte
payload cap, the first result occupies `200..65736` and continuation is `65736`. A further read is
allowed only while the remaining interval is still demanded. Completion means reaching the end
of that requested interval, not loading the complete resource. Range bytes may split a UTF-8
sequence; delivery preserves exact bytes and text presentation owns boundary-aware interpretation.

The existing `syndic-storage/tests/projection_boundaries.rs` case
`heavy_resource_text_is_range_read_with_exact_bounded_continuations` proves byte-exact chained
reads, missing and empty ranges, invalid limits and invalid extents. Its source was inspected;
it was not rerun for this documentation-only qualification. Mounted continuation and stale-result
behavior are not covered by that storage test and remain next-phase acceptance requirements.

The delivery key must retain exact window/host lifetime, activation, Home identity/generation,
selected thread/view/history authority and provider-service generation, owning presentation record/projection/item, resource
id and resource revision, backing identity, requested interval and fresh request id. Reconfirm the
selected publication and metadata backing before returning and before host admission. Keeping
the durable metadata privately in provider preparation avoids exposing storage mechanics to the
panel. Resource continuation must not replace the outstanding requested interval with an
unrelated prefix or promote a prior response under a new request id.

### Non-Text Ownership And Separate Media Gate

`ResourceBacking::GeneratedMedia` has `PendingAsset`, `Asset(AssetId)` and terminal `Unavailable`
dispositions. Pending/unavailable metadata deliberately omits byte length and digest; absence is
not a zero-length text resource. Admitted metadata obtains length/digest from the exact asset.
`resource_text_range` rejects every generated-media disposition with `ResourceHasNoTextBacking`.
It supplies no image reader, decoder or sidecar authority.

The image-asset system owns admitted sidecars, original-image verification and bounded decode,
rendition, upload and cache admission. `beryl-state/src/asset.rs::verify_sidecar` verifies the exact
asset address and admitted length; it does not prove selected repair visibility. The image-asset
authority's selected repair-resource route additionally requires the exact thread/turn,
item/resource locator, asset and sealed commitment. AssetId-only lookup cannot discover repaired
staging or substitute another asset. Neither the current transcript `ResourceMetadata` nor
`ResourceRangeResponse` carries this owner-qualified media authority. The mounted activation
resource-reference arm currently requires a matching ordinary projection owner; this qualification
does not establish a mounted selected-repair-media reader.

Consequently the next text-delivery boundary must leave non-text byte demand explicitly locally
unsupported/unavailable, preserving any admitted descriptor and stable identity. It must not
attempt text reads, follow a provider path, load an original image into a text slice, reinterpret
missing length as empty content, or claim image readiness. Exact asset-backed rendition and
selected-repair ownership need their separately qualified consumer before media mounting.

### Derived Delivery Acceptance Boundary

Mount one post-publication metadata/text-range consumer through the existing tracked ordinary
worker boundary. Host demand names current realized/protected resource records; metadata determines
the bounded preview extent and exact text backing. Admit only exact current responses, with
resource revisions separate from selected history authority, and continue only currently demanded
missing intervals. Metadata-only and byte readiness remain independent of thread publication.

Use the existing finite pending, slice and resident-resource budgets. Preserve local stable
missing/unsupported/capacity outcomes without repeated dispatch for the same current demand;
failed invocations may use the established retry behavior with fresh ids. Cancel removed or
superseded demands, release obsolete metadata/slices/rejections and return all worker/output
charges on cancellation, stale return, selection change, recovery, source retirement and close.
Preserve the prior immutable snapshot; any changed measured resource-shell geometry must pass
the current allocation/appearance gate and keep the semantic anchor.

Required implementation evidence includes nonzero backing offsets, short/empty/invalid previews,
bounded multi-page continuation, split UTF-8 bytes, wrong resource/revision/backing/range/byte count,
request-kind mismatch, supersession, pin/viewport removal, terminal non-text rejection, capacity
refusal and drain to baseline. A native mounted text-resource case must prove metadata-first
post-publication loading, unchanged selected authority/title, stable anchor and disposal while a
read is pending. Resource rendering, Markdown structure, nested interaction, asset renditions,
focus-pin mounting and live suffixes retain their separate gates.

Independent semantic review accepted this complete qualification and the derived text-delivery
boundary after clarifying preview fallback cases and explicitly retaining Home identity/generation
in request custody. No source or test changes and no runtime acceptance are claimed for this phase.

## Post-Publication Text Resource Delivery Acceptance

The ordinary selected transcript now starts metadata-first text delivery after its measured
selected-view publication. Current realized/protected records supply demand; the existing tracked
worker prepares exact Code/Table metadata and bounded missing byte intervals. Selected title,
lineage and history revision remain independent of resource readiness and resource revisions.

Request custody includes Home identity/generation, provider generation, host lifetime, activation,
selected source authority, owning record/projection/item, exact resource and retained pin categories.
The worker checks immutable backing, relative extents and bounded continuation, reconfirms selected
head/summary, rereads backing before return and retains a coherent Home mutation observation.
GUI admission elects that observation and checks private prepared/request identity without durable
reads or re-entering the Home health lock. A superseded observation cannot publish.

Core admission verifies response kind, resource, revision, interval, byte count and completion
before capacity mutation. Metadata validates preview extents; absent metadata never produces a
guessed range. Missing intervals require the admitted resource revision. Terminal local
missing/unsupported/capacity outcomes preserve record identity and stop dispatch; partial capacity
refusal releases prior slices. No automatic preview pin holds an offscreen row indefinitely.
Demand removal releases obsolete metadata/slices, bounded rejection keys and pending custody.
Explicit pins remain distinct even when they protect an already visible record.

Resource candidates pass the current allocation and appearance measurement before publication,
preserving immutable prior snapshots and the manual semantic anchor. Manual input or changed
demand revokes a candidate. Every resource attempt yields once to ordinary refresh/adjacent work,
so stale resource work cannot starve a newer selected history publication. Cancellation, suspend,
source retirement and output disposal share the existing tracked worker drain boundary.

### Verification

The focused final provider/panel run passed all 42 cases
(`e9d4781f-5231-4049-974b-c741ae44c615`). It covers exact backing
offsets and bounded multi-page continuation, split UTF-8 bytes, preview extents, request/backing
supersession, cancellation, non-text rejection, immutable candidates, capacity and pin custody.
Final combined regressions passed all 122 cases across activation, resident resources, provider
resources/revisions, release, realized frames, selection/copy, media actions, measured preparation
and source boundaries (`95b454b2-0f56-4eae-a748-ce4fade31616`). Both runs used
`cargo +stable --config .cargo/local.toml nextest run -p beryl-app --features test-faults`;
the focused run selected `--lib -E 'test(transcript_provider) | test(syndic_transcript::panel)'`,
and regressions selected those ten integration targets.

Two exact native cases ran from `target/debug/deps/beryl_app-06ddcee903197152.exe` with
`--exact --ignored --nocapture`, through `Start-Process -WindowStyle Minimized`:

- `native_mounted_text_resources_load_after_selection_and_dispose_pending_ranges` passed in
  4.52 seconds, PID 29864, exit 0. It proves initial selected publication without metadata/slices,
  subsequent loading without changing selected title/authority, fractional wheel/manual-anchor
  preservation and disposal while an actual range is pending. Retained prior snapshots stay
  immutable; suspension drains worker/output custody without resurrection.
- `native_mounted_resource_failure_yields_to_selected_history_refresh` passed in 2.31 seconds,
  PID 24340, exit 0. Advancing selected head and summary during a pending resource read publishes
  the newer source despite the stale resource attempt.

Both cases live under
`app_services::tests::startup_owner::running_exit_work::ordinary_commands::transcript_viewport::resources`.
These tests prove mounted measurement and lifecycle behavior; they do not establish screenshot or
GPU-paint acceptance. Task-owned native Home directories were reclaimed after process exit.
Independent final semantic review accepted the complete mounted delivery boundary after directly
checking the focused, combined and native evidence, exact pin/capacity regressions and fixture
corrections. No remaining blocker or required source correction was identified.

Native loaded text exposed an unresolvable literal `monospace` font family in the existing shell.
The shell now uses `Consolas`, matching the application's code-font default; focused verification
and the successful native loading case include that correction. Existing media-action fixtures
also needed previews bounded to their actual payload length under strict metadata admission.

Non-text asset preparation/renditions, full resource rendering, Markdown structure, nested
interaction, focus-pin mounting and live suffixes remain separate acceptance gates. Ordinary
projection ownership remains required; this boundary supplies no selected-repair-media reader.

## Resource Rendering Consumer Qualification

This source-backed qualification follows accepted text delivery at `b82cb387`. It establishes
readiness and missing capabilities for Code/Table presentation, not runtime or GPU-paint acceptance.
Controlling authorities are the transcript feature's [design](../features/transcript/design.md)
and [GUI](../features/transcript/gui.md), the
[renderer architecture](../systems/transcript-presentation/renderer-architecture.md),
[Syndic history](../systems/syndic-conversation-history/design.md), and canonical
[code panel](../gui/widgets/code-panel/spec.md) and [table panel](../gui/widgets/table-panel/spec.md).
The feature and presentation system select `production-application/v2`; consequential identity,
residency and lifecycle decisions require independent semantic review.

### Current Sources And Limits

`crates/beryl-app/src/shell/syndic_transcript/panel.rs::render_text_resource_shell` selects
`first_resident_resource_slice`; `resource_preview_text` decodes at most 2,048 bytes of that slice
with `String::from_utf8_lossy`. It neither joins contiguous pages nor consults the requested
interval when interpreting `complete`. A scalar split at the display cap or page boundary can
become a replacement character; later slices are not displayed. Normal whitespace also fails
source-preserving code layout. This shell is not a canonical Code/Table panel.

`report_nested_resource_demands` reports realized resource records; `resource_demand_range` asks
for a metadata-derived preview capped at 4,096 bytes. There is no internal viewport or line/cell
mapping. Its coverage helper looks for one slice covering the interval; contiguous multi-slice
coverage requires an interval union. Accepted core delivery already finds missing intervals across
current slices and bounds preview demand separately. The old panel fact queue cannot own outstanding
work or establish internal scroll demand; reconcile these preview policies in a mounted consumer.

`shell/render/code_panel_projection_cache/request.rs::CodePanelSourceRevision` owns complete
display/copy strings. Its worker calls `CodePanelDisplayProjection::new` in
`shell/render/code_panel/projection.rs`, constructing all display lines and scanning for maximum
text. Later window selection bounds returned lines, not initial projection. These helpers can
process an explicitly small admitted fragment; they do not qualify arbitrary-size range-backed
code panels. Fragment-local string offsets cannot replace exact resource identity and offsets.
Source searches found no `TablePanel`/`table_panel` implementation in `crates/beryl-app/src`.
Canonical specs define the target; they do not prove an existing ready consumer.

### UTF-8 And Partial Presentation

Presentation must group only contiguous slices with the same resource and revision, intersect
them with the exact demanded interval and preserve resource-relative byte offsets. Gaps stay
pending/unavailable; concatenating bytes across a gap invents contiguous source text. Coverage
and completion are separate: `complete` ends the requested interval, not necessarily the resource.

For example, `A€B` occupies bytes `41 E2 82 AC 42`. Pages `0..3` and `3..5` split the euro sign.
Independent lossy decoding creates false text. Boundary-aware preparation joins current contiguous
bytes before strict decoding, preserving `A€B` and its offsets. A bounded streaming decoder needs
at most three pending bytes at a page boundary. Carry cannot cross a gap, revision, replacement
generation or disposal.

An interval starting inside a scalar cannot invent preceding bytes; one ending inside it cannot
paint a replacement character as source. A bounded preview can expose only complete interior
scalars with distinct partial-range status. A source-preserving widget needing the boundary scalar
requires an explicitly admitted neighboring-byte demand within extents and budgets. Malformed
interior bytes require a local fallback. Display truncation ends at a scalar boundary; its status
stays outside selectable/copyable source text. These are derived preparation requirements, not
acceptance of a new production decoder.

Empty resources, explicit empty previews, missing ranges, capacity rejection and unsupported
presentation remain distinct. An empty response cannot prove an unloaded nonempty resource is
empty. A preview may stay visibly partial but cannot claim full code navigation or a structured
table. Copy and streaming Save use exact-source contracts, never painted ellipses or fallback labels.

### Missing Structural Capability

`syndic-storage/src/record/projection/resource.rs::ResourceStructure` stores code language and
logical-line count, or table body-row count, column count and resource-relative header range.
`mutation/projection/parser/output.rs::emit_code` externalizes the code body; `emit_table`
externalizes original Markdown table bytes. Counts and one header interval do not locate arbitrary
lines, rows or cells. The inspected public read surface supplies metadata, resource pages and
`read/range.rs::resource_text_range`: exact byte lookup through indexed text backing, without a
logical-line or table-cell query.

`beryl-app/src/transcript_provider/resources.rs::resource_descriptor` transports counts and
preview extent but drops code language and table header range. Adding those fields alone cannot
supply stable line/cell descriptors. Splitting a table preview on pipes would rediscover Markdown
structure in Beryl and mishandle escaping, partial rows and source/display mapping, contrary to
Syndic projection ownership.

Full consumers need a separately qualified bounded structural query: stable source positions,
line/row/cell identities, exact ranges and cell presentation from Syndic, with indexed lookup that
does not scan a complete prefix to find a late viewport. Construction, publication, revision
mapping and pathological-content limits must be resolved in owning design before implementation.
Accepted byte delivery supplies reusable custody, not proof that this query exists.

### Demand, Geometry And Next Acceptance

Canonical code panels require source anchors and bounded logical/wrapped-line realization; table
panels require bounded two-axis realization over owner-supplied stable rows, columns and cells.
Both report demand through residency and consume admitted data without storage access or an
independent unloaded-history cache. Resize, wrap, appearance and source revision reconcile stable
panel identity and anchor; same-source changes need owner-supplied bounded revision mapping.

The outer renderer retains one bounded panel record. Actual layout, including header, viewport,
feedback and fallback changes, passes the exact measured-publication gate; estimated inner tracks
cannot qualify guessed outer height. Only current realized panels construct elements. Internal
visible-range changes update authoritative demand even when the outer record window is unchanged,
cancel removed work and release obsolete slices, descriptors, layout and charges. Candidate
preparation exposes no interactive targets or committed demand before coherent publication.

After structural qualification, mounted evidence must distinguish multi-page UTF-8 joins, clipped
previews, invalid interior bytes, gaps and stale revisions; late line/cell lookup without prefix
scans; huge lines/cells and wide tables within documented limits; bounded realized rows/columns
and layout; resize/wrap/appearance anchor reconciliation; local fallback and exact outer measurement;
inner-demand changes/release and pending disposal. Native checks must establish actual mounted
panels and geometry with minimized startup where practical. Byte-delivery tests cannot substitute
for rendering or visual acceptance. Nested interaction, selection/copy/Save, wheel routing, focus
pins, Markdown structure, assets/renditions and live suffixes retain separate gates.

No source or tests changed; prior delivery results remain reusable only for unchanged byte custody
and measured resource publication. Structural-query readiness is the next prerequisite; full
Code/Table mounting remains unaccepted.

Independent semantic review accepted the complete qualification and derived structural-query
prerequisite after directly checking the cited sources, unchanged against `b82cb387`, and owning
widget/feature/system contracts. No blocker or required correction was identified. Local links
and `git diff --check` passed; no builds or tests were run for this documentation-only boundary.

## Resource Structure Query Qualification

The [Syndic structural-query contract](../systems/syndic-conversation-history/design.md#indexed-text-resource-structure-queries)
defines separate completed companions for immutable Code/Table resources, exact-root indexed
line/fragment and cell-rectangle lookup, source-anchor resolution, bounded source-preserving cell
presentation, resumable construction and coherent publication. Storage public-boundary and
renderer custody documents link that owner. The concepts supplement now distinguishes raw
byte-range payload reads from typed companion structure queries.

Construction and queries have separate bounds: source-page progress remains resumable, while a
late viewport seeks indexed descriptors without a prefix scan. Responses cap descriptors and
encoded bytes, charge source mappings/display fragments and carry exact continuations. Finalized
resource records remain immutable; a companion does not change their identity or source revision.
Inner viewport demand, cancellation, stale completion and release stay with existing selected-
history/provider/residency custody. Structural readiness does not accept outer widget geometry.

Existing V1 `parser.rs::process_line` accepts body rows without requiring their cell count to
match the header, and `parser/output.rs::emit_table` persists header-derived columns. Therefore a
two-column table containing `a|b|c` cannot be repaired by mutating finalized counts or dropping `c`.
The selected whole-row fallback keeps the exact row identity and raw source range, is returned
once by any intersecting nonempty rectangle, and permits bounded source-anchor/byte lookup and
explicit raw-row Copy/Save. It supplies no invented authored cell selection.

Semantic query authority does not specify new persisted bytes. The next prerequisite defines
families, codecs, index ordering, source/root commitments and publication/reconciliation proofs
in the package's sole [schema authority](../../crates/syndic-storage/doc/design-schema-v7.md), with
exact inventory and home-admission reconciliation before implementation. No storage index,
provider query or mounted Code/Table consumer is implemented or accepted by this qualification.

Independent semantic review accepted the corrected complete design boundary after inspecting
owning feature/widget/system/package contracts and exact parser/resource-emission symbols.
The prior semantic-table-read prohibition and unspecified surplus-row delivery were resolved;
no blocking finding remained. Root verification passed local Markdown link checks and
`git diff --check`; this documentation-only phase ran no builds or tests.

## Resource Structure Persistence Qualification

The sole [V7 byte authority](../../crates/syndic-storage/doc/design-schema-v7.md#resource-structure-companion-canonical-encoding)
now defines five companion primary families and five coordinate/source index families, exact
keys and payload order, scalar widths, closed tags, immutable source binding, digest domains,
bounded resumable passes and atomic page/index/build/receipt/root/head closure. Table derivation
uses bounded persisted passes rather than retaining all rows or cells. Literal source presentation
and identity mappings preserve raw Copy/Save authority; whole-row fallback retains surplus source.

Target inventory is 74 primary plus 29 index families, 103 unique names. The package entry's stale
92-family count was reconciled. The current implemented declaration remains 89 families, with
four target terminal-repair families deferred; this qualification adds no registration placeholders.
The target registration encoding is 5,891 bytes, within HomeStore's 8,192-byte ceiling. Matching
homes with empty companion families are valid and derive companions on demand. Differing persisted
declarations retain existing strict rejection policy; this work specifies no home migration or
deletion and routine open performs no application-record scan.

Independent review checked the complete persistence/query boundary, key and payload lengths,
checkpoint bounds, digest dependencies, indexed lookup, exact publication/reconciliation and
restart custody. The default scalar-width gap was corrected to exact eight-byte unsigned
big-endian fields; one-byte tags remain explicit. No blocking finding remained. Root checks
confirmed inventory uniqueness, registration size, local Markdown targets and whitespace.
No production code, codec fixtures, runtime queries or rendering behavior are accepted here;
construction/query/provider/residency verification is the next implementation boundary.
## Mounted Indexed Code Acceptance

Accepted on 2026-10-12 under the transcript feature, Code widget and presentation-system contracts.
The production Syndic panel realizes bounded readonly no-wrap Code for every recognized saved fence,
including empty and unfinished bodies, without a CAS item-completion gate. Bounded range demand,
font-observed geometry, coherent outer measurement, independent inner scrolling, exact source
anchors, resident selection and clipboard-limited Copy passed native and semantic qualification.
Save streams complete exact bytes through worker-owned staging; explicit native Yes permits
replacement. New choices preserve collision protection. Cancellation and retirement fence publication
before its one-shot election; elected publication and original-root shutdown drain retain custody
through worker completion. Cleanup refusal reports a possible owned temporary output.

Combined final Beryl qualification passed 138/138 cases in 329.518 seconds
(`e54f387e-28b1-4d3c-ba83-f0da7bfe903f`): 83 panel, 22 native integration, 31 provider and two admission
cases. Actual Windows Save scenarios and checked clipboard passed 23/23 in 2.713 seconds
(`908f3ef1-f450-4c9f-abbc-f746a604e649`). Native mounted Copy captured the checked writer invocation;
the privileged administrator-only clipboard target was not run. Prior unchanged dependency evidence
is GPUI 63 cases (`dfe2207c-b27f-4139-8a24-8d696f49dbe8`) and text-input 90 exact-geometry cases
(`e13aff1f-680f-420a-83d0-ce182bc94bce`); provider/publication 123 and storage 144 cases were reused.
The cumulative streaming-family fixture uses a sixty-second watchdog with unchanged per-step
assertions. Retained failure records explain publication election, refusal/retry, staged delivery,
font/layout capacity, native choice authority and shutdown custody.

Independent behavior and dependency-delivery reviews found no remaining blocker. Published GPUI is
`14d9622001ecbfa0db4a5530b989b649690472a1`, scrollbar `25e09652f356bc2cb163c78912bfa7a61534aacd`,
text-input `aa9dfd3b727450556570637c133de319a1f78d7e` and settings
`f4c6b19e880f25fe06edd4c2fd64cbca9dd40091`. Canonical isolated locked metadata proves one GPUI
and the published widget sources; only fourteen expected Git-source entries changed in the root
lock. Production checks passed for all widgets and Beryl (canonical 35.58 seconds, local 12.48);
the analyzer restarted successfully afterward. Review matched all 22 Beryl and four native GPUI
Save-owned source/test hashes to accepted inputs. Text-input's published two-line continuation
correction matches the unchanged accepted behavior; no retained prior raw test hash is claimed.
Temporary verification inputs and fixture homes reached their last review consumer at acceptance;
this record and version control retain the durable outcome. Table mounting remains unaccepted.

## Fenced Code Storage Form Qualification

Inspected at `e2ce90f6` on 2026-10-12 for root-plan phase 768. The assumed small inline fenced
Code path does not exist in the production producer. The controlling
[projection contract](../systems/syndic-conversation-history/concepts.md#markdown-projection)
already requires every recognized fence to become a Code resource, including empty and
unfinished fences. The 2,048-byte/eight-line bound limits previews, not storage-form selection.
Inline code spans belong to their surrounding narrative block and are a different consumer.

Storage `mutation/projection/parser/output.rs::emit_code` unconditionally emits
`ParserOutput::Resource` with the exact body range and Code structure.
`mutation/projection/materialize.rs::materialize_output` maps that output to
`ProjectionPayload::ResourceReference` and `ResourceKind::Code`. Opening/closing fence lines
are excluded from body source; an opener without a body has an exact zero-length range.
The generic inline payload constructor accepting a block-kind argument does not establish an
inline fenced-Code producer. Table-specific inline preparation cannot be generalized into a
missing Code path on that basis.

The existing provider resource branch, typed presentation and indexed Code panel are therefore
the ordinary consumer for these small fences. Their accepted indexed source, geometry, selection,
Copy/Save and source-qualified replacement/disposal contracts remain applicable. No inline Code
model, adapter, resource conversion, budget change or production implementation is required.
The renderer architecture now states this storage-form distinction explicitly.

Existing objective evidence covers this conclusion: storage
`projection_boundaries::fenced_code_all_sizes_crlf_and_unfinished_blocks_publish_resources` and
`empty_and_small_fenced_resources_retain_exact_body_and_language` cover tiny/large fences,
opener-only sources with and without a newline, empty closed backtick/tilde fences, exact CRLF
body bytes and an unfinished small body. These tests belong to the unchanged producer/projection
29/29 run recorded in [inline Table acceptance](#mounted-inline-table-acceptance).
Mounted Code evidence remains the [indexed acceptance](#mounted-indexed-code-acceptance),
with affected Code regressions in the subsequent 231/231 app/core/provider/native run.
Existing native `indexed_resources/code_streaming.rs::streaming_code_with_deadline` checks
zero-line opening descriptors, one-line growth, exact body ranges and stable panel identity;
`code_save.rs::native_code_save_empty_source_and_late_picker_retirement_preserve_output`
qualifies empty-source Save. The existing checked clipboard/picker evidence does not qualify
OS clipboard/common-dialog automation or the complete composer workflow.
This documentation-only qualification reuses that evidence; it runs no Cargo or native tests
and makes no new runtime acceptance claim. Narrative presentation, remaining interactions and
diagnostic-child activation remain separate mounting boundaries.

Independent read-only readiness review accepted this boundary on 2026-10-12 after checking the
controlling authority, closed/unfinished fence producer, resource materialization, authenticated
provider, typed presentation and panel mounting. It confirmed the cited tests and unchanged-input
evidence reuse, with no supported-guarantee blocker. Documentation whitespace checking passed.
The review performed no tests, edits or artifact creation. The next evidence-only boundary is
ordinary narrative presentation, whose generic Text consumer still flattens stored block facts.

## Narrative Source And Interaction Qualification

Inspected production source at `3487f5ee` on 2026-10-12 for root-plan phase 769.
This qualifies narrative implementation readiness, not mounted narrative acceptance. The
source diagnosis preceded target contract reconciliation. The controlling authorities remain the
[transcript feature](../features/transcript/design.md#selection-copy-quote-and-menus),
[transcript view](../gui/widgets/transcript-view/spec.md#interaction),
[Markdown projection](../systems/syndic-conversation-history/concepts.md#markdown-projection) and
[renderer architecture](../systems/transcript-presentation/renderer-architecture.md).

### Stored Facts And Production Consumers

Storage `record/projection/item/payload.rs::ProjectionPayload::InlineMarkdown` retains block
identity, typed block kind, exact source range, source and span ordinal. The recognized kinds in
`record/projection/item.rs::MarkdownBlockKind` include Paragraph, Heading(1–6), BlockQuote, List,
ThematicBreak and Fallback, alongside separately mounted Code and Table. Large paragraph spans
are this same payload form; a conceptual sentence/span chunk does not imply another stored type.
`mutation/projection/parser/output.rs` preserves one block identity and exact ordered source
intervals. Genuine unsplit paragraph/fallback payloads reach 16,384 UTF-8 bytes; split payloads
are at most 8,192 bytes. Long fresh lines become bounded Fallback fragments. The public inline
constructor permits 65,536 bytes, so a consumer cannot infer the narrower producer limits from
the payload variant alone.

App `transcript_provider/source.rs::read_bounded_activation` handles Table separately, but turns
ordinary inline Markdown into `ProjectionPayload::Text`. Block identity and source range survive
in provenance; block kind and span ordinal do not survive as presentation facts. The ordinal only
informs precomputed stripped Copy. `shell/syndic_transcript/snapshot.rs` publishes `TextChunk`
with original and optional stripped source, without typed narrative runs or source/display maps.
`panel.rs::render_record_content` paints raw source as a `SharedString`. Natural measurement uses
the same constructor through `layout_as_root`, qualifying matching raw-text geometry rather than
Markdown hierarchy, inline styling or character hit geometry.

Outer left-click currently focuses the transcript; realized-row handlers provide right-click
context targeting. `renderer_selection.rs` accepts explicit whole-record identities with stable
row geometry. `selection.rs::ResidentSelectedRecord` contains the whole copy-source interval,
without character endpoints. Existing keyboard Copy genuinely dispatches the first Markdown and
quick second stripped request through the checked writer, but this does not establish native
pointer selection. Table cell selection and Code source positions are separate accepted models;
neither silently supplies ordinary narrative selection.

### Readiness Decisions

The owning authorities now resolve the following implementation boundaries:

- Syndic's pure preparation boundary for already admitted block/span payloads, including retained
  source authority, supported inline formatting, source/display mappings and conservative literal
  behavior when preceding delimiter context is unavailable. Preserve block-group continuity across
  physical spans without whole-paragraph reads, a new history route or render-time parsing.
- App transport and resident ownership of typed narrative facts, allocation admission, explicit
  preparation refusal and release on source/view replacement, eviction and disposal. Current source
  plus stripped-source charging cannot stand in for charging added run and mapping allocations.
- Bounded inner preparation and shaping work, alongside the existing measured-frame observation
  bound. Preparation admits at most 65,536 source/display bytes, 4,096 runs and 16 nested delimiter
  levels; shaping receives at most 4,096 display bytes per fragment and 32 fragments per span.
  Fragment observations share the existing 32-observation GUI-frame allowance. A bounded record
  count alone does not qualify per-record layout work.
- Shared prepared input for measurement and paint, current geometry for hit testing, character
  endpoints and exact source-range binding. A fully selected construct includes its own syntax;
  partial selection invents no balancing delimiters, and transformed display units are indivisible.
  Slicing the precomputed whole-record stripped string has no
  current source mapping proof. Preserve the accepted Copy gesture, size and publication election.
- Native routed pointer selection followed by both Copy methods, without injecting whole-record
  selection. Cover wrapping, Unicode, continuation spans, source growth/replacement, remeasurement,
  unrealized boundaries and retirement. Quote, branch/menu commands and diagnostic-child activation
  retain distinct acceptance boundaries.

Selected-thread publication retains typed Pending source facts with natural measured row
placeholders and does not wait for Markdown parsing. Tracked post-publication preparation consumes
already admitted immutable source under the host reservation and existing request-count limit;
its exact request/currentness election precedes measured Ready adoption. Same-record growth keeps
the prior coherent model visible. Appended syntax can change a construct's treatment, while an
unchanged-source terminal transition reuses its published model. This reconciles preparation with
the feature's activation and live-continuity rules without a new history route. Widget anatomy and
theme roles now cover the prepared block/run treatments and mapped pointer interaction.

### Evidence And Limits

Source investigation and targeted root validation performed no production edits, builds, native
launches or new tests. Existing `syndic-storage/tests/projection_boundaries.rs` cases exercise
paragraph threshold, exact source intervals, shared block identity and span ordinals. App
`tests/unit/transcript_provider/copy_text.rs` covers conservative literal continuation Copy.
`tests/syndic_transcript_measured_preparation.rs` uses injected heights to exercise bounded record
traversal, not Markdown shaping. Native `transcript_keyboard_copy.rs` routes real keyboard events
but installs selection programmatically. Those seams remain useful; none supplies missing native
character selection or styled narrative acceptance. Runtime acceptance remains the next mounted
boundary; this readiness change does not add that evidence.

Independent semantic review accepted the complete revised readiness boundary on 2026-10-12.
It identified and rechecked the post-publication parsing and live-continuity interactions above,
then found no remaining supported-guarantee blocker. Review covered the source diagnosis,
system/package ownership, numerical limits, mapped selection/Copy and widget anatomy/roles.
Documentation whitespace checking passed. The review made no edits, builds, launches, tests or
temporary artifacts. No runtime evidence is claimed from this documentation-only phase.

## Mounted Indexed Table Readiness

Readiness inspection on 2026-10-12 continued from accepted Code mounting. This checkpoint is source
evidence and a proposed product clarification; it does not accept Table rendering or interaction.

The [Table widget](../gui/widgets/table-panel/spec.md) requires bounded realization on both axes,
stable header/body row and column identities, owner-projected cell fragments, rectangular selection,
aligned headers, local whole-row fallback and measured outer geometry. The
[structural-query authority](../systems/syndic-conversation-history/design.md#indexed-text-resource-structure-queries)
already defines exact immutable binding, row/column query continuation, raw/trimmed source ranges,
explicit empty versus missing cells, alignment and source-anchor lookup. The feature
[composition](../features/transcript/gui.md#transcript-region) configures both Copy and streaming Save.

Existing implementation facts:

- `syndic-storage/src/resource_structure.rs::Descriptor` supplies Table row, cell and column
  descriptors. `resource_structure/query_response.rs::Request::Table` addresses one header/body
  role and bounded row/column ranges; `Item::Missing` distinguishes absent authored cells.
- `beryl-app/src/transcript_provider/resources/indexed.rs` transports range, display and anchor
  results through the existing selected-history worker. The focused provider case
  `selected_narrow_table_delivery_preserves_empty_missing_alignment_and_exact_source` exercises
  a narrow rectangle with explicit empty/missing cells and Center alignment. This inspection
  does not claim a new test run or mounted native acceptance.
- `transcript_provider/resources/indexed/source.rs::read_code_source_response` admits only Code
  bindings, and the Source query authenticates its root through an empty Code request. Generalize
  this exact-source path for Table rather than adding widget-owned storage reads.
- `shell/syndic_transcript/core/resource_delivery.rs::code_panel_demands` enumerates only ready
  Code panels. `core/resource_delivery/indexed.rs` retains continuation pages only for a bounded
  Code-line request and validates Source responses only for Code. Table mounting needs bounded
  rectangle/header retention and ready-panel enumeration through this same admission owner.
- `shell/syndic_transcript/panel.rs` mounts retained Code panels; Table still follows the generic
  resource shell. Code's native Save publication and shutdown custody are qualified reuse inputs,
  whose Table integration still requires behavior evidence.

The readiness inspection identified a product decision about the clipboard representation of a
partial rectangle. For a source row `x|y|z`, selecting only the middle cell could produce its source spelling,
the complete original row, or a spreadsheet-oriented value. These differ in included columns,
Markdown escaping, whitespace and treatment of missing cells. Current authority says the owner
supplies the representation and prohibits treating painted text as source, but does not select
one of these product behaviors.

The initial proposal was selected-cell Markdown with source spelling preserved and export syntax
distinguished from authored bytes. Operator resolved and broadened the target on 2026-10-12:
transcript-wide keyboard Copy supports two methods, with one distinct `Ctrl+C` press copying
Markdown and a quick second distinct press copying best-effort stripped text. The initial inverse
ordering was explicitly corrected by the Operator before implementation. This removes the format
clarification blocker; the feature design records the corrected product contract.

Shared source-based conversion and keyboard routing now have a separate preceding acceptance
boundary. Table then integrates both methods, including header/delimiter inclusion and explicit
empty/missing-cell behavior. Whole-source Save and raw-row fallback actions retain their source
authority. The other identified gaps remain bounded Table mounting prerequisites. No new keyboard,
conversion or mounted Table runtime acceptance is claimed by this documentation update.

### Exact Table Source Prerequisite Checkpoint

Continuation on 2026-10-12 generalized the selected-history Source path to Code and Table.
The provider authenticates the exact companion with an empty query of the backing's kind,
then reads at most 8,192 original source bytes per page under the existing root, backing,
revision, budget and publication election. Residency additionally requires the response metadata
kind to match its Code/Table owner. No Table renderer, clipboard export or alternate loader is
introduced by this prerequisite.

Eleven focused current cases passed with unchanged production inputs: five existing provider
cases and the Code admission case from `aab58c34-bcc4-46bc-bff9-f6eb1be38e99`, three ready Table
provider cases and corrected admitted-payload release from
`582c9adf-4cd3-45dc-984b-fb683f87d3d5`, and the corrected companion-discovery case from
`4ab252d3-cd3c-4836-9cb6-7a77530939f3`. The earlier runs also contained superseded fixture failures,
which are excluded from that current count. Fixtures were corrected to use a resource-relative
header interval, force a split UTF-8 scalar, and distinguish exact-root refusal from rootless
Pending discovery. An incidental unchanged stable-source-identity case also passed. All runs used
the local stable Cargo configuration, locked/offline dependencies, one build job, no debug data or
incremental compilation, `test-faults` and serial nextest. Independent semantic review found no
source defect. These tests exercise provider/storage and the GPUI test host; they do not prove
native mounted Table interaction or clipboard/Save behavior. The production `beryl-app --lib`
check passed under the same local stable locked/offline configuration. Independent review accepted
this exact-source prerequisite after the corrected tests and production check completed.

Operator subsequently clarified the empty-content behavior: Markdown must preserve the original
table verbatim, including empty rows and cells; text mode omits empty rows and trailing empty or
missing cells. The feature records this product behavior and the structural system retains
empty/missing distinctions in descriptors and raw source authority, allowing the text export's
intentional omission. Operator then resolved selection scope: each cell is an indivisible
selection unit, and only selected cells are copied. A partial rectangle preserves selected raw
cell spelling with necessary export separators; selecting all cells permits verbatim whole-table
source. The feature and widget record cell selection rather than character endpoints or text spans
crossing cells. The clipboard clarification blocker is resolved. Bounded header/body retention,
two-axis mounting, export and native Table acceptance remain outstanding.

## Shared Transcript Keyboard Copy Acceptance

Accepted on 2026-10-12 after independent semantic review. Mounted ordinary transcript and Code
route distinct Ctrl+C presses through one gesture owner: first copies Markdown immediately;
a second within 600 ms requests best-effort stripped text. Held autorepeat does not advance the
pair; the next press after a completed pair starts Markdown again. Selection, raw rendered focus,
window activation, source identity and retirement invalidate pairing. A focus change alone does
not cancel an otherwise valid accepted Code source read; a later keyboard request supersedes it.

Ordinary Copy preserves existing whole-record selection. Code preserves byte endpoints and literal
source in both methods. The activation worker derives and charges stripped text alongside original
source before provider admission. Bounded scanning conservatively preserves uncertain syntax and
continuation spans whose opening literal context may be unloaded. The GUI consumes admitted source
representations without parsing or reconstructing painted text. Copy is bounded to 65,536 UTF-8
bytes; the native clipboard budget separately admits UTF-16 and allocation overhead. Missing
selection/source, capacity refusal and checked-writer refusal report unavailability without partial
publication. Ordinary rejection feedback does not alter selection geometry.

Combined local qualification passed 155/155 in 123.360 seconds
(`ead83c6f-ce86-4ba4-814f-5a0b83a73f54`): panel, provider, native ordinary/Code Copy and unchanged
Code Save regressions. After clipboard accounting and explicit empty-Code-selection feedback were
corrected, affected integration and keyboard qualification passed 97/97 in 8.006 seconds
(`93ecb488-b2fd-445b-8972-2fbb2ae6ed7f`). The final keyboard/native suite passed 6/6 in 13.076 seconds
(`d7d3bc7e-40b0-4cdf-a56e-c163f07ae79a`), including pending-request supersession, repeat, focus reset,
empty selection, refusal and retirement. Native windows receive GPUI platform keyboard input;
checked-writer seams preserve the Operator clipboard. This is not privileged clipboard mutation
or Windows SendInput qualification.

Runs used stable Cargo/nextest, explicit `.cargo/local.toml`, locked offline resolution, one job,
serial tests, debug disabled and nonincremental compilation. The final production library check
passed in 13.40 seconds with existing warnings. No manifest or dependency revision changed;
the preceding mounted-Code publication and canonical one-GPUI evidence remains applicable.
Independent review accepted the final conversion, accounting, source binding, command custody and
failure behavior. That boundary did not accept synthetic context or Table. Their native integration
belongs to separate acceptance boundaries.

Source-context and refresh lessons are retained in
[source context](../failures/transcript-copy-source-context.md) and
[resource refresh](../failures/transcript-copy-resource-refresh.md). Execution logs and positively
identified native fixture homes reached their last review consumer at acceptance. Early native
runs preceded fixture-ledger registration; their unidentified homes cannot be safely reclaimed
from shared Temp. The [fixture-ledger lesson](../failures/native-copy-fixture-ledger.md) records
that attribution limit; shared caches and unrelated temporary material remain untouched.

## Synthetic Discussion Context Readiness

Inspection on 2026-10-12 established a bounded mounting path under the existing
[context presentation](../features/branch-discussions/design.md#context-presentation) and
[presentation records](../systems/transcript-presentation/design.md#presentation-records)
contracts. This checkpoint does not accept mounted context or its native Copy integration.

`ThreadRecord::context_owner_id` identifies the exact immutable envelope owner. For the first
context draft, its matching discussion-context submission intent and selected committed tail prove
the insertion parent. After submission, a point read of the permanent owner turn's immutable
parent proves that same boundary, including when replacement has selected away from the owner.
The draft-to-submitted transition preserves the identity payload. A group keyed by discussion,
that payload and parent can therefore remain stable without a new persisted descriptor.

The exact envelope read is `SyndicStorage::context_envelope`. Its descriptor retains source thread,
turn, item, projection, revision, selected range, digest and creation time. Decoding validates range
and digest; source admission must also check agreement with the independently proven boundary.
Missing or invalid envelope data can yield unavailable context at that boundary. Missing or invalid
owner topology cannot prove a boundary and must fail explicitly rather than search visible text.

Selected text is bounded to 65,536 UTF-8 bytes; the encoded context-family ceiling is 66,048 bytes.
The existing 65,536-byte transcript attachment allowance cannot admit a maximum-sized encoded
envelope. Context requires separately bounded admission including metadata and derived Copy text,
plus live/candidate read parity and current-source fencing. Activation currently supplies only
view and projection responses; explicit context transport and local presentation provenance are
mounting prerequisites rather than canonical projection changes.

Insertion means after the entire immutable source turn, even when a bounded page ends inside that
turn. Exact topology must determine the boundary without a whole-history scan. Context chunks
then use existing realized-frame measurement, selection and anchor mechanisms. Selection eligibility
does not grant Quote, branch, edit or ordinary turn-menu eligibility; these commands need explicit
exclusion when selection is broadened to copyable context. Storage's discussion creation and
temporal-context tests supply existing topology evidence; source admission and mounted native
interaction still need qualification.

## Synthetic Discussion Context Acceptance

Accepted on 2026-10-12 after independent semantic review. Ordinary selected-thread activation
admits the exact durable context envelope and independently proven insertion parent. Bounded
indexed topology reads locate the end of the complete source turn, including a page split within
that turn. Stable discussion/owner-payload/parent identity survives the first draft's submitted
transition and later selected-path replacement. Context remains explicit local presentation with
no canonical projection or turn identity; it does not change turn counts or enter CAS input.

The selected source limit is 65,536 UTF-8 bytes; the encoded envelope point-read allowance is
66,048 bytes. Source adaptation admits separately charged original and best-effort stripped text
as chunks of at most 2,048 bytes, with at most 33 UTF-8 chunks. The existing measured frame bounds
rendering and preserves ordinary virtual extent. Group retention protects the whole admitted
envelope plus its paging boundary; authored projection fallbacks and draft-tail context retain the
same return path. Whole-group eviction releases its payload, and bounded source admission remounts
it on return. Missing or invalid envelopes produce unavailable context at the proven boundary.
An oversized authored projection can retain its exact metadata-backed fallback alongside admitted
context, rather than silently dropping the context or reading the oversized projection.

The `DISCUSSING` callout uses declared context roles through the canonical appearance schema.
Resolved fonts and colors, font-derived group geometry and heading spacing feed the same measured
and rendered composition. The finite text-role property sets expose only consumed properties.
GPUI's normal wrapping preserves source whitespace and explicit line breaks; its line wrapper
also breaks an overlong word when no earlier wrap opportunity exists. No literal spaces are added
to the heading label to simulate letter spacing.

Stable realized context permits selection and Copy. First Ctrl+C copies exact selected Markdown;
the quick second distinct press uses the accepted source-owned stripped representation. Structural
heading, provenance, unavailable labels and padding are excluded. Quote, mixed-context Quote,
branch, edit and ordinary turn-menu targets remain unavailable. Continuation chunks conservatively
preserve uncertain opening syntax under the shared Copy contract. Source, candidate and publication
fences reject stale results; retirement and disposal release admitted payload and pins.

Final local qualification passed:

- Panel/provider/native library: 167/167 in 166.127 seconds,
  `fe148bc6-4903-4d62-9530-43225c723128`, including ordinary and Code Copy, native Code Save,
  context Copy, long-context scroll-away/return, font/width remeasurement, exact source admission,
  candidate parity, page splitting and projection-capacity fallback.
- Twelve affected transcript integration binaries: 96/96 in 0.494 seconds,
  `df5a7094-92d0-482d-ad25-69dba9f73c77`, including group retention, fallback/tail boundaries,
  eviction, disposal, action exclusions and existing selection/activation behavior.
- Theme schema/document/semantic qualification: 9/9 in 0.654 seconds,
  `38852309-4e04-4258-aeab-cae8a1d5834d`.
- Storage value qualification: 4/4 in 0.023 seconds,
  `dfe9fc3e-ff31-4135-91a4-bf132b3f9e32`. Fault-enabled temporal/creation qualification passed
  twelve unchanged cases in `266a81f2-d5e2-4350-a7a5-6f04f700e66d`. Its inheritance fixture omitted
  projection of a sealed pending user input; the local correction preserves Pending and zero
  finalized items. That corrected case passed 1/1 in 9.451 seconds,
  `8770d1d6-ace8-4a04-880f-f4093dd09962`. No shared helper or production input changed, so the
  twelve successful cases and application qualification remain applicable.

Stable Cargo/nextest used explicit `.cargo/local.toml`, locked offline resolution, one build job,
serial cases, debug disabled and nonincremental compilation. The final production library check
passed in 23.70 seconds with existing warnings. Independent review checked all 28 qualified
implementation/test source hashes before accepting the local storage-fixture correction and the
complete boundary. No manifest, lockfile or owned-fork revision changed; preceding canonical
single-GPUI publication evidence remains applicable.

Native tests use GPUI keyboard/wheel dispatch and captured checked-clipboard writes. They mount a
prepared context group in the actual ordinary measured host; real durable source/candidate
admission is qualified separately. This is not Windows SendInput, privileged OS clipboard mutation,
or end-to-end discussion creation qualification. Those creation and broader branch workflows keep
their separate rework boundaries. Table mounting remains unaccepted.

The [group-residency lesson](../failures/discussion-context-group-residency.md) retains the
invalidated partial-eviction approach. Execution logs and positively registered native fixture
homes reached their last review consumer at acceptance; their exact owned cleanup does not extend
to shared caches or the unidentified fixtures documented by the preceding Copy boundary.

## Inline Table Interaction Readiness

The 2026-10-12 source inspection establishes a separate inline consumer boundary. Storage
`mutation/projection/parser/output.rs::emit_table` emits one complete Table span with ordinal one
when source bytes, body rows and columns fit the existing thresholds. The stored payload retains
`MarkdownBlockKind::Table`, block identity, exact source interval and complete bytes. Provider
`transcript_provider/source.rs::read_bounded_activation` currently erases the kind into `Text`;
`core.rs::presentation_record_for_projection` publishes `TextChunk`, and the ordinary panel paints
that source as text. No prepared cell model reaches that consumer today.

The [system decision](../systems/syndic-conversation-history/design.md#inline-table-structure-preparation)
and storage/app boundary decisions select pure Syndic preparation in the provider worker and typed
projection residency. Existing `table_syntax` and resource Table derivation supply the grammar;
the bounded preparation API is a required implementation prerequisite, not an existing capability.
No persisted format, storage threshold, structural companion or resource demand is needed.

Implementation must separate resource-specific owner/query mechanics in `table_viewport`,
`table_source_actions` and `table_save` from shared cell geometry, rectangle selection, export and
file publication. `TableExport::selected` currently refuses more than 128 cells; that resource
window limit must remain enforced for indexed inputs while inline export accommodates its complete
396-position model. Rectangles crossing typed whole-row fallback remain unavailable, preserving
the previous coherent selection; whole-table cell Copy requires all positions eligible. Raw-row
actions and whole-source Save remain the explicit source-preserving fallback paths.
`TableViewport::selection_cells` independently caps rectangles at 128 and
`whole_table_selection` depends on that admission; both need an explicit inline-owner bound rather
than a global cap increase. The model remains complete for offscreen selection, while paint uses the
smaller shared viewport window. Merely converting small Tables into indexed resources or parsing
cells in the widget would violate the selected ownership boundary.

Qualification must use genuinely inline fixtures through ordinary saved-history projection and
provider preparation, including a maximum-boundary Table and threshold-crossing replacement.
Native evidence must distinguish whole-cell selection and selected raw-source exports from text
selection, preserve empty/missing and fallback facts, exercise the first/second Copy gesture and
checked writer refusal, and verify actual resize/font geometry, scrolling, stale callbacks and
release. Save must qualify its inline source adapter under the existing native-choice and atomic
publication contract. External indexed Table and Code regressions remain required when shared
interaction or action code changes. No inline mounting acceptance or runtime verification is
claimed by this inspection.

Independent read-only review accepted readiness on 2026-10-12 after checking the complete authority
boundary against storage, provider, presentation, selection, export and Save source seams. It
confirmed the unchanged storage thresholds, projection admission, bounded paint and source-qualified
action retirement, and resolved whole-table Copy eligibility around fallback rows as documented
above. No supported-guarantee blocker remains. Documentation whitespace checking passed. This
review ran no Cargo or native tests, changed no source and created no temporary artifacts. Inline
mounting remains unaccepted until the implementation and native qualification boundary completes.

## Mounted Inline Table Acceptance

Independent completion review accepted this boundary on 2026-10-12 with no blocking finding.
Final qualification covers ordinary saved-history inline Table preparation, typed
provider/resident publication and measured Table mounting. Syndic validates the stored complete
Table span and existing grammar under unchanged 8,192-byte, 32-body-row and 12-column thresholds.
The prepared immutable source, relative cell/row ranges, alignment and missing/empty/fallback facts
remain bounded to 396 positions. Invalid structure and capacity refusal preserve typed source
authority. Retained source, model and Arc allocations are charged before publication; layout and
action reservations share existing admission ledgers without a new cache or budget expansion.

Shared owner-neutral cell and raw-row facts support geometry, selection and export. Inline owners
consume preparation directly; indexed owners retain their authenticated query/binding path and
128-descriptor cap. Paint remains bounded to fifteen body rows, one header and eight columns.
Complete inline source residency permits offscreen selection and Copy without resource demand.
Same-source resize/font changes preserve inline selection and focus. Source replacement,
inline-to-resource replacement, outer eviction and view retirement close obsolete interaction and
action ownership while preserving the outer block anchor where applicable.

Native qualification covers cell click, Shift-click, drag and keyboard rectangles; both distinct
Copy gestures and held-repeat rejection; checked writer refusal; exact selected Markdown and
complete CRLF source; empty/missing cells and stripped omission; maximum 396-cell offscreen Copy;
fallback-crossing refusal and raw-row Copy/Save; two-axis wheel routing; actual resize/font
remeasurement; confirmed replacement and disposal. Inline Save consumes immutable source through
the shared cancellation, native-choice and atomic publication boundary. Source replacement and
allocation tests complement native evidence; an explicit obsolete-release integration test proves
model/source expiry, zero layout charge and rejected obsolete ownership after outer eviction.

Final checks passed with stable Cargo, local LLVM configuration, locked offline dependencies,
one build/test job, debug information zero and nonincremental builds:

- Storage preparation/indexed derivation: 136/136 in 66.580 seconds,
  `75362990-d9ed-42b5-a09e-0b55b6549f5b`.
- Ordinary producer/projection boundaries: 29/29 in 160.395 seconds,
  `f9c6ea14-d84d-4cb2-90bf-1329ca07637c`.
- Combined app/core/provider/native: 231/231 in 464.469 seconds,
  `bf1c4398-6540-435e-a087-a4a76e62e8dc`, including all five inline native cases and 37 native cases total.
- Twelve transcript integration binaries: 131/131 in 0.761 seconds,
  `f057358c-f579-475d-b7d7-b7f39acffe29`.
- Four state theme binaries: 10/10 in 0.304 seconds,
  `6cbc1269-7155-412b-8de6-4b72e3413e99`.
- Production `beryl-app --lib` check without test-fault features: exit zero in 22.15 seconds.

Thirty-eight shared changed Rust inputs remained identical across final runs; the additional
outer-eviction integration test was separately frozen and passed. No manifest, lockfile, storage
schema or owned dependency fork changed. Earlier development failures are superseded by this
final input set; the [owner-reservation lesson](../failures/inline-table-action-owner.md) records
the significant correction.

Native evidence uses real GPUI windows, routed test input, genuine saved-history producer/provider
fixtures, measured geometry, checked clipboard/picker seams and filesystem publication. It does
not qualify OS clipboard or common-dialog automation, or the complete user composer workflow.
Remaining transcript contributions and diagnostic-child activation keep their separate gates.
All seventy positively attributed native fixture/output directories are verified absent. Reviewed
task execution artifacts have reached their final consumer and are reclaimed; shared caches and
unrelated temporary resources remain outside this cleanup boundary.

## Mounted Indexed Table Acceptance

Independent completion review accepted the external indexed Table panel on 2026-10-12. Header,
Body, Source and anchor requests serialize through the existing indexed owner; the widget neither
loads storage nor reparses Markdown structure. A coherent window covers at most fifteen body rows,
one header row and eight columns, with 128 descriptors and response pages. Current and replacement
windows retain at most 256 descriptors under the existing resource budget. Shared response
allocations count once; distinct windows and copied viewport state remain charged. Final promotion
keeps the former window charged until measured publication and exact viewport adoption acknowledge
release. Table Header/Body requests use a 16,384-byte allowance under the unchanged query ledger;
Code retains its separate sixteen-page continuation limit.

Native qualification proves two-axis nested wheel routing over body, column headers and command
header without outer co-scroll; whole-cell click, Shift-click, drag and keyboard rectangles;
resident-gap preservation/demand; stable focus; actual width/font remeasurement; bounded replacement
and disposal. Ordinary navigation skips known fallback rows, while an unrealizable rectangular
extension preserves its last coherent selection. Paint callbacks reject obsolete owner/source
identities, and truncated cell tooltips use bounded owner-projected fragments.

First distinct `Ctrl+C` exports selected raw cell spelling and whitespace as Markdown; a quick
second distinct press exports best-effort stripped text. Held repeats do not advance the gesture.
Partial exports include only selected cells and required separators, with selected-column delimiter
syntax for selected headers. Selecting every table cell in a coherent rectangle with no fallback
row reproduces the complete original
source, including CRLF, delimiter spelling, empty rows and cells. Text exports omit empty rows and
trailing empty/missing cells while preserving interior gaps and literal code. The shared conservative
scanner retains ambiguous markup. Pending first-Copy supersession, checked-writer refusal and
selection preservation are qualified alongside existing global Code/Table supersession.

Typed surplus-row fallback has explicit exact raw-row Copy/Save actions. Native complete-selection
Copy refuses source above the clipboard limit before writer invocation and preserves its rectangle;
Save streams the full source independently of that limit. Table and Code share the original-root
Save implementation, native replacement-choice custody, atomic publication and cancellation/release
fences. Checked fault injection preserves the destination and removes staging output, with explicit
writer-admission and failure-feedback evidence. All affected Code behavior remains qualified.

Final frozen-input qualification passed:

- Combined panel/core/provider/native: 177/177 in 397.502 seconds,
  `081c1e94-09d8-4cd0-9ef5-2bada8a9aef5`, including all six Table native cases and 32 native cases total.
- Twelve affected transcript integration binaries: 130/130 in 0.691 seconds,
  `b189b2d0-7cc6-41b2-b3f0-8452fde5e933`.
- Four theme binaries: 10/10 in 0.304 seconds,
  `34d0e9c8-0761-4951-942b-7a955fc0ba89`.
- Production `beryl-app --lib` check without test-fault features: passed in 13.90 seconds.

Independent review reconciled 112 shared source/authority inputs across these runs with no drift;
the final theme/production manifests contained 146 identical inputs. Verification used the stable
toolchain, local LLVM configuration, locked offline dependencies, one build/test job, no normal
debug information and nonincremental builds. No dependency or fork revision changed.

Native evidence uses actual GPUI windows and routed test input, checked clipboard/picker seams,
measured geometry and real filesystem publication. It does not qualify OS clipboard/common-dialog
automation or end-to-end Table creation. Existing storage/projection classification and preview
limits remain separate contracts. Small inline Tables retain a separate cell-interaction readiness
and mounting boundary; this acceptance does not complete uniform Table interaction.

The [window-admission lesson](../failures/indexed-table-window-admission.md) and
[fixture lesson](../failures/native-table-fixture-construction.md) preserve the invalidated approaches.
Temporary observers are removed. Retained execution evidence and positively attributed native homes
have reached their last review consumer and are reclaimed at this boundary; unrelated shared caches
and the preceding Copy boundary's unidentified fixtures remain outside cleanup ownership.

## Mounted Narrative Acceptance

Independent complete-boundary review accepted mounted narrative presentation and character selection
on 2026-10-13. Ordinary saved Markdown publishes immutable Pending source facts before tracked
preparation over already admitted source. Typed prepared adoption preserves exact block/span identity,
raw source and provenance; checked refusal retains its source. Live growth keeps the prior coherent
presentation until replacement publishes, and unchanged terminal source reuses its prepared model.
Preparation adds no storage schema, history route or producer classification.

Projected paragraphs, headings, quotes, lists, thematic breaks and supported inline formatting use
resolved typography. Pure source/display maps preserve complete selected construct syntax and exact
spelling; partial selection invents no delimiters. Unsupported syntax, explicit fallback and later
continuation spans retain their qualified literal treatment. Source/display size, run, depth and
fragment limits remain bounded; measured prepared fragments share the existing 32-observation frame
allowance and layout/resource ledger. Crossed layouts and obsolete candidate reservations release
without retaining offscreen geometry.

Natural measurement, paint, highlight and pointer hits use the same current shaped geometry. Actual
body bounds exclude padding, markers and context chrome. Drag, reversed ranges and Shift extension
map character endpoints with indivisible transformed units; unrealized or nested Code/Table crossings
preserve the last coherent selection. Font, viewport, source, activation and disposal invalidation
close obsolete selection and Copy. Prepared Code/Table owners retain their separate interactions.

The first distinct `Ctrl+C` copies selected original Markdown; a quick second distinct press copies
best-effort stripped text. Held repeats do not advance the gesture. Mapped content consumes exact
prepared display facts; literal selections use one bounded off-GUI conversion slot with current
source/selection/focus/geometry election. Supersession, checked-writer refusal and cancellation retain
the previous clipboard. Synthetic context uses ordinary mapped text selection and both Copy methods
while remaining ineligible for Quote, branch and transcript menus.

Native evidence uses genuine saved producer/provider fixtures, actual GPUI windows, routed pointer
and keyboard input, measured geometry and checked clipboard seams. It covers Unicode/CRLF/escaping,
partial and complete formatting, multiple coherent spans, literal continuation, maximum capacity
refusal, source growth/completion, font/width changes, long ordered markers, focus/repeat behavior,
adjacent paging and exact pre/post extension anchors, cancelled-worker drain and retirement. A bounded
test-only observer fences the actual admitted adjacent task before publication; it does not inject
demand or selection. Affected indexed/inline Code/Table Copy, Save and context regressions pass.

Final qualified checks used stable Cargo, local LLVM configuration, locked offline dependencies,
one build/test job, zero normal debug information and nonincremental builds:

- Combined app/core/provider: 256/256 in 543.361 seconds, including 48 native transcript viewport
  cases; `97afdd0c-040f-44e3-8dcf-2b071694428f`. The 1,244 outside-filter cases were not executed.
- Eighteen transcript integration binaries: 153/153 in 0.721 seconds;
  `d64b73ea-e941-4722-8af2-c77b1c6521c3`.
- Five state theme binaries: 13/13 in 0.337 seconds;
  `896e9041-75f9-4730-b99f-5e12cf5d52bc`.
- Storage narrative preparation: 13/13 in 2.009 seconds;
  `3f11ac75-f4e4-4a93-a860-cb5a1b99c61e`.
- Focused adjacent/Code drain corrections: 2/2 in 70.849 seconds;
  `819a3838-1eae-48ce-92dd-d4c9d40c0cb7`.
- Production app library check without test-fault features: exit zero in 13.04 seconds.

The reviewer independently rehashed all 58 frozen changed Rust inputs with zero drift. Unaffected
storage, theme and transcript-core evidence remains valid across the final host lifecycle correction;
combined and production checks ran on that correction. No manifest, lockfile or owned fork changed.
The [health-election](../failures/transcript-narrative-publication-election.md),
[cached presentation](../failures/transcript-narrative-cached-presentation.md),
[adjacent demand](../failures/transcript-narrative-adjacent-demand.md) and
[poll admission](../failures/transcript-poll-admission-after-sync.md) records preserve durable lessons.

This acceptance does not qualify OS clipboard/common-dialog automation, complete composer workflows,
diagnostic-child activation or the remaining transcript commands. Temporary diagnostics are removed.
Reviewed execution artifacts are reclaimed: the exact owned task root and all 245 unique native
fixture ledger paths are verified absent, with no task processes or reparse points. Unrelated shared
caches and Operator-owned skill edits remain outside that cleanup.

## Remaining Transcript Command Readiness

Read-only qualification on 2026-10-13 uses accepted source commit `b71e0cad8420cd81084c899dcd962d82ff3eceb8`.
No command is mounted or accepted by this record. The controlling product contracts are
[transcript selection and menus](../features/transcript/design.md#selection-copy-quote-and-menus),
[transcript command composition](../features/transcript/gui.md),
[composer Quote insertion](../features/composer/design.md),
[conversation replacement editing](../features/conversation-threads/design.md), and
[branch creation](../features/branch-discussions/design.md#discuss-in-new-branch).
[Branch source authentication](../systems/branch-discussion-handoff/design.md) and
[renderer selection](../systems/transcript-presentation/renderer-architecture.md#selection-and-menus)
own their internal boundaries. These authorities determine the missing connections; the existing
whole-record helpers do not define character-selection behavior.

### Mapped Source And Existing Targets

`panel/narrative.rs::NarrativeSelection` binds directed character endpoints to activation and
presentation revisions. Prepared and literal `MappedGeometry::source_range` maps a display range
to offsets local to its admitted source span. `panel/narrative/copy.rs::narrative_copy_text(false)`
reconstructs exact selected Markdown, with a 65,536-byte contiguous representation limit.
`narrative_copy_capture` instead captures mapped display or literal source parts for stripped Copy;
it is not a historical command request. Its scalar source/layout bindings and current-election
checks provide reusable invalidation mechanics, not branch eligibility.

The provider in `crates/beryl-app/src/transcript_provider/source.rs` retains projection source ranges
in Syndic provenance and Copy ranges; `core.rs::presentation_record_for_projection` transports them
unchanged. An exact historical command adapter must checked-add local mapped offsets to the admitted
source-range start, check containment and UTF-8 boundaries, and preserve selected bytes, record,
projection, revision, activation and current geometry. It must not interpret a local offset as an
absolute historical offset or replace the selected range with a whole-record range.

The old Quote path (`selection.rs::resident_selected_record`,
`renderer_quote.rs::resident_quote_command_for_realized_record_ids`, core target validation and
`quote_payload_for_target`) consumes whole `ResidentSelectedRecord::copy_source_range` values.
Its record admission accepts `TextChunk`; current prepared `NarrativeReady` does not qualify.
`panel.rs::apply_realized_quote_target_for_record_ids` has no native character-selection action
consumer. Reusing that path unchanged would expand selection and omit prepared narrative.

`context_menu.rs::resident_context_menu_record` accepts nonempty `TextChunk` and resource references
with exact Syndic turn/item/projection/revision provenance. It likewise has no prepared narrative
admission. `renderer_context_menu.rs` supplies realized-record validation and target invalidation.
`command.rs` action provenance and Edit/Branch wrappers retain target identity and source ranges,
but do not establish user-input versus assistant ownership, selected-path eligibility, selected
character bytes or proven-terminal status. These wrappers are reusable transport, not permission.
Synthetic discussion context, unavailable labels, pending state and noncontent remain excluded
from historical command targets even when context text is selectable and copyable.

### Quote Consumer

The composer has ordinary segmented mutation mechanics:
`main_window/conversation_composer_owner/dispatch.rs::MainWindowConversationComposer::pump_one`
drains text-input requests into `composer_host/mutation/admission/begin.rs` and
`composer_host/mutation/execution.rs`. There is no transcript Quote insertion consumer.
Mounting Quote must capture the saved insertion position and route exact selected eligible text
through these shared edit, settlement and undo semantics. It must preserve the system clipboard
and transcript focus, and reject obsolete source or geometry. Clipboard paste is not a substitute.
Quote's eligibility must follow its own authority; branch's proven-terminal restriction must not
silently become a Quote restriction.

### Historical Menus And Replacement Editing

Menu targeting requires current prepared narrative admission, genuine pointer/keyboard invocation,
exact target provenance and owner-supplied rows through the canonical menu widgets. Empty space,
status chrome, synthetic context and obsolete geometry supply no target. Source/activation/geometry
changes close the menu instead of retargeting it.

`crates/syndic-storage/src/record/history.rs::ReplacementEditIntent` and mutation admission already
validate exact target, selected path and transcript-entry proofs. There is no app consumer of that
intent or mounted start/cancel replacement-edit workflow. Exact user-turn eligibility, explainable
disabled gates, draft loading, targeted/later-turn dimming, Escape and replacement submission need
their own workflow acceptance before an enabled `Edit message` row can claim that behavior.
Menu admission alone does not accept replacement editing.

The 2026-10-13 continuation checked that statement against root `6c474e72` and found a
durable prerequisite. Replacement admission validates parentage and selected-path/transcript-entry
proofs, but no storage consumer starts or cancels replacement intent. The declared sealed
`ComposerV1` import is also absent. `draft_piece/model.rs::DraftPieceBuildRecordV1` and
`draft_piece/codec.rs::encode_build` support only mapped edit builds;
`DraftPieceSettlementClosureV1` commits only ordinary edit adoption, and
`DraftEditHistoryFrontierKeyV1` has only canonical-empty, session and publication variants.
An ordinary edit cannot stand in for import: the controlling storage design requires the imported
root, fresh-baseline history and durable intent to publish together, with Undo and Redo unavailable.
Historical image-reference ownership must remain immutable while the draft receives its
copy-on-write owner head.

The sole V7 schema authority already declared import builds and settlements, but their exact
discriminators and canonical layouts were incomplete. Existing
length-framed command-header bytes and a new closed build-frontier variant provide a route to
explicit import discrimination without changing ordinary V6 record bytes or inferring import
from an absent coordinate map. The
[canonical extension](../../crates/syndic-storage/doc/design-schema-v7.md#sealed-import-canonical-extension),
[storage boundary](../../crates/syndic-storage/doc/design-draft-storage.md#sealed-input-import-and-replacement-intent)
and [historical image proof flow](../systems/image-assets/design.md#reference-and-label-evidence)
now define the missing capability. Independent adversarial readiness review identified and resolved
the bounded marker-to-Asset evidence transport, canonical frontier hash and final session-generation
rules. Implementation readiness is accepted; ordinary frozen-byte vectors, bounded runtime
continuation, atomic root/history/intent and Asset publication, and reconciliation remain
implementation acceptance obligations. The durable prerequisite precedes the mounted replacement
workflow; Edit stays disabled.

Final Asset selection also requires exact unchanged historical-head and marker-free absence
facts in the original command's reconciliation descriptor. Existing validation-only assertions
omit them, while existing equal-old/new classification rejects them. The independently reviewed
[exact-unchanged contract](../../crates/beryl-home-store/doc/design-atomic-commands.md#reconciliation-and-operation-health)
resolves that shared prerequisite without a historical mutation or persisted-format change.
Its separate runtime and semantic acceptance precedes final replacement selection; see the
[invalidated assertion reuse](../failures/asset-head-assertion-reconciliation.md).

### Exact Unchanged Reconciliation Acceptance

Accepted on 2026-10-13 against the exact-unchanged HomeStore/system and State hook authorities.
Matching descriptor-listed equal old/new natural-record facts constrain both possible sides.
HomeStore selects ordinary exact old/new only when every domain supports that side and at least
one differentiating domain proves it. All-unchanged, mismatched and mixed-side composition remains
collision; empty contributions retain their stronger pre-writer refusal. Unchanged source,
witness and passive roles cannot qualify first-acceptance successor reconciliation. No persisted
family/version, public operation outcome or historical owner revision changes.

Qualified 135 distinct cases through serial, explicit Cargo-nextest targets: 49 focused cases
across `unchanged_reconciliation`, `asset_reconciliation_constraints`, `reconciliation_constraints`
and `catalog`; 86 dependent cases across `targeted_reconciliation`, `successor_reconciliation`,
`command_outcomes`, `recovery`, `commit_receipts`, `assets_v3`, `asset_owner_validation`, `settings`,
`runtime_root`, `session` and `durable_job`. None were skipped. Actual Asset tests retain byte-equal
image-backed heads and marker-free absent heads through original receipt reconstruction, exact
old/new acknowledgement loss, revision advancement, mismatch collision and scoped disposal.

Final production validation passed for HomeStore, State, Syndic and app libraries with stable
Cargo, local resolved dependencies, locked/offline mode, one build job, zero normal debug and
nonincremental compilation. Independent complete semantic/adversarial review accepted original
receipt/custody, bounds, error provenance, unchanged constraints and successor restrictions with
no blocking finding. All 28 registered task-owned fixtures are absent and validation processes
have exited. This accepts the shared capability; historical replacement import/selection and the
mounted Edit workflow retain their separate acceptance gates.

### Branch Creation Consumer

`crates/syndic-storage/src/discussion_source.rs::prepare_discussion_source` authenticates a
`DiscussionContextSource`, `SelectedPathProof` and `CurrentTranscriptEntryProof`. Its `authenticate`
checks current selected path/transcript generation, exact entry/item/projection membership and
revision, assistant-message ownership, proven-terminal turn lifecycle and finalized item coverage.
Preparation reads the selected absolute source range and compares exact bytes and byte length;
command validation authenticates again. A rendered record or successful Copy is insufficient.

The existing source contract binds one projection owner and one contained source interval.
Capture may combine mapped fragments only when they prove that same owner and contiguous interval.
Cross-projection or mixed-owner selections remain branch-ineligible; no concatenation, stripped
representation or similar-text lookup may fabricate a valid source. The immutable envelope keeps
the exact selected bytes, historical provenance and SHA-256 digest without normalization.

`crates/beryl-app/src/discussion_creation.rs::DiscussionCreationService::prepare` consumes the
authenticated source, `CreateDiscussion` and cancellation. It prepares durable discussion creation
and initial catalog publication in one home command with exact runtime/root and claim checks;
`discussion_creation/recovery.rs` retains outcome reconciliation. No main-window consumer mounts
this workflow. Mounting needs off-GUI proof acquisition and exact captured request custody,
65,536-byte selected-text admission, busy/reconciling/collision states, preserved source selection,
and proven success followed by ordinary activation. Activation Retry must target the already-created
discussion and never repeat creation. Durable creation and activation remain different elections.

### Derived Acceptance And Evidence Limits

The next outcomes are saved-position Quote insertion from real mapped selection, transcript menu
mounting, exact historical replacement editing, and captured branch creation with ordinary activation.
Menus keep Edit unavailable until the exact menu-origin replacement workflow is accepted.
Each needs native invocation and its owning workflow evidence; reusable service tests alone cannot
accept the mount. Supporting capture and eligibility adapters belong to the command they enable,
with reusable facts shared where their contracts agree. No new history route, schema or producer
classification is needed for this readiness conclusion.

Readiness verification inspected the exact mapped Copy bodies, presentation provenance transport,
Quote and menu admission, branch source authentication, durable creation/reconciliation, and composer
mutation entry points. Existing narrative acceptance remains evidence for mapped pointer selection,
Copy and lifecycle only; no tests were rerun for this documentation-only qualification. Independent
complete-boundary semantic review accepted the source claims and derived boundaries with no remaining
blocking findings. Review corrected menu-before-edit ordering before acceptance. Diagnostic-child
activation and complete transcript workflow remain separate pending evidence boundaries.

## Mounted Mapped Quote Acceptance

The ordinary transcript now exposes a canonical Quote menu for exact current mapped authored
selection. Pointer invocation requires a hit inside that selection; keyboard invocation requires
direct transcript focus and anchors at its mapped head. Nested selection owners and synthetic
discussion context cannot reuse an outer Quote capture. The menu contributes one bounded row;
turn menus, replacement editing and branch creation retain their separate acceptance boundaries.

Host harvesting validates activation, presentation, realized consecutive records, exact Syndic
provenance, checked historical range containment, UTF-8 boundaries and the 65,536-byte aggregate
limit. It copies selected Markdown bytes from the admitted source, including complete selected
formatting syntax, partial constructs, Unicode and authored CRLF. It neither expands to whole
records nor harvests pending, fallback, missing or synthetic content. Quote has no branch-terminal
restriction.

The shell binds the capture to its current transcript and composer claim. The widget inserts
at the remembered directed selection head using a collapsed source range, preserving selected
draft content and inline-object gaps. Ordinary immutable evidence, paged staging, commit and
Undo/Redo own the edit. Quote preserves transcript focus and the clipboard. Weak source observation
and a shared cancellation token reject obsolete captures before edit admission; coherent candidate
staging cancels synchronously rather than relying only on deferred observers. Current geometry,
source, activation and selection changes close the menu and invalidate its capture. Source changes
cannot redirect an in-flight request to a new selection or composer.

Native qualification exposed and corrected two bounded prerequisites. Private history ownership
now fits the fixed Windows worker stack without changing durable encoding or authentication;
the [worker-stack lesson](../failures/composer-history-worker-stack.md) records the evidence.
Range geometry recognizes exact CRLF delimiters across pages, including trailing CRLF and the
one-byte text-segment limit; the [geometry lesson](../failures/range-editor-crlf-geometry.md) records
why correct draft bytes and quiescence alone did not prove a usable successor surface. The widget
source is published at `1366c97b0e56c63fee12e0bd0dddb888bb8bad14`, pinned in the root manifest and
canonical lockfile. The earlier fixed aggregate-byte test assertion was removed; measured exact
capacity admission, one-byte-under refusal and shared-backing tests remain intact.

Verification used stable Cargo, the existing local dependency overrides, LLVM linking, one build
job, disabled ordinary debug/incremental output, serial nextest and explicit Cargo targets.
App tests enabled `test-faults`; the final production check used no test features:

- Combined app library: 258/258 in 557.163 seconds, including 50 native viewport cases;
  `08380955-0e90-40fc-af32-19844cbcb59d`. The filter covered transcript core/panel, transcript
  provider, ordinary transcript viewport and Running threads. Genuine native Quote covered
  formatted and reverse partial Unicode selection, saved-position insertion, exact CRLF,
  pointer/keyboard actions, Undo/Redo, unchanged focus/clipboard and pre-admission geometry
  cancellation. Context refusal, font remeasurement and same-record source growth also passed.
- Focused composer integration: 118/118 in 218.880 seconds;
  `8f9805c3-7fae-4fc0-a61b-3db215347ce1`. Targets were `conversation_composer_gpui`,
  `main_window_composer_mount`, `composer_lifecycle`, `syndic_composer_mutations` and
  `syndic_composer_history`, including ordinary edits, cancellation, late settlement, recovery,
  marker admission and history behavior.
- Mapped-source and storage history: 66/66 in 118.979 seconds;
  `1ce9564c-ad09-451d-8d98-c7769f32fee0`. Targets were `syndic_transcript_selection_copy`,
  `historical_root_adoption`, `draft_edit_history`, `draft_edit_history_retention` and
  `draft_marker_historical_adoption`. All seven mapped Quote cases and actual prepared history
  commands on a one-MiB worker passed. Relevant source and configuration remained unchanged
  through the later app/widget corrections.
- Complete widget `range_widget` target: 132/132 in 4.699 seconds;
  `2d0d257f-3e77-451f-882b-a9b64f78d864`, including exact insertion/evidence replay,
  object gaps, capacity/read-only refusal and all three CRLF geometry regressions.
- Locked canonical metadata and final no-feature production `beryl-app --lib` check passed
  after the pin update; the language server restarted successfully.

Independent complete-boundary semantic and adversarial review accepted the implementation and raw
verification evidence with no remaining blocker. The review covered source and claim substitution,
pre-admission cancellation, custody, bounded traversal/allocation, storage authentication and
ordinary atomic edit settlement. All 96 positively registered native fixture and output directories
are verified absent. Complete mounted transcript workflow acceptance remains pending; this outcome
qualifies mapped Quote only.
Reviewed execution artifacts have also been reclaimed; no task-owned process or session mutation
remains.

## Mounted Transcript Turn Menu Acceptance

The ordinary transcript overlay now mounts canonical pointer and keyboard turn menus for current
realized user-input text, including prepared narrative. Current resident view provenance determines
user ownership; transport identities alone grant no editing eligibility. A collapsed caret or an
unselected pointer target exposes disabled `Edit message`; a mapped selection can retain Quote
beside it. The static collection has at most two rows. Edit explains that historical message
editing is unavailable and has no editing or selected-path mutation dispatch.

Captures retain exact activation, presentation, source, layout, selection and frame facts. Source
replacement, remeasurement, selection changes, activation, realization loss and retirement close
obsolete targets. Keyboard record fallback validates the old target before extracting its identity.
Synthetic context, pending/unavailable preparation, empty content and missing stable provenance
remain ineligible. Existing mapped Quote source bytes, insertion cancellation and nested-owner
routing remain qualified.

Native verification exposed two pointer-focus prerequisites. A disabled menu row allowed its
click to reach the transcript underneath, preventing Escape dismissal; the canonical menu now
retains popup focus and stops inside left-click propagation. A passive model readout allowed
GPUI's tracked ancestor default to take focus; the existing default-prevention handler now applies
to passive segments too, with enabled-only command dispatch preserved. The
[durable lesson](../failures/passive-pointer-focus.md) records the mechanism and native fixture
requirements. Final native tests preserve exact focus, actual disabled pointer/keyboard refusal,
Escape return, enabled activation, and pending Retry duplicate suppression. Disabled tooltip
coverage credits asserted reason data and direct canonical tooltip composition, without claiming
a new native tooltip-paint assertion.

Acceptance qualifies **316 distinct cases** on the final implementation:

- Combined app library run `7bc68dbf-b3a2-42e1-9739-ea6099e16f07` ran 280 cases in
  594.343 seconds: 278 passed and two model-consumer fixtures failed. Reuse is limited to the
  260 non-menu-consumer cases whose relevant behavior, tests, dependencies and configuration
  remained unchanged. These include current transcript, provider, Running threads, Code/Table,
  Quote and source/geometry lifecycle coverage. This run is not reported as wholly passing.
- Final affected menu consumers: **20/20**, 30.533 seconds,
  `da12abf2-1adc-4899-838c-790978736d04`. All eight native model cases, eight model/menu-row
  cases and four canonical-menu cases passed after passive-focus and test setup corrections.
  Fixtures use current painted geometry and eligible native focus, draw outside root entity
  borrows, and exercise permitted manual Retry after settled failure without runtime automatic
  retry or relaxed duplicate-request assertions.
- Complete renderer and selection-copy targets: **36/36** (13 renderer, 23 selection-copy),
  from `4ac9d024-c1d0-4f0d-92d2-0b7d9ae9d529`. That combined run recorded 53/56, with three
  subsequently corrected model-native fixture failures; only the unchanged core integrations
  are reused. Prepared admission, wrong owner/source/view/position, stale frame/geometry,
  synthetic/fallback refusal and mapped Quote boundaries passed.
- Focused native turn/Quote/context and canonical menu run: **10/10**, 16.317 seconds,
  `6a58ff3a-7102-44f3-9f02-a8a81aa3ea0f`. These are included in the distinct-case total,
  not counted again.
- Final no-feature production `beryl-app --lib` check passed in 13.82 seconds. No Cargo model
  change or dependency update was required.

Commands used stable Cargo with `.cargo/local.toml`, locked offline dependencies, one build job,
one test thread, LLVM linking, no normal dev/test debug information, no incremental compilation
and process-local `RUST_MIN_STACK=67108864`. Library native runs enabled `test-faults`.
The combined library expression covered `syndic_transcript`, transcript-provider tests, transcript
viewport, Running threads shell, anchored menus and the model-menu consumers. Integration targets
were `syndic_transcript_renderer_selection` and `syndic_transcript_selection_copy`.
Inputs derive from Beryl `3442af0598ede2dc3562494a418eee809d120142`, unchanged widget fork
`1366c97b0e56c63fee12e0bd0dddb888bb8bad14` and GPUI fork
`14d9622001ecbfa0db4a5530b989b649690472a1`, with the accepted scoped source changes.

Independent complete-boundary semantic review accepted the final implementation and raw evidence
with no remaining blocker. Replacement editing, branch creation, diagnostic child activation and
complete transcript workflow acceptance retain their separate gates.
All 125 positively registered native fixture/output directories are verified absent. Reviewed
execution artifacts are reclaimed; no task-owned process or session mutation remains.
