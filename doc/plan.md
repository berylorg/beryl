# Scope

Resume Checkpoint 4 of the Beryl-home architectural rework tracked by
`doc/rework/beryl-home/REWORK.md`. Protect concrete supported-envelope consequences without
duplicating dependency guarantees, runtime validation, or review machinery.

The replacement shell is the final target-state composition boundary, not a compatibility shell or
a reduced copy of the archived workspace-era view. It ultimately mounts every declared main-window
slot and feature contribution, including theme roles, toolbar and lineage, transcript and its owned
scrolling, optional activity and discussion surfaces, composer, status line, overlays, notices, and
Settings entry. Reuse accepted live target-state services, hosts, projections, and widgets; keep
unimplemented mounts visibly absent or unavailable until their owning bounded phase completes.

Propagate the accepted owned-GPUI first-publication revision through the dependency order
`gpui-scrollbar` to `gpui-text-input` to `gpui-settings-window`, then canonically pin the resulting
single GPUI type universe in Beryl before shell implementation resumes. Keep startup restoration,
onboarding, placement, close, Exit, later catalog/navigation/activity/status/notice/settings/
transcript mounts, repair, recovery, branch, asset, integration, and closure boundaries in the
active rework tracker until their own bounded slices are ready.

Marker-operation admission and visible refusal, diagnostic activation, and compact repair-media
implementation and acceptance remain in their owning
rework checkpoints; updating target authority does not mark those behaviors implemented.

# Phase 292: Correct The Scrollbar Visibility Fixture (finished)

Accepted scrollbar `70a4dc2`: the fixture tolerates activation redraws while bounded test-support
observation proves obsolete-frame rejection. The negative mutation failed the exact request-count
assertion; restored source passed 62 default and 63 enabled package tests with independent semantic
review. Production behavior is unchanged. The
[dependency-propagation record](failures/gpui-dependent-fork-revision-pin.md) preserves evidence and
retained temporary-checkout state.

# Phase 288: Propagate And Pin Live-Appearance Dependencies (pending)

Publish accepted owned revisions in dependency order through scrollbar, text-input, and settings-
window, then canonically pin one GPUI type universe in Beryl. Verify tracked-lock metadata and
focused checks from neutral working directories before restarting the analyzer.

GPUI `b6939aa0a2d78be59facd12a909a4ada3b6f6a64` is published on `origin/gpui-fork` after
45 focused tests and independent review. Accepted local scrollbar correction `70a4dc2` clears the
visibility gate described in [dependency propagation](failures/gpui-dependent-fork-revision-pin.md).
Retain both default and `--features test-support` scrollbar package gates during propagation.
Scrollbar's canonical manifest and lock still use the prior published GPUI pin; the retained neutral
candidate uses the accepted GPUI revision and correction. Beryl's existing Cargo edits and accepted
text-input appearance work remain unmodified for resumption.

# Phase 289: Build The Target-State Main-Window Shell Foundation (pending)

Use Phase 284 reservations and Phase 285 editor preparation to build the injectable hidden window
host, one distinct window-local controller, and theme-aware ordinary shell composition over the
declared main-window slots. Prepare
the exact claimed editor before GPUI construction, publish no partial OS window, and retain the
reservation through Phase 238 abandonment or reconciliation after construction failure or
close-before-publication. The binary startup/bootstrap path remains outside this phase.

Verify exact editor and appearance preparation, distinct controller ownership, hidden construction
and first publication, construction failure and close-before-publication, retained reservation
through unresolved abandonment, and exact terminal release using the injected host and focused GPUI
tests. Check shell slot layout and absent optional mounts against GUI integration. Run the focused
Cargo check and nextest cases, then independent semantic review of publication and custody.

The live-appearance prerequisites above resolve the gap recorded in
[range-input live appearance](failures/range-input-live-appearance.md) before shell construction.

# Phase 290: Mount Bounded Independent Main-Window Creation (pending)

Mount `New Window` and `Ctrl+Shift+N` through the invoking controller's exact runtime/root, the
process registry, Phase 236 acquisition, Phase 289 hidden host, and exact publication or
abandonment. Verify two isolated visible windows, command and disabled states, capacity and
duplicate admission, failure and late settlement, focus isolation, and repeated creation/release.
