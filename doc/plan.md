# Scope

Active architectural rework: `syndic-to-renderer`.

Rework tracker: `doc/rework/syndic-to-renderer/REWORK.md`.

This plan covers the current executable phases only. The exhaustive migration checklist lives in the rework tracker. Target design authority remains in `doc/features/`, root `doc/design.md`, and package `doc/design.md` files.

No adapters from the new Syndic transcript stack to legacy transcript data structures are allowed without explicit operator approval.

# Phase 1: Establish rework authority (finished)

- Move obsolete transcript feature documentation out of authoritative paths.
- Install target transcript feature docs under `doc/features/transcript/`.
- Move the Syndic-native renderer note under the transcript feature as a supplemental target doc.
- Create `doc/rework/syndic-to-renderer/REWORK.md` with target docs, cutover boundary, reference snapshot, forbidden local APIs, and checklist.
- Update root documentation entry points so fresh sessions find the rework tracker and target transcript docs.
- Verify no obsolete transcript feature body remains at the authoritative transcript path.

Resumable milestone: rework awareness is explicit before any live source is archived or replaced.

# Phase 2: Define shell-facing transcript boundary (wip)

- Define the new transcript host boundary that can replace legacy `ConversationSurfaceState` transcript fields without preserving old type names.
- Specify the owned state, inputs, outputs, demand facts, diagnostics, and invariants for that boundary.
- Keep the boundary independent of `syndic-storage` and all forbidden legacy transcript APIs listed in `REWORK.md`.
- Do not create source files or migrate call sites in this phase.

Current finding: the archive step is blocked until the shell-facing transcript surface is replaced first. `ShellView`, `ConversationSurfaceState`, selected-thread activation, diagnostics, render theme construction, and integration tests all depend directly on legacy transcript modules and type names. Preserving those names as no-op shims would be an adapter-shaped workaround and is not allowed by the active rework.

Operator hold: rework preparation is complete for now. Do not start live source migration, source archiving, or new transcript host implementation until the operator explicitly resumes the rework.

Resumable milestone: the proposed shell-facing transcript boundary is documented in this plan or a linked design note and is ready for temporary-behavior planning.

# Phase 3: Decide temporary cutover behavior (pending)

- Decide what the user sees or loses temporarily while the new host is initially empty or fixture-backed.
- Cover transcript rendering, selection, quote, branch/edit menus, media actions, status-line view facts, diagnostics, activation progress, and retained previous transcript visibility.
- Record any behavior that must remain unavailable until later checkpoints rather than emulated through legacy adapters.
- Do not create source files or migrate call sites in this phase.

Resumable milestone: temporary behavior is explicit enough that the first source cutover can avoid accidental compatibility shims.

# Phase 4: Choose first source cutover slice (pending)

- Identify the first implementation slice that can replace live shell transcript fields with the new boundary.
- Name the owning live module path, shell fields and accessors to replace, source modules to leave untouched until later, and tests to retire or rewrite.
- Keep the slice small enough to compile and review before source archival.
- Do not create source files or migrate call sites in this phase.

Resumable milestone: the first source cutover slice is small, ordered, and reviewable before implementation starts.

# Phase 5: Define verification gates (pending)

- Define exact forbidden-import searches, manifest/path checks, and build or test commands for the first implementation slice.
- Include gates proving renderer code does not call Syndic or `syndic-storage` directly and new code does not use forbidden legacy APIs.
- Include gates for obsolete tests that import legacy transcript internals by path.
- Do not create source files or migrate call sites in this phase.

Resumable milestone: the first source cutover has objective pass/fail verification before implementation starts.

# Phase 6: Review Cutover Blueprint checkpoint (pending)

- Review the boundary, temporary behavior, first slice, and verification gates as the Checkpoint 1 Cutover Blueprint.
- Update `doc/rework/syndic-to-renderer/REWORK.md` to mark the accepted Checkpoint 1 items done if the blueprint is accepted.
- If accepted, replace this plan with the first implementation phase derived from the next unchecked `REWORK.md` checkpoint.
- If not accepted, revise only the blueprint planning phases; do not start source migration.

Resumable milestone: Checkpoint 1 is either accepted and ready for implementation planning, or blocked with explicit operator feedback.
