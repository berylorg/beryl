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

# Phase 242: Propagate The GPUI Revision Through GPUI Text Input (wip)

Update `gpui-text-input` to GPUI commit `f013db758350882871ec0b666b5a841343557b26` and
`gpui-scrollbar` commit `f0bd3ab07399374f6cc439c257a898cde8ef1247` without source or API
changes. Regenerate the canonical lockfile outside repository-local path-patch scope, verify locked
metadata and package compilation, and run the focused prepublication and streaming integration
targets with Cargo Nextest. Confirm one resolved GPUI revision and the accepted scrollbar source,
obtain the package-contract completion review, publish the accepted commit, and preserve local
development overrides as noncanonical environment state.

Do not substitute a Beryl root patch or local path override for the published dependency-consistent
revisions. If the accepted dependency commits require text-input source, API, test, or design
changes, pause and replan that separate acceptance boundary.

# Phase 243: Propagate The GPUI Revision Through GPUI Settings Window (pending)

Update `gpui-settings-window` to the accepted GPUI revision and the Phase 241 and 242 widget commits,
verify its focused package boundary, and publish the accepted commit.

# Phase 244: Canonically Pin The Unified GPUI Dependency Chain In Beryl (pending)

Pin GPUI and all three owned widget forks to the accepted propagation commits, regenerate the
canonical lockfile outside local path-patch scope, verify locked metadata and the focused Beryl app
check, restart the language server after the manifest model is accepted, and close the active
rework checklist item.

# Phase 245: Build The Target-State Main-Window Shell Foundation (pending)

Build the app-owned process registry, move-only 256-slot reservation, injectable hidden window host,
one distinct window-local controller, and theme-aware ordinary shell composition over the declared
main-window slots. Prepare the exact claimed editor before GPUI construction, publish no partial OS
window, and use Phase 238 abandonment for construction failure or close-before-publication. The
binary startup/bootstrap path remains outside this phase.

# Phase 246: Mount Bounded Independent Main-Window Creation (pending)

Mount `New Window` and `Ctrl+Shift+N` through the invoking controller's exact runtime/root, the
process registry, Phase 236 acquisition, Phase 245 hidden host, and exact publication or
abandonment. Verify two isolated visible windows, command and disabled states, capacity and
duplicate admission, failure and late settlement, focus isolation, and repeated creation/release.
