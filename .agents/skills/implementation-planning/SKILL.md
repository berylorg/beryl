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
- Represent every known future acceptance boundary as a `pending` phase so work cannot disappear
  from the plan. Keep only the few near-term phases detailed; express later phases as a heading and
  concise acceptance-boundary summary until they approach activation.
- Retain at most the immediately preceding `finished` phase as a short outcome.
- Compact a phase immediately after its completion review succeeds. Remove its task checklist,
  investigation narrative, incremental results, and test-by-test history before further
  implementation begins, while retaining a concise verification result or durable evidence link in
  the phase outcome.
- Remove any older finished-phase outcome when a newer phase finishes.
- Before deleting material investigation or invalidated-approach history during compaction,
  preserve it through the applicable project research-memory or failure-record authority. Link the
  resulting record when useful; do not duplicate its body in the root plan.

If another active skill or project authority constrains planning scope, inputs, sequencing, or continuation, reflect those constraints in `doc/plan.md` without redefining that authority's file format or domain-specific workflow.

## Planning Workflow

1. Read the controlling feature, system, package or subproject, API, rework, and design docs, plus any project-declared root or parent authority docs.
2. Stop if the requested work contradicts design authority.
3. Define scope from the authoritative docs.
4. Split implementation into small coherent phases.
5. Mark the active phase `wip`; leave future phases `pending`.
6. Include phase tasks, edge cases, verification, and resumable milestone details.
7. Before stopping on a blocker, write the issue into the relevant phase.

Hacks, migration adapters, and untracked workarounds require explicit operator approval before they appear in the plan or code.

If another active skill or project authority explicitly allows a constrained exception, that specific allowance takes priority over the generic workaround rule. The plan may include that exception only with the stated constraints, verification, and completion condition.

## Phase Sizing

Each phase must have exactly one primary acceptance boundary.

A phase may contain a tightly coupled task cluster only when no constituent task can be
independently implemented, verified, reviewed, or resumed. If any constituent task can cross one
of those boundaries independently, make it a separate phase.

Do not pack multiple hard tasks into a single phase just because they share a feature, package, or milestone. If a phase would require broad investigation, multiple independent code paths, or several verification strategies, split it.

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
