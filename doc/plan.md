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

# Phase 239: Establish Hidden-Window First Publication In Owned GPUI (finished)

Published owned-fork commit `f013db758350882871ec0b666b5a841343557b26` with one identity-
preserving, nonactivating, retryable hidden-window first-publication boundary across Test, Windows,
macOS, Wayland, and X11 backends. Hidden controls are inert until publication, immediate behavior is
preserved, macOS hidden fullscreen fails before native allocation, and platform-required initial
ordering is the only admitted ordering change.

Formatting, locked metadata, GPUI checks, and all 6 focused lifecycle cases passed on the Windows
host. Fresh independent semantic review accepted the complete boundary with no finding; native
macOS and Linux execution remains unavailable. A direct Beryl pin was invalidated because the three
owned widget forks still pin the prior GPUI revision; the clean propagation is split into the next
bounded phases and recorded in
[`doc/failures/gpui-dependent-fork-revision-pin.md`](failures/gpui-dependent-fork-revision-pin.md).

# Phase 240: Propagate The GPUI Revision Through GPUI Scrollbar (wip)

Update `gpui-scrollbar` to the accepted GPUI revision, regenerate only its canonical dependency
state, run locked metadata plus focused package verification, and publish the accepted commit.

Blocked pending Operator acknowledgement of the clean dependency-pin cascade discovered by the
canonical Beryl check. Do not substitute a Beryl root patch or local path override for a published
dependency-consistent revision.

# Phase 241: Propagate The GPUI Revision Through GPUI Text Input (pending)

Update `gpui-text-input` to the accepted GPUI revision and the Phase 240 scrollbar commit, verify
its focused package and prepublication/streaming boundaries, and publish the accepted commit.

# Phase 242: Propagate The GPUI Revision Through GPUI Settings Window (pending)

Update `gpui-settings-window` to the accepted GPUI revision and the Phase 240 and 241 widget commits,
verify its focused package boundary, and publish the accepted commit.

# Phase 243: Canonically Pin The Unified GPUI Dependency Chain In Beryl (pending)

Pin GPUI and all three owned widget forks to the accepted propagation commits, regenerate the
canonical lockfile outside local path-patch scope, verify locked metadata and the focused Beryl app
check, restart the language server after the manifest model is accepted, and close the active
rework checklist item.

# Phase 244: Build The Target-State Main-Window Shell Foundation (pending)

Build the app-owned process registry, move-only 256-slot reservation, injectable hidden window host,
one distinct window-local controller, and theme-aware ordinary shell composition over the declared
main-window slots. Prepare the exact claimed editor before GPUI construction, publish no partial OS
window, and use Phase 238 abandonment for construction failure or close-before-publication. The
binary startup/bootstrap path remains outside this phase.

# Phase 245: Mount Bounded Independent Main-Window Creation (pending)

Mount `New Window` and `Ctrl+Shift+N` through the invoking controller's exact runtime/root, the
process registry, Phase 236 acquisition, Phase 244 hidden host, and exact publication or
abandonment. Verify two isolated visible windows, command and disabled states, capacity and
duplicate admission, failure and late settlement, focus isolation, and repeated creation/release.
