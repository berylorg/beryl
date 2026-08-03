---
name: subagent-orchestration
description: Coordinate delegation-forward, model-routed subagents with orchestrator ownership. Use whenever substantive bounded research, source or dependency inspection, evidence gathering, triage, summarization, production, implementation, verification, or review can be delegated; make profiled subagents the default execution path, minimize main-thread execution, use fresh-context task packets and disjoint ownership, require concise handoffs, and keep authority, integration, and user-facing decisions with the main thread.
---

# Subagent Orchestration

## Core Rule

Treat the persistent main thread as a thin authoritative orchestrator. The main thread owns authority resolution, decomposition, routing, approval boundaries, conflict resolution, final integration, final judgment, and user-facing decisions.

Run substantive bounded research, production, implementation, verification, and independent review in explicitly profiled subagents by default. Minimize main-thread execution, not main-thread judgment.

Delegating work transfers execution responsibility, never authority. A subagent may investigate, draft, edit, implement, verify, or review, but the main thread remains responsible for validating decision-relevant evidence and integrating the result.

## Delegation Gate

Delegate a work unit when all of these conditions hold:

- It has one coherent bounded deliverable and an explicit completion or evidence condition.
- It is separable from final cross-workstream judgment.
- It is substantive: it requires multiple nontrivial reasoning or tool cycles, inspection of multiple non-authority artifacts, an edit plus verification, or an independent evaluation.
- The main thread can write a complete task packet without first performing substantially the same work.

Independence or useful concurrency may justify delegation even when the execution itself is small. Otherwise keep work on the main thread when packet creation, handoff, and validation are reasonably expected to cost as much as direct execution.

Do not delegate when the operator explicitly says not to, subagent tooling is unavailable, or the task contains secrets or machine-local private data that should not be copied into a handoff.

## Model and Reasoning Routing

Treat the routing reference's orchestrator profile as a recommended pre-session configuration. This skill cannot select or change the profile of an already-started main thread. Apply delegation-forward routing with the configured main profile, and obtain routine savings by delegating execution to the lowest sufficient routing class.

Before the first spawn in a task, read [model routing](references/model-routing.md), select the lowest-cost routing class sufficient for that subtask, and use the reference to map that class to the current model and reasoning effort.

Pass the selected model and reasoning effort explicitly when the spawn tool supports them. Use a fresh context by default and supply the required context through the task packet. Delegate substantive work that meets the delegation gate even when the main thread could perform it. Do not delegate a microtask merely to access a cheaper model.

If a mapped profile or routing override is unavailable, do not guess a replacement model name or
effort value. Keep the task on the configured main profile when direct execution is safe and allowed;
otherwise report the unavailable route as a blocker.

Other skills may classify domain-local work by the generic routing criteria in this skill. Keep exact model names, effort values, and class mappings centralized in the routing reference; do not duplicate them in domain skills.

## Routing Criteria

Prefer a balanced worker profile when the contract is explicit, controlling sources are known, the work is reversible, and correctness has a strong verifier.

Prefer a frontier profile when the work requires novel judgment, conflicting-authority reconciliation, architecture or causality decisions, adversarial review, weakly verifiable synthesis, or a materially costly conclusion.

Choose reasoning depth independently from model capability. Use shallow reasoning for bounded one-pass work, normal reasoning for ordinary multi-step work, deep reasoning for several interacting constraints or alternatives, and critical reasoning for adversarial, deeply coupled, or high-impact analysis.

## Routing Classes

- **Economy:** Inventories, targeted searches, extraction, formatting, deterministic transformations, known command execution, and concise summaries with objective verification.
- **Standard:** Bounded research, routine triage, localized implementation, and drafting from explicit settled decisions.
- **Careful:** Rule-bound work with several interacting constraints and a strong verifier, including normal implementation from a settled design or reconciliation against an explicit constraint set.
- **Judgment:** Bounded but semantically subtle decisions, such as classifying authority, comparing a few alternatives, or reviewing an ordinary authoritative artifact.
- **Deep:** Architecture, novel design, complex implementation, conflicting evidence or authority, scientific or mathematical feasibility, long causal chains, and weakly verifiable synthesis.
- **Critical:** Adversarial independent review and high-consequence work involving safety, security, irreversible loss, protected authority, deeply coupled systems, or extensive downstream effects.

## Exceptional Routes

- **Quality-First:** One bounded problem for which Critical produced a concrete deficiency or the operator explicitly prioritizes marginal quality over token use.
- **Nested:** One deliberately delegated multi-workstream problem with its own disjoint ownership and integration boundaries, used only when project instructions explicitly authorize recursive orchestration.

Exceptional routes are not part of the ordinary routing ladder and must never be selected automatically.

## Escalation Rules

- If context is missing, instructions conflict, or scope is vague, repair the task packet instead of raising reasoning depth.
- If explicit rules are present but search or analysis depth is insufficient, raise reasoning depth one level.
- If ambiguity, synthesis, or judgment quality is insufficient, move from a balanced worker profile to a frontier profile without automatically increasing reasoning depth.
- If work is broad but extractive, partition it into bounded worker tasks instead of raising reasoning depth.
- Use Quality-First and Nested only under their explicit exceptional-route conditions.
- Preserve every approval and authority gate; no routing class expands a subagent's authorization.

Assign one coherent bounded deliverable per agent, not one file or command per agent. Batch tightly related operations when they use the same sources, ownership boundary, and verifier. Require concise handoffs and stop once the evidence threshold is met.

## Delegate Substantive Bounded Work

Use a fresh subagent for work that meets the delegation gate. The subagent must rely on the explicit task packet rather than unbounded inherited parent-thread context.

Common delegable work includes:

- Workspace, repository, corpus, or artifact exploration beyond narrow inspection.
- Dependency, upstream source, evidence, or authority exploration.
- Test, log, data, event, or record triage and summarization.
- External research and documentation lookup.
- Architecture, structure, design, or authority reconnaissance beyond shared artifacts the main thread owns or must update.
- Independent review whose evidence can be checked by the main thread.
- Drafting, production, or implementation work with a coherent deliverable and disjoint ownership boundary.
- Verification or investigation whose result can be summarized as findings, inspected sources, evidence, and recommended next steps.

## Main-Thread Direct Work

The main thread may directly:

- Read governing instructions and shared planning, design, authority, or decision files needed to decompose or judge the task.
- Check manifests, catalogs, registries, file names, or directory layout to scope delegation.
- Perform one known lookup, inspect one short cited region, run one direct command, or make one atomic edit.
- Define acceptance criteria, ownership boundaries, task packets, and routing.
- Validate targeted evidence, inspect diffs or changed artifacts, integrate results, and resolve conflicts.
- Handle work that cannot safely be placed in a task packet.
- Produce the final judgment and user-facing response.

Direct work must not grow into substantive execution that satisfies the delegation gate.

## Task Packet

Provide every subagent an explicit task packet:

- User goal.
- Workspace, repository, corpus, or artifact location.
- Relevant instructions or constraints.
- Controlling authorities or source material.
- One coherent bounded deliverable.
- Whether the subagent may edit files.
- Owned files, artifacts, packages, modules, or subjects when edits are allowed.
- Completion condition and required verification or evidence standard.
- Expected handoff shape.

For editing work, tell workers they are not alone in the workspace, must not revert others' edits, and must adjust to concurrent or existing changes.

## Ownership Boundaries

Do not assign multiple subagents to change the same files, artifacts, subject, or project boundary in parallel.

Keep authority and final integration for shared or project-wide artifacts with the main thread. The main thread may explicitly delegate one bounded drafting or editing unit for such an artifact, then validate and integrate it. If a subagent discovers an unassigned shared-contract or authority change, it must report rather than apply it.

Prefer concrete production subtasks with disjoint ownership over vague exploration when the requested work can be safely partitioned.

Reuse the same agent for corrections within its existing work unit. Use a fresh context when independence is required.

## Review Routing

Use a fresh reviewer context and provide the artifact, controlling sources, acceptance boundary, and required evidence without leaking the expected verdict or prior diagnosis.

Spawn an independent reviewer when required by risk, weak objective verification, applicable instructions, or an explicit acceptance plan. Let routine objective verification remain with the worker plus targeted main-thread validation when no independent review requirement applies.

Choose reviewer strength from the consequence and verifiability of the reviewed decision, not from the artifact's format or the author's profile. Use Judgment for ordinary semantic review, Deep for authoritative or architectural review, and Critical for adversarial, deeply coupled, weakly verifiable, or materially costly review.

A reviewer reports findings and evidence. It does not inherit the main thread's authority, and the main thread must not rubber-stamp a stronger model's conclusion.

## Parallelism

Start with the smallest useful fan-out. Parallelize only genuinely independent workstreams when simultaneous execution reduces critical-path time or independence is itself required.

Do not assign redundant investigation or overlapping edits unless independent comparison is deliberate. Avoid both microtask spawning and oversized work units that require an unnecessarily strong profile or large context.

## While Agents Run

While agents run, perform only necessary nonduplicative orchestration or integration work. Do not manufacture main-thread work merely to avoid waiting.

When a handoff arrives, inspect the cited files, diffs, or commands needed for integration. Do not redo delegated exploration unless the handoff is incomplete or untrustworthy.

## Subagent Cleanup

Close or terminate every subagent promptly after receiving and recording its completed handoff. Do
not leave finished subagents open or idle, even when concurrent-agent capacity is still available.

Keep each subagent's name or identifier until termination is confirmed. Treat cleanup as part of
consuming the handoff and complete it before context compaction can discard the identifier. If the
handoff is incomplete and follow-up work is required, reuse the same subagent only until that
follow-up handoff finishes, then terminate it.

## Handoff Requirements

Require concise handoffs with:

- Exact files, artifacts, sources, URLs, symbols, records, or commands inspected.
- Findings relevant to the task.
- Changed files or artifacts, if any.
- Verification performed and results.
- Recommended next steps.
- Unresolved questions, risks, and blockers.

Reject broad transcript dumps, unrelated source excerpts, and vague summaries as insufficient.
