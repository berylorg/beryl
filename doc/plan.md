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

Use the accepted, canonically pinned single GPUI dependency graph and atomic GPUI window-set
appearance publication when shell implementation resumes. Keep startup restoration,
onboarding, placement, close, Exit, later catalog/navigation/activity/status/notice/settings/
transcript mounts, repair, recovery, branch, asset, integration, and closure boundaries in the
active rework tracker until their own bounded slices are ready.

Marker-operation admission and visible refusal, diagnostic activation, and compact repair-media
implementation and acceptance remain in their owning
rework checkpoints; updating target authority does not mark those behaviors implemented.

# Phase 294: Establish Atomic GPUI Appearance Publication (finished)

All existing theme-runtime publication routes use one bounded GPUI window-set adoption boundary.
Focused Cargo verification and all 33 theme-runtime tests passed, including actual root/composer
painting, editor preservation, rejection, window-set changes, and retirement; independent semantic
review accepted the result. The [publication record](failures/theme-runtime-gpui-publication.md)
preserves evidence and the unaccepted Phase 289 checkpoint, which is ready to resume.

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

Resume the retained, unaccepted shell implementation only after Phase 294 supplies actual GPUI
appearance publication. Complete exact acquisition/editor binding, canonical adaptive layout,
first-presentable and stale-selection publication gates, and the remaining fault tests recorded in
the [publication-gap checkpoint](failures/theme-runtime-gpui-publication.md). The accepted
[range-input live appearance](failures/range-input-live-appearance.md) capability remains a
dependency prerequisite, not evidence of app-level atomic adoption.

# Phase 290: Mount Bounded Independent Main-Window Creation (pending)

Mount `New Window` and `Ctrl+Shift+N` through the invoking controller's exact runtime/root, the
process registry, Phase 236 acquisition, Phase 289 hidden host, and exact publication or
abandonment. Verify two isolated visible windows, command and disabled states, capacity and
duplicate admission, failure and late settlement, focus isolation, and repeated creation/release.
