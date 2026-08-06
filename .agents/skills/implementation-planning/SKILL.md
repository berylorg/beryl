---
name: implementation-planning
description: Maintain root doc/plan.md implementation plans. Use before implementation work, including single-package work, to create or update the authoritative plan; enforce # Scope and # Phase N status structure; keep one acceptance boundary per phase; pause and replan on material scope growth; maintain a compact sliding execution window; derive edge cases from design docs; record blockers; respect other active planning authorities; and review every phase before completion.
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

Treat `doc/plan.md` as a sliding execution window, not a historical ledger:

- Keep the active `wip` phase detailed enough to execute and verify.
- Keep every known future acceptance boundary as a `pending` phase. Detail only the few near-term phases; until activation approaches, later phases need only a heading and concise boundary summary.
- Retain at most the immediately preceding `finished` phase as a short outcome.
- Immediately after successful completion review and before more implementation, compact the phase: remove its checklist, investigation narrative, incremental results, and test history, retaining a concise verification result or durable evidence link.
- Remove any older finished-phase outcome when a newer phase finishes.
- Before compaction deletes material investigation or invalidated-approach history, preserve it through the applicable project research-memory or failure-record authority. Link it when useful; do not duplicate it in the plan.

Reflect planning scope, input, sequencing, or continuation constraints from another active skill or project authority in `doc/plan.md` without redefining its format or workflow.

## Planning Workflow

1. Read the controlling feature, system, package or subproject, API, rework, and design docs, plus project-declared root or parent authorities.
2. Stop if the request contradicts design authority; otherwise derive scope from it.
3. Split the work into small coherent phases.
4. Mark the active phase `wip` and future phases `pending`.
5. Include phase tasks, edge cases, verification, and resumable milestone details.
6. Record any blocker in its phase before stopping.

Before creating a plan, authoring or revising scope or phase content, or reviewing authoring completeness, read [Plan Authoring Template and Edge-Case Prompts](references/plan-authoring.md) in full as normative. Status-only updates, blocker recording, phase compaction, and clearing use this file alone.

Hacks, migration adapters, and untracked workarounds require explicit operator approval before they appear in the plan or code.

If another active skill or project authority explicitly allows a constrained exception, that specific allowance takes priority over the generic workaround rule. The plan may include that exception only with the stated constraints, verification, and completion condition.

## Phase Sizing

Each phase must have exactly one primary acceptance boundary.

A phase may contain a tightly coupled task cluster only when no constituent task can be
independently implemented, verified, reviewed, or resumed. If any constituent task can cross one
of those boundaries independently, make it a separate phase.

Do not pack multiple hard tasks into one phase because they share a feature, package, or milestone. Split phases requiring broad investigation, independent code paths, or several verification strategies.

Do not create numbered implementation sequences, subphases, tranche items, or checkpoint items
inside a phase as substitutes for real phases. Any item substantial enough to carry its own status,
completion result, resumable milestone, or review is a phase.

An integration phase may connect and jointly verify already accepted components. It must not also
implement those components or absorb unfinished component work.

When another active skill or project authority limits the current planning window, keep phases inside that window.

## Scope Growth

Pause implementation immediately when material scope growth reveals another hard task or acceptance
boundary.

Add the newly discovered work as a separate phase and re-establish the execution order before
continuing. Do not append it to the active phase or broaden that phase's acceptance boundary. Keep
minor implementation details within the active phase only when they remain necessary to its existing
acceptance boundary and do not create an independently implementable, verifiable, reviewable, or
resumable unit.

## Execution Rules

When executing the plan:

- Keep `doc/plan.md` status current.
- Apply the scope-growth rule before implementing newly discovered hard work.
- If a planned step cannot technically work, stop and notify the operator instead of quietly inventing a workaround.
- In absence of more specific instructions, stop after one phase is finished.
- If a phase cannot be completed, record the blocking issue in that phase before stopping.
- Do not begin the next phase until the active phase has passed its completion review and has been
  compacted.
- When a phase is finished and later phases remain, stop according to the project's continuation
  policy after performing that compaction.
- When all phases in the current plan are finished, follow any continuation rules from other active skills or project authorities before declaring the plan complete.

## Completion Review

When one phase's implementation and verification are complete, get a reviewer subagent review
before marking that phase `finished` or beginning the next phase. This review is required for every
phase, including documentation-only, verification-only, integration, and no-change outcomes.

If the reviewer finds issues within the phase's acceptance boundary, keep the phase `wip`, record
the corrective work in that phase, and address it before repeating review. If a finding reveals a
new hard task or acceptance boundary, apply the scope-growth rule and create a separate phase.

After review succeeds, mark the phase `finished` and immediately compact it to its heading plus a
few-line outcome that includes the verification result or a durable evidence link. Remove detailed
tasks, edge cases, verification logs, investigation history, and resumable diary content. Perform
this compaction before starting or expanding another phase.

When all phases are complete and no active skill or project authority requires continuation, leave `doc/plan.md` empty unless the project declares another archival convention.
