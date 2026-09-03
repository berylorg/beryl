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

# Phase 269: Diagnose Current-Surface Pointer Activation (finished)

Accepted `gpui-text-input` test-only commit
`58363fcad0b155a4604ff56675eb777d41e3ba13`. The original failing click never reaches the pointer
pipeline: presentation ownership advances to generation two, but terminal publication records one
`CandidateSurfaceIncomplete`, becomes quiescent, and retains the generation-one surface at epoch
two. Exact current-surface gating therefore correctly rejects the retained surface before hit
resolution, `Set` intent, activation, resolver, or installation. The accepted diagnostic binds the
generation-one activation and `Superseded` loss, generation-two requests, repeated-generation no-op,
retained surface and geometry, inert pointer outcome, accounting, and quiescence. Repeated exact
feature-enabled runs and fresh independent review pass. The production correction owner is the
presentation-transition terminal rejection/publication atomicity path, not pointer activation.

# Phase 270: Diagnose Atom-Cut Restoration Proof (pending)

Locate the missing published source-position or object-gap proof that makes restoration export fail
before the Cut action begins; do not change Cut propagation without that proof.

# Phase 271: Diagnose Large-Object Presentation Publication (pending)

Locate the first request, response, accounting, or terminal-publication liveness mismatch that leaves
one bounded large-object target pending without a coherent surface.

# Phase 272: Correct Presentation-Transition Publication Atomicity (pending)

Prevent an accepted presentation-generation transition from becoming quiescent with only the prior
generation's surface retained after `CandidateSurfaceIncomplete`. Preserve exact current-surface
authority, bounded request and response ownership, coherent terminal publication, and existing loss
semantics; prove the original generation-two pointer activation reproducer passes without weakening
stale-surface rejection.

# Phase 273: Requalify The Complete GPUI Text Input Focused Gate (pending)

After every proven and newly diagnosed correction has its own accepted phase, rerun the complete
focused gate and preserve only the accepted unchanged seven-test library baseline.

# Phase 274: Publish GPUI Text Input After Its Focused Gate Passes (pending)

Verify every required focused target from a clean canonical checkout, independently review the exact
accepted graph and source, publish only the accepted commit, and remove all task-owned artifacts.

# Phase 275: Propagate The GPUI Revision Through GPUI Settings Window (pending)

Update `gpui-settings-window` to the accepted GPUI revision and the published widget commits, verify
its focused package boundary, and publish the accepted commit.

# Phase 276: Canonically Pin The Unified GPUI Dependency Chain In Beryl (pending)

Pin GPUI and all three owned widget forks to the accepted propagation commits, regenerate the
canonical lockfile outside local path-patch scope, verify locked metadata and the focused Beryl app
check, restart the language server after the manifest model is accepted, and close the active
rework checklist item.

# Phase 277: Build The Target-State Main-Window Shell Foundation (pending)

Build the app-owned process registry, move-only 256-slot reservation, injectable hidden window host,
one distinct window-local controller, and theme-aware ordinary shell composition over the declared
main-window slots. Prepare the exact claimed editor before GPUI construction, publish no partial OS
window, and use Phase 238 abandonment for construction failure or close-before-publication. The
binary startup/bootstrap path remains outside this phase.

# Phase 278: Mount Bounded Independent Main-Window Creation (pending)

Mount `New Window` and `Ctrl+Shift+N` through the invoking controller's exact runtime/root, the
process registry, Phase 236 acquisition, Phase 277 hidden host, and exact publication or
abandonment. Verify two isolated visible windows, command and disabled states, capacity and
duplicate admission, failure and late settlement, focus isolation, and repeated creation/release.
