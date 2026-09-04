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

# Phase 280: Publish GPUI Text Input After Its Focused Gate Passes (finished)

Accepted `gpui-text-input` commit `d8d04dd` passed the clean detached-clone gate at 18 of 18
prepublication, 306 of 306 default integration, 309 of 309 `test-support`, and 103 of 108 library
cases with exactly five expected historical failures. Independent graph and source review found no
blocking issue; canonical `origin/main` now names the exact accepted commit, and all Phase 280
artifacts were removed.

# Phase 281: Propagate The GPUI Revision Through GPUI Settings Window (pending)

Update `gpui-settings-window` to the accepted GPUI revision and the published widget commits, verify
its focused package boundary, and publish the accepted commit.

# Phase 282: Canonically Pin The Unified GPUI Dependency Chain In Beryl (pending)

Pin GPUI and all three owned widget forks to the accepted propagation commits, regenerate the
canonical lockfile outside local path-patch scope, verify locked metadata and the focused Beryl app
check, restart the language server after the manifest model is accepted, and close the active
rework checklist item.

# Phase 283: Build The Target-State Main-Window Shell Foundation (pending)

Build the app-owned process registry, move-only 256-slot reservation, injectable hidden window host,
one distinct window-local controller, and theme-aware ordinary shell composition over the declared
main-window slots. Prepare the exact claimed editor before GPUI construction, publish no partial OS
window, and use Phase 238 abandonment for construction failure or close-before-publication. The
binary startup/bootstrap path remains outside this phase.

# Phase 284: Mount Bounded Independent Main-Window Creation (pending)

Mount `New Window` and `Ctrl+Shift+N` through the invoking controller's exact runtime/root, the
process registry, Phase 236 acquisition, Phase 283 hidden host, and exact publication or
abandonment. Verify two isolated visible windows, command and disabled states, capacity and
duplicate admission, failure and late settlement, focus isolation, and repeated creation/release.
