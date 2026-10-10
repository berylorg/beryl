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
