# Activity Panel Mounting Readiness

## Accepted Mounting And Historical Preparation

The selected-thread widget and native interaction boundary are now independently accepted;
[mounting evidence and coverage limits](activity-panel-mounting.md). The source inventory below
records the earlier readiness assessment and does not describe the current implementation state.

The persistence boundary was implemented and independently accepted in 666cff4f;
[evidence](activity-panel-persistence.md). Exact addition/deletion totals require source traversal,
but that traversal can span bounded, cancellable preparation quanta with bounded retained state.
The subsequent claim that this requires new durable summary facts is withdrawn;
[corrected lesson](../failures/activity-display-source-readiness.md). A resumable typed source
extractor and selected-window preparation are now independently accepted;
[reader/display evidence](activity-panel-pages.md).

## Scope

The next unfinished Beryl-home mounting slice is selected-thread Activity. Catalog/search,
lineage, navigation, model, transcript and restoration retain their accepted owning evidence.
The [Activity feature](../features/activity-panel/design.md), [GUI composition](../features/activity-panel/gui.md)
and [widget contract](../gui/widgets/activity-panel/spec.md) already select Auto visibility,
runtime-period ownership, fixed-height paged virtualization, resize persistence and exact Retry.
This audit supplies source-backed implementation readiness, not mounted runtime acceptance.

## Existing Inputs

- `activity_service/collection.rs` supplies exact-period initial requests, one revision-bound head
  with total logical row count, bounded pages and opaque forward continuations. Initial `open`
  failure preserves the original request; page reads borrow their continuation. These permit
  same-scope explicit Retry without manufacturing fresh authority.
- `RuntimeActivityObservation::with_current` uses nonblocking runtime/period publication fences.
  Busy invokes no consumer callback; retirement invalidates observations. Prepared results may be
  retained for later publication, with their existing capacity ownership.
- `ActivityQueryHeadRecord` supplies counts and revision. `ActivityQueryEntryRecord` supplies exact
  source/event/item identity, kind, lifecycle and optional compact handoff byte counts. It supplies
  no prepared display labels. Source updates invalidate old cursor/head revisions.
- Main-window integration already declares the Activity slot. `MainWindowShellRoot` has no Activity
  contribution, and `widgets.rs` has no Activity widget. Existing shell layout constants and the
  old `ToolActivityProjection` do not implement the current paged widget contract.

## Persistence Boundary

`SessionWindowRecord` contains placement, target, selection and revision, but no Activity height.
Session V1 is655 bytes, and its registered family declaration is exact. Adding a family to that
existing declaration or silently changing the encoding would reject current homes.

HomeStore registration explicitly creates a previously absent named domain and reopens existing
domains against their unchanged declarations. Use a separate State-owned window-presentation
domain, with optional finite-positive logical-pixel height and exact fixed-size encoding.
Absence uses the widget's144px default. Preserve Session V1 and existing home data; no migration,
dual reader, app-wide setting or native-placement overload is needed.

Existing `HomeCommand::add` and `add_validation` compose typed participants atomically. State owns
prepared bundles validating the exact Running session/window plus original presentation presence
before writes. Removal/abandonment delete presentation state in the same command and guard absence.
Captured removal evidence carries the optional original height/revision; cancelled-close recovery
restores it atomically with renewed revisions. Restore-set/Exit/thread switching retain the same
window preference, and successful removal releases it. Do not retain orphan preference records.
These contracts belong to [State](../../crates/beryl-state/doc/design-runtime-session.md#window-local-presentation)
and the [storage system](../systems/beryl-home-storage/design.md#keyspace-ownership).

## Reader And Viewport Boundary

The existing reader cannot seek by ordinal. Use a cancellable off-GUI walk over its exact cursors,
at most one bounded page per quantum, at most eight cursor checkpoints, fixed resident pages and
one pending demand. Distant/backward jumps may restart from an eligible checkpoint or the start.
Retain full logical extent and honest missing-page state; no latency-independent seek or new
ranked durable index is required. Do not accumulate a page directory or complete row vector.

Prepare bounded display values from exact canonical/source ranges and admitted normalized metadata
off GPUI. Optional nickname/model completion remains period-qualified and does not resume a thread.
The legacy whole-collection activity projection is not a mounting input.

The window adapter adds weak graph lifetime, exact window/selected-thread ownership and a local
demand identity to runtime-period/query authority. Auto eligibility comes from accepted-parent
and compaction facts, never from row count or query success. Preserve failure feedback and exact
Retry; stale query revisions require a replacement collection rather than repeated old-cursor
Retry. A new period requires coherent fresh rows before interaction resumes. Pure rendering does
no storage, backend or metadata work.

The widget realizes fixed28px rows with four-row overscan before/after the viewport, stable row
identity, stable viewport focus, bounded tooltip lifetime, keyboard/wheel boundary propagation and
one resize capture. Native minimum/layout composition preserves pinned lower chrome. Diagnostics
remain bounded and content-free. Owning [App adapter rules](../../crates/beryl-app/doc/design-feature-adapters.md#activity-status-notices-and-audio)
record these mechanics.

## Acceptance Boundaries

Persistence is a separate state-integrity boundary: verify additive registration/reopen, finite
scalar/codec refusal, exact source rejection, independent windows, deletion/abandonment, original
cancelled-close restoration, indeterminate reconciliation and unchanged Session V1/claims/placement.

Reader preparation verifies exact publication, bounded cursor walking and cancellation, display
projection provenance, initial/page Retry, query replacement, capacity and original retirement.
GUI mounting then proves Auto lifecycle, native composition, resize persistence, virtualization,
focus/scroll/tooltips, period replacement, stale delivery and disposal through the actual shell.
Independent review covers the consequential boundaries. None of these implementation results is
claimed by this readiness audit.

## Readiness Review

Independent semantic review accepted this boundary on2026-10-18 after the removed-window evidence
bound was reconciled with the optional presentation record and its recovery successor. No material
Operator choice remains. Current production admission allows four collections and four raw pages,
with64 rows/256KiB per page; concrete retention and cancellation must respect those shared bounds.
Retain collection ownership while its checkpoints or Retry remain eligible, distinguish failure
of an initial open from failure of page zero after successful open, discard all checkpoints on
query replacement, and never use period-only publication validation as a fresh-head observation.
All29 local documentation link targets and whitespace checks passed. No production code or runtime
acceptance is claimed; the following phases supply their own owning evidence.
