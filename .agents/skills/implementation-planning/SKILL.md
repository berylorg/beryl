---
name: implementation-planning
description: Maintain root doc/plan.md implementation plans. Use before implementation work, including single-package work, to create or update the authoritative plan; enforce # Scope and # Phase N status structure; derive edge cases from design docs; track readiness and resumable milestones; record blockers; and follow stop-after-phase and review-after-completion rules.
---

# Implementation Planning

## Core Rule

Before implementation starts anywhere in a workspace, capture the work in the root `doc/plan.md`. The plan is authoritative for implementation sequencing and must derive from the design docs. Code changes must derive from the plan.

Use this default unless a project explicitly declares another plan authority.

## Plan File State

Interpret root `doc/plan.md` as:

- Missing: no implementation work has ever been planned for this workspace.
- Present and non-empty: active or pending planned work exists.
- Present and empty: planned work existed before and all phases are complete.

When non-empty, `doc/plan.md` must contain:

- `# Scope`
- one or more phase sections exactly in the form `# Phase N: <description> (pending|wip|finished)`

Track readiness and the latest resumable milestone so later sessions can continue correctly.

## Planning Workflow

1. Read the controlling root, feature, parent, package, API, and design docs.
2. Stop if the requested work contradicts design authority.
3. Define scope from the authoritative docs.
4. Split implementation into coherent phases.
5. Mark the active phase `wip`; leave future phases `pending`.
6. Include phase tasks, edge cases, verification, and resumable milestone details.
7. Before stopping on a blocker, write the issue into the relevant phase.

All hacks, migration adapters, and workarounds require explicit operator approval before they appear in the plan or code.

## Edge-Case Checklist

During planning, derive an explicit edge-case checklist from relevant design docs and contracts. Pay special attention when work:

- Creates new state from existing state: copy, fork, clone, import, restore, resume, retry, migration, or template flows.
- Combines ownership boundaries: local, remote, persisted, generated, cached, or user-authored state.
- Has precedence, fallback, inheritance, defaulting, or override rules.
- Runs asynchronously, in the background, or across sessions or processes.
- Depends on optional, stale, partial, missing, or externally supplied metadata.
- Must preserve identity, ordering, provenance, permissions, or user intent.
- Has cleanup, cancellation, rollback, or partial-failure behavior.

For each identified interaction, include a verification case or state why no additional verification is needed.

## Execution Rules

When executing the plan:

- Keep `doc/plan.md` status current.
- If a planned step cannot technically work, stop and notify the operator instead of quietly inventing a workaround.
- In absence of more specific instructions, stop after one phase is finished.
- If a phase cannot be completed, record the blocking issue in that phase before stopping.
- When a phase is finished and later phases remain, stop according to the project's continuation policy.

## Completion Review

When all phases are finished and the changes touched authoritative docs or code, get a reviewer subagent review and address discovered problems.

If the reviewer finds issues that need fixing, plan that work in `doc/plan.md` before implementing the fixes.

When all phases are complete, leave `doc/plan.md` empty unless the project declares another archival convention.

