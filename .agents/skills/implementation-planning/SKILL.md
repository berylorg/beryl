---
name: implementation-planning
description: "Maintain root doc/plan.md implementation plans. Use before implementation work, including single-package work, to verify architecture readiness, create or update the authoritative plan, enforce # Scope and # Phase N status structure, keep one acceptance boundary per phase, pause and replan on material scope growth, maintain a compact sliding execution window, derive phase work and review from applicable design authority and engineering rigor, record blockers, and respect other active planning authorities."
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
2. Stop if the request contradicts design authority; otherwise apply the architecture-readiness gate
   to the actionable slice.
3. Split only architecture-ready implementation work into small coherent phases.
4. Mark the active phase `wip` and future phases `pending`.
5. Include phase tasks, edge cases, verification, and resumable milestone details.
6. Record any blocker in its phase before stopping.

## Architecture-Readiness Gate

Apply the `project-doc-authority` architecture-readiness review before authoring an implementation
scope or phase, and repeat it when scope growth, diagnosis, or review exposes a previously hidden
material choice.

An actionable phase must be derivable from authoritative decisions without using the plan to choose
among materially different architectures. Phase text may translate those decisions into ordering,
work, edge cases, verification, and review, but it must not become the only place that defines
ownership, public boundaries, lifecycle or state semantics, cross-boundary dataflow, failure or
recovery behavior, supported limits, or acceptance evidence.

If any such choice remains missing or contradictory, stop implementation planning for that slice and
update the owning feature, system, package, subproject, GUI, API, or project-declared design authority
first. Do not use an implementation phase, rework item, test expectation, or source experiment as a
temporary architecture decision. A bounded research or diagnosis phase may gather evidence only when
its acceptance boundary is the evidence itself and it does not authorize production implementation.

Implementation-private choices may remain open when every allowed choice satisfies the same
authoritative contract and would not change phase boundaries or completion evidence.

Resolve the effective engineering-rigor contract across every applicable design scope before
splitting phases. Read the engineering-rigor authority and profile catalog as required there. Treat
a missing, unknown, or incompatible required declaration as incomplete design and stop planning
until its owning design authority is corrected.

Translate the contract's supported operating envelope, defensive behavior, failure handling,
verification evidence, and review requirements into concrete phase work. Do not copy profile,
modifier, or catalog declarations into `doc/plan.md`.

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

If scope growth reveals an unresolved architectural choice rather than merely another accepted-design
task, apply the architecture-readiness gate before adding implementation phases. Correct the owning
design authority first, then derive the new phases from it.

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

Perform a completion review for every phase, including documentation-only, verification-only,
integration, and no-change outcomes, before marking it `finished` or beginning the next phase.
Review the completed work and evidence against the phase acceptance boundary and effective
engineering-rigor contract.

Use an independent reviewer only when the effective rigor contract, a concrete consequence, weak
objective verification, another applicable authority, or the phase acceptance plan requires it.
Otherwise, objective verification plus worker self-review and targeted main-thread validation may
satisfy the completion review.

Treat a finding as blocking only when it identifies an unmet applicable guarantee, a concrete
consequence inside the supported operating envelope, or an undeclared trust, exposure, blast
radius, or irreversibility condition. Keep speculative hardening outside the effective contract
non-blocking. Escalate an undeclared condition to the owning design authority; apply the scope-growth
rule only if the corrected authority creates another hard task or acceptance boundary.

When completion review finds a blocking issue within the phase's acceptance boundary, keep the
phase `wip`, record the corrective work in that phase, and address it before repeating review.

After completion review succeeds, mark the phase `finished` and immediately compact it to its heading plus a
few-line outcome that includes the verification result or a durable evidence link. Remove detailed
tasks, edge cases, verification logs, investigation history, and resumable diary content. Perform
this compaction before starting or expanding another phase.

When all phases are complete and no active skill or project authority requires continuation, leave `doc/plan.md` empty unless the project declares another archival convention.
