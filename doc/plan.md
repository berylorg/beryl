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

# Phase 241: Propagate The GPUI Revision Through GPUI Scrollbar (finished)

Published `gpui-scrollbar` commit `f0bd3ab07399374f6cc439c257a898cde8ef1247` with the accepted
GPUI revision and one canonical GPUI type universe. Locked metadata, package compilation, and the
focused four-case scrollbar integration target passed outside local patch scope; fresh independent
semantic review accepted the dependency-only change with no finding.

# Phase 242: Diagnose GPUI Text-Input Revision Compatibility (wip)

Use the prepared canonical GPUI and scrollbar pins to determine whether the three `range_widget`
failures are revision-caused package incompatibilities, target-order or concurrency interference, or
pre-existing failures. Reproduce the exact cases individually and together under bounded repeats,
compare them with the prior accepted canonical dependency graph without changing the shared checkout
or relying on local path overrides, and inspect only the implicated `gpui-text-input` and GPUI
boundaries.

Produce an evidence-backed root-cause map naming the violated package contract, owning source
boundary, and smallest source, test, or design correction required for publication. Make no source,
test, API, design, manifest, or lockfile edits in this diagnostic phase; retain the prepared two-file
pin change uncommitted. Obtain fresh independent semantic review of the diagnosis, including its
causal evidence and phase boundary. Clean up any exact task-owned comparison checkout or temporary
artifacts before completion. If the evidence contradicts accepted GPUI authority or requires a
public-contract change, stop for authority correction instead of planning around it.

# Phase 243: Reconcile And Publish GPUI Text Input At The Accepted Revision (pending)

Apply the smallest authority-consistent `gpui-text-input` source correction established by Phase
242, preserve its bounded range-backed and shared-resource guarantees without a compatibility path,
and publish the accepted dependency-consistent commit. Verify canonical locked metadata, package
compilation, prepublication behavior, the corrected range-backed cases and applicable neighboring
integration targets with Cargo Nextest, one GPUI type universe, exact dependency sources, local-
override isolation, and fresh independent semantic review. Replan before editing if Phase 242 finds
an API, design, GPUI-fork, or separately verifiable test-contract boundary.

# Phase 244: Propagate The GPUI Revision Through GPUI Settings Window (pending)

Update `gpui-settings-window` to the accepted GPUI revision and the Phase 241 and 243 widget commits,
verify its focused package boundary, and publish the accepted commit.

# Phase 245: Canonically Pin The Unified GPUI Dependency Chain In Beryl (pending)

Pin GPUI and all three owned widget forks to the accepted propagation commits, regenerate the
canonical lockfile outside local path-patch scope, verify locked metadata and the focused Beryl app
check, restart the language server after the manifest model is accepted, and close the active
rework checklist item.

# Phase 246: Build The Target-State Main-Window Shell Foundation (pending)

Build the app-owned process registry, move-only 256-slot reservation, injectable hidden window host,
one distinct window-local controller, and theme-aware ordinary shell composition over the declared
main-window slots. Prepare the exact claimed editor before GPUI construction, publish no partial OS
window, and use Phase 238 abandonment for construction failure or close-before-publication. The
binary startup/bootstrap path remains outside this phase.

# Phase 247: Mount Bounded Independent Main-Window Creation (pending)

Mount `New Window` and `Ctrl+Shift+N` through the invoking controller's exact runtime/root, the
process registry, Phase 236 acquisition, Phase 246 hidden host, and exact publication or
abandonment. Verify two isolated visible windows, command and disabled states, capacity and
duplicate admission, failure and late settlement, focus isolation, and repeated creation/release.
