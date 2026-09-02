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

# Phase 248: Diagnose Clipboard Use Of The Published Coherent Surface (finished)

An exact original-source baseline proved that normal Copy and Cut incorrectly returned `Busy`
before clipboard preparation while a same-binding, same-revision target continued. One isolated
candidate established `begin_clipboard` and `settle_clipboard_write`, with package-boundary tests in
`tests/range_widget.rs`, as the smallest correction owner: clipboard begin must use the published
surface and capture no edit proofs, while Cut resolves its deletion proof only after a successful
write. Locked structural checks, focused clipboard and geometry evidence, exact unchanged-source
failure comparison, and fresh independent semantic review accepted the diagnosis; no shared source
was committed and every task-owned artifact was removed.

# Phase 249: Correct Clipboard Command Gating And Residency (wip)

Correct normal range-backed Copy and Cut only at the proven clipboard owner. Make
`begin_clipboard` select a mounted published coherent surface whose binding and revision match the
current configuration while ignoring a continuing same-binding, same-revision target candidate and
its desired selection, geometry, residency, presentation generation, and interaction state. Capture
only the published binding, revision, composite selection, and predecessor needed by clipboard
begin; do not resolve or retain ordinary edit proofs there, and do not weaken `interactive_surface`
for caret, hit-test, IME, object activation, or other geometry-dependent commands.

After a successful Cut clipboard write, make `settle_clipboard_write` resolve the captured
selection's exact endpoint proofs and admit the deletion transaction against its captured base.
Issue no mutation before `Written`; a failed or cancelled write, missing proof, later proof or
mutation-admission failure, conflict, rejection, cancellation, rebind, or unmount must delete
nothing, clear exact clipboard custody, and never let a late result affect a replacement binding.
Copy must never require deletion proofs.

Add normal-command package-boundary coverage for Copy and Cut while a same-binding, same-revision
target page remains held, including a different unpublished target selection, exact reuse or
coalescing of resident text and object pages, independent target progress, no duplicate request or
page ownership, and exact clipboard release. Preserve unavailability with no coherent surface, a
different binding or revision, an unpublished-only selection, or missing valid published selection.
Keep later exact-fit and one-under clipboard preparation retry distinct from command eligibility and
prove post-write Cut proof or edit failure retains the copied value without deletion. Do not change
the accepted deterministic response-closure implementation or revive purpose-key compatibility.

Use the exact accepted manifests and an isolated task-owned build target. Verify locked metadata,
all package targets, formatting, exact diffs, the focused normal-command and coordinator matrix,
Phase 247 geometry and custody neighbors, applicable unchanged-source baseline failures, and fresh
independent semantic review. Commit only the accepted package source and tests, remove every
task-owned build artifact, and preserve the shared manifest and lockfile hashes.

# Phase 250: Diagnose Resident Object Realization Through Publication (pending)

Trace the overlapping-object case from resident selection through exact scanner preparation,
continuation, terminal target construction, and coherent publication, then exercise one bounded
candidate at the first causal mismatch.

# Phase 251: Correct Resident Object Realization (pending)

Apply and independently verify the smallest package-source correction established by Phase 250
without a purpose-key compatibility path or duplicate object-page ownership.

# Phase 252: Publish GPUI Text Input At The Accepted Revision (pending)

Integrate the accepted corrections, verify canonical locked metadata, package compilation,
prepublication and applicable range-widget targets, one GPUI type universe, exact dependency
sources, local-override isolation, and fresh independent semantic review, then publish the accepted
commit.

# Phase 253: Propagate The GPUI Revision Through GPUI Settings Window (pending)

Update `gpui-settings-window` to the accepted GPUI revision and the Phase 241 and 252 widget commits,
verify its focused package boundary, and publish the accepted commit.

# Phase 254: Canonically Pin The Unified GPUI Dependency Chain In Beryl (pending)

Pin GPUI and all three owned widget forks to the accepted propagation commits, regenerate the
canonical lockfile outside local path-patch scope, verify locked metadata and the focused Beryl app
check, restart the language server after the manifest model is accepted, and close the active
rework checklist item.

# Phase 255: Build The Target-State Main-Window Shell Foundation (pending)

Build the app-owned process registry, move-only 256-slot reservation, injectable hidden window host,
one distinct window-local controller, and theme-aware ordinary shell composition over the declared
main-window slots. Prepare the exact claimed editor before GPUI construction, publish no partial OS
window, and use Phase 238 abandonment for construction failure or close-before-publication. The
binary startup/bootstrap path remains outside this phase.

# Phase 256: Mount Bounded Independent Main-Window Creation (pending)

Mount `New Window` and `Ctrl+Shift+N` through the invoking controller's exact runtime/root, the
process registry, Phase 236 acquisition, Phase 255 hidden host, and exact publication or
abandonment. Verify two isolated visible windows, command and disabled states, capacity and
duplicate admission, failure and late settlement, focus isolation, and repeated creation/release.
