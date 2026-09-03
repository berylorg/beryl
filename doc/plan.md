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

# Phase 273: Correct Presentation-Transition Publication Atomicity (finished)

Accepted `gpui-text-input` commit `1793f665fd938c1f6884f8493251ba30ffedc7be`.
Exact-index presentation transitions now carry the retained surface's exact scroll position only
when `ScrollAnchor` is the effective priority, while non-restoration caret, IME, directed-selection,
and active-interaction targets clear the incompatible lower-priority preservation claim. The
historical generation-two reproducer now publishes and activates the current object, preserves the
exact `Superseded` loss and bounded quiescent ownership, and leaves stale-surface rejection intact.
Fresh review rejected the first unconditional retained-scroll correction; distant active-interaction
coexistence coverage closes that gap, and a new independent semantic review passes the corrected
priority, lifecycle, publication, and stale-surface gates.

# Phase 274: Requalify The Complete GPUI Text Input Focused Gate (pending)

After every proven and newly diagnosed correction has its own accepted phase, rerun the complete
focused gate and preserve only the accepted unchanged seven-test library baseline.

# Phase 275: Publish GPUI Text Input After Its Focused Gate Passes (pending)

Verify every required focused target from a clean canonical checkout, independently review the exact
accepted graph and source, publish only the accepted commit, and remove all task-owned artifacts.

# Phase 276: Propagate The GPUI Revision Through GPUI Settings Window (pending)

Update `gpui-settings-window` to the accepted GPUI revision and the published widget commits, verify
its focused package boundary, and publish the accepted commit.

# Phase 277: Canonically Pin The Unified GPUI Dependency Chain In Beryl (pending)

Pin GPUI and all three owned widget forks to the accepted propagation commits, regenerate the
canonical lockfile outside local path-patch scope, verify locked metadata and the focused Beryl app
check, restart the language server after the manifest model is accepted, and close the active
rework checklist item.

# Phase 278: Build The Target-State Main-Window Shell Foundation (pending)

Build the app-owned process registry, move-only 256-slot reservation, injectable hidden window host,
one distinct window-local controller, and theme-aware ordinary shell composition over the declared
main-window slots. Prepare the exact claimed editor before GPUI construction, publish no partial OS
window, and use Phase 238 abandonment for construction failure or close-before-publication. The
binary startup/bootstrap path remains outside this phase.

# Phase 279: Mount Bounded Independent Main-Window Creation (pending)

Mount `New Window` and `Ctrl+Shift+N` through the invoking controller's exact runtime/root, the
process registry, Phase 236 acquisition, Phase 278 hidden host, and exact publication or
abandonment. Verify two isolated visible windows, command and disabled states, capacity and
duplicate admission, failure and late settlement, focus isolation, and repeated creation/release.
