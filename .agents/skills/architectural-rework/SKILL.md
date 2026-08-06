---
name: architectural-rework
description: Manage clean architectural replacement work with concise rework trackers and without incremental migratory compatibility. Use when a project, feature, system, or subproject is under rework, old docs or source must be archived and removed from live authority, target design must remain in authoritative docs, the durable plan must consume bounded tracker slices, completed tracker work must be compacted, or Codex must avoid migration adapters while replacing architecture.
---

# Architectural Rework

## Core Rule

Treat an architectural rework as a removal-first clean replacement with explicit authority, archive, and cutover boundaries.

Do not migrate by keeping obsolete implementation alive behind adapters, compatibility layers, bridges, or transitional flows unless the operator explicitly approves that workaround.

- Stop and ask the operator if the durable plan requires a step that cannot technically work without a migration adapter or untracked workaround.

During an active rework, do not preserve obsolete live behavior by naming it legacy, transitional, compatibility, or compile-only surface. If existing docs describe an obsolete behavior that way, reconcile the docs to the target architecture before implementing. Treat obsolete APIs, tests, protocol probes, and call paths as removal candidates unless the operator has explicitly approved a bounded exception.

After target authority and the rework tracker exist, remove obsolete live docs and source from authoritative locations and live project membership. Do this even if the project temporarily stops building or running.

Temporary breakage is acceptable only inside an active tracked rework gap. Record that gap in the cutover boundary and checklist instead of hiding it with compatibility glue.

Iterative work during a rework means iteratively adding the new target implementation after the obsolete implementation has been removed. It does not mean iteratively reshaping old code toward the target design.

## Removal Gap And Cutover Shims

The intentional gap is the mechanism that prevents defaulting to incremental migration. Once
obsolete code is archived, the surviving outer code may have jagged edges where dependencies used
to exist. Authorize and briefly explain each gap in `# Cutover Boundary`. When an active durable-plan
phase touches a gap, reflect that gap in the phase; do not copy unrelated gap history into the plan.

Temporary cutover shims are allowed only after the obsolete implementation has been removed. They may connect surviving outer code to the new replacement boundary as that boundary is built.

An allowed cutover shim must be:

- Minimal: only enough surface to connect a surviving edge to target-state replacement code.
- Forward-facing: its design points at the target docs and replacement boundary, not the archived implementation.
- Tracked: authorized in `# Cutover Boundary` with its short reason and removal condition. A
  checklist item may track its removal but cannot authorize the shim.
- Isolated: it does not import, wrap, call, extend, or preserve archived code.

Do not create a shim that keeps obsolete models authoritative, routes new code through old shapes, or gradually migrates old implementation internals toward the target design. That is a migration adapter, not a cutover shim.

## Authority Model

Target-state design stays in the normal authoritative locations:

- User-visible feature behavior stays in authoritative feature docs.
- Cross-feature or cross-package technical architecture stays in authoritative system docs.
- Local package, subproject, or artifact boundary contracts stay in their authoritative package docs.
- Project-declared root or parent authority docs are used only when the project assigns them a target-state role.

New source code stays in final live source locations.

Obsolete material moves under the rework archive:

- Obsolete docs: `doc/rework/<name>/old-doc/...`.
- Obsolete source snapshots: `doc/rework/<name>/old-code/...`.

The rework tracker is `doc/rework/<name>/REWORK.md`.

The active durable plan must point to the active `doc/rework/<name>/REWORK.md`. The project's planning authority owns the durable-plan location and pointer format.

`REWORK.md` feeds the durable plan; it does not replace the project's planning authority or define that authority's work-item format.

Do not add other tracker sections unless the operator explicitly requests one or a project-declared
tracker convention requires one.

Before a checklist item becomes actionable, every target-state decision it depends on must be
complete and non-contradictory in an authoritative doc linked under `# Target Docs`. If authority is
missing, retain only one short target-doc-gap item naming its owner or path, stop implementation,
and correct that authority. Never make `REWORK.md` the temporary home of proposed or accepted
target design.

## Discovery Rule

On any architectural rework task, read the active durable plan before source edits.

If the durable plan points to an active rework, read that `REWORK.md` and its target docs before relying on existing source, feature docs, system docs, or package docs.

If the user describes an active rework but the durable plan does not point to `doc/rework/<name>/REWORK.md`, reconcile the awareness docs first. Do not start implementation while the active rework authority is implicit or discoverable only from conversation history.

## Required Workflow Reference

Before creating or revising `REWORK.md`, archiving or cutting over obsolete material, selecting a tracker slice for the durable plan, executing or resuming rework work, reviewing or verifying any rework tracker, rework batch, or architectural rework, compacting completed tracker work, or closing a rework, read [Rework Tracker Workflow](references/rework-tracker-workflow.md) in full and follow it as normative. It owns the exact tracker schema and prose limits, checkpoint and compaction mechanics, creation and active-work sequences, and verification checklist. Do not improvise alternate tracker structures or execution flows.
