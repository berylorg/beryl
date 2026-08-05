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

Treat the routing reference's orchestrator profile as a recommended pre-session configuration. This skill cannot select or change the profile of an already-started main thread. Apply delegation-forward routing with the configured main profile, and obtain routine savings by delegating execution to the lowest sufficient model family and reasoning depth.

Before the first spawn in a task, read [model routing](references/model-routing.md). Select the model family from the task's judgment and verifiability requirements, then independently select the reasoning depth from its complexity and consequence. Use the reference to map both selections to the current model and reasoning effort.

Pass the selected model and reasoning effort explicitly when the spawn tool supports them. Use a fresh context by default and supply the required context through the task packet. Delegate substantive work that meets the delegation gate even when the main thread could perform it. Do not delegate a microtask merely to access a cheaper model.

If a mapped profile or routing override is unavailable, do not guess a replacement model name or
effort value. Keep the task on the configured main profile when direct execution is safe and allowed;
otherwise report the unavailable route as a blocker.

Other skills may classify domain-local work by the generic routing criteria in this skill. Keep exact model names, effort values, and axis mappings centralized in the routing reference; do not duplicate them in domain skills.

## Routing Criteria

Prefer a balanced worker profile when the contract is explicit, controlling sources are known, the required judgment is limited, and correctness has a strong verifier.

Prefer a frontier profile when the work requires novel or semantically subtle judgment, conflicting-authority reconciliation, architecture or causality decisions, adversarial review, or weakly verifiable synthesis.

Choose reasoning depth independently from model capability. Use shallow reasoning for bounded one-pass work, normal reasoning for ordinary multi-step work, deep reasoning for several interacting constraints or alternatives, and critical reasoning for adversarial, deeply coupled, or high-impact analysis.

## Routing Axes

Select the model family and reasoning depth separately:

- **Balanced model:** Explicit settled contracts, known controlling sources, limited semantic judgment, and strong objective verification. Examples include inventories, extraction, deterministic transformations, routine triage, localized implementation, and drafting from settled decisions.
- **Frontier model:** Novel or semantically subtle judgment, conflicting-authority reconciliation, architecture or causality decisions, adversarial review, or weakly verifiable synthesis.
- **Shallow reasoning:** Bounded one-pass work with few interacting constraints.
- **Normal reasoning:** Ordinary multi-step work with a clear evidence or verification path.
- **Deep reasoning:** Several interacting constraints, alternatives, or causal steps.
- **Critical reasoning:** Adversarial, deeply coupled, high-impact, safety-sensitive, security-sensitive, or irreversible work.

## Exceptional Routes

- **Quality-First:** One bounded problem for which a frontier model with critical reasoning produced a concrete deficiency or the operator explicitly prioritizes marginal quality over token use.
- **Nested:** One deliberately delegated multi-workstream problem with its own disjoint ownership and integration boundaries, used only when project instructions explicitly authorize recursive orchestration.

Exceptional routes override the ordinary routing axes and must never be selected automatically.

## Escalation Rules

- If context is missing, instructions conflict, or scope is vague, repair the task packet instead of raising reasoning depth.
- If explicit rules are present but search or analysis depth is insufficient, raise reasoning depth one level.
- If ambiguity, synthesis, or judgment quality is insufficient, move from a balanced worker profile to a frontier profile without automatically increasing reasoning depth.
- If work is broad but extractive, partition it into bounded worker tasks instead of raising reasoning depth.
- Use Quality-First and Nested only under their explicit exceptional-route conditions.
- Preserve every approval and authority gate; no routing selection expands a subagent's authorization.

Assign one coherent bounded deliverable per agent, not one file or command per agent. Batch tightly related operations when they use the same sources, ownership boundary, and verifier. Require concise handoffs and stop once the evidence threshold is met.

## Delegate Substantive Bounded Work

Use a fresh subagent for work that meets the delegation gate. The subagent must rely on the explicit task packet rather than unbounded inherited parent-thread context.

Before preparing or reviewing a task packet, spawning or managing a subagent, or consuming its handoff, read [delegation workflow](references/delegation-workflow.md) fully as normative. It owns examples, packet fields, editing-worker warning, active-agent rules, and handoff schema. Examples do not broaden the delegation gate.

## Main-Thread Direct Work

The main thread may directly read governing instructions and shared planning, design, authority, or decision files; inspect manifests, catalogs, registries, names, or layout to scope delegation; and perform one known lookup, one short cited inspection, one direct command, or one atomic edit. It defines acceptance criteria, ownership, packets, and routing; validates targeted evidence and diffs; integrates results and resolves conflicts; handles work unsafe to packet; and produces the final judgment and user-facing response.

Direct work must not grow into substantive execution that satisfies the delegation gate.

## Ownership Boundaries

Do not assign multiple subagents to change the same files, artifacts, subject, or project boundary in parallel.

Keep authority and final integration for shared or project-wide artifacts with the main thread. The main thread may explicitly delegate one bounded drafting or editing unit for such an artifact, then validate and integrate it. If a subagent discovers an unassigned shared-contract or authority change, it must report rather than apply it.

Prefer concrete production subtasks with disjoint ownership over vague exploration when the requested work can be safely partitioned.

Reuse the same agent for corrections within its existing work unit. Use a fresh context when independence is required.

## Review Routing

Use a fresh reviewer context and provide the artifact, controlling sources, acceptance boundary, and required evidence without leaking the expected verdict or prior diagnosis.

Spawn an independent reviewer when required by risk, weak objective verification, applicable instructions, or an explicit acceptance plan. Let routine objective verification remain with the worker plus targeted main-thread validation when no independent review requirement applies.

Choose reviewer strength from the consequence and verifiability of the reviewed decision, not from the artifact's format or the author's profile. Use a frontier model with normal reasoning for ordinary semantic review, deep reasoning for authoritative or architectural review, and critical reasoning for adversarial, deeply coupled, weakly verifiable, or materially costly review.

A reviewer reports findings and evidence. It does not inherit the main thread's authority, and the main thread must not rubber-stamp a stronger model's conclusion.

## Parallelism

Start with the smallest useful fan-out. Parallelize only genuinely independent workstreams when simultaneous execution reduces critical-path time or independence is itself required.

Do not assign redundant investigation or overlapping edits unless independent comparison is deliberate. Avoid both microtask spawning and oversized work units that require an unnecessarily strong profile or large context.

## Subagent Cleanup

Close or terminate every subagent promptly after receiving and recording its completed handoff. Do
not leave finished subagents open or idle, even when concurrent-agent capacity is still available.

Keep each subagent's name or identifier until termination is confirmed. Treat cleanup as part of
consuming the handoff and complete it before context compaction can discard the identifier. If the
handoff is incomplete and follow-up work is required, reuse the same subagent only until that
follow-up handoff finishes, then terminate it.
