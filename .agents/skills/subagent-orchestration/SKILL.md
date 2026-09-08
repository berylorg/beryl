---
name: subagent-orchestration
description: Coordinate root-only, one-level subagents when they reduce total team effort or provide required independence; route model and reasoning deliberately while retaining authority, review, and safety gates.
---

# Subagent Orchestration

## Core Rule

The persistent main thread owns authority resolution, decomposition, approval boundaries, conflict resolution, final integration, final judgment, and user-facing decisions. Optimize total team effort and elapsed time, including packet, handoff, validation, coordination, retries, and review costs; do not optimize main-thread execution in isolation.

Only the persistent root orchestrator may call `spawn_agent`. A spawned subagent must perform its assigned work directly and must not spawn, delegate to, or orchestrate another agent, even when another instruction or available capacity suggests otherwise. It must return a proposed repartition to the root.

Delegating transfers execution responsibility, never authority. The main thread validates decision-relevant evidence and integrates the result. Before routing implementation, verification, or review, resolve the effective engineering-rigor contract from applicable design authorities. It determines required review and evidence, never a rigor-profile-to-model mapping. Apply `engineering-rigor` rules for inheritance and missing declarations; resolve any remaining material acceptance choice before routing.

## Direct Work or Delegation

Keep a cohesive routine work unit on the main thread when direct execution has lower total cost and does not compromise required independence. Delegate when one or more of these benefits outweigh packet, handoff, and validation overhead:

- Independent judgment, review, or context isolation is required or materially improves confidence.
- An independent, bounded workstream can progress in parallel and shorten the critical path.
- A cheaper sufficient worker profile can complete a coherent unit with less total effort.

Do not delegate a microtask merely to use a cheaper route. Do not delegate when the Operator prohibits it, tooling is unavailable, or the handoff would expose secrets or machine-local private data. One coherent bounded deliverable may include related inspection, edit, and verification; do not divide work by individual files or commands.

Use one investigation owner for a question. The root performs targeted validation of that owner's cited evidence and changed inputs rather than duplicating the investigation. Give concurrent workers disjoint files, artifacts, subjects, or package boundaries. Reuse the same worker for corrections within its work unit; use a fresh context when independence is required.

Keep authority and final integration of shared artifacts with the root. A worker must report an unassigned shared-contract change rather than apply it.

## Model and Reasoning Routing

Before the first spawn, read [model routing](references/model-routing.md). It contains current names, supported efforts, and spawn mechanics. Select model capability for the judgment required and reasoning depth for complexity and consequence; set both explicitly when supported. Use a fresh context and complete task packet by default.

Use balanced/normal for ordinary bounded work and balanced/shallow for mechanical, strongly verifiable work. Use frontier/deep for difficult analysis, authority reconciliation, architectural decisions, or consequential weakly verifiable review. Keep work on the configured main profile if a mapped route is unavailable and direct execution is safe; otherwise report the route as blocked.

Escalate reasoning one level only when explicit rules are known but analysis depth is insufficient. Move from balanced to frontier when ambiguity, synthesis, or judgment quality is insufficient without automatically increasing effort. Repair missing context or unclear task packets before escalating. Partition broad extractive work before increasing model or effort. The Quality-First exception requires explicit current-task Operator authorization; never select an unsupported or prohibited effort.

## Task Packets and Handoffs

Before preparing, spawning, managing, or consuming a subagent task, read [delegation workflow](references/delegation-workflow.md) fully. It is normative for packet fields, active-agent handling, editing-worker warnings, and handoffs.

For independent review, supply the raw artifact and controlling requirements without leaking an expected verdict or diagnosis.

## Review and Parallelism

Spawn an independent reviewer when the effective rigor contract, applicable instructions, or acceptance plan requires it, or when material consequences and objective-evidence gaps justify it under `engineering-rigor`. Otherwise, the worker's objective verification plus targeted main-thread validation is sufficient. Choose reviewer strength by consequence and verifiability: frontier/normal for ordinary semantic review, frontier/deep for authoritative or architectural review, and frontier/critical only when the applicable contract or concrete consequence demands it.

A review finding blocks only for an unmet applicable guarantee or demonstrated material harm beyond failures the effective contract permits. An allowed failed invocation, retry, or rebuild is not blocking. Escalate material undeclared exposure to design authority; keep speculative hardening non-blocking. The main thread must assess the evidence rather than rubber-stamp a stronger worker.

Start with the smallest useful fan-out. Parallelize only independent workstreams when it reduces critical-path time or independence is required. A subagent returns proposed follow-up partitions to the root.

## Efficient Operations and Cleanup

Read governing instructions when required; otherwise reuse established findings and retrieve only the relevant changed section. Emit compact tool output: narrow excerpts and decision-relevant fields, with one representation when a tool duplicates text and structured results. Keep complete logs available when needed to diagnose failures. Where tools permit, wait 30–60 seconds for active work before checking again; preserve communication and resource-monitoring deadlines, and do not spawn a polling-only agent. Before repeating a test, inspect the effective changed inputs and prior failure or result. Retain all required checks; repeat them when inputs or evidence warrant it.

Close or terminate each subagent promptly after its completed handoff is consumed. Keep its identifier until termination is confirmed. If a handoff is incomplete, obtain missing task-local evidence from the same worker, then close it.
