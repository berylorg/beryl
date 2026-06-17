---
name: subagent-orchestration
description: Coordinate subagents with orchestrator ownership. Use when broad exploration, dependency inspection, test triage, log summarization, research, review, or parallel implementation is needed; preserve automatic delegation for broad work, fork_context=false task packets, disjoint edit ownership, concise handoffs, and main-thread ownership of routing, planning, design judgment, root/shared artifacts, final integration, and user-facing decisions.
---

# Subagent Orchestration

## Core Rule

Treat the main thread as orchestrator. The main thread owns routing, planning, design judgment, root/shared artifacts, final integration, and user-facing decisions.

Use subagents automatically for broad exploration unless the operator explicitly says not to, subagent tooling is unavailable, or the task involves secrets or machine-local private data that should not be copied into a handoff.

## Delegate Broad Work

Delegate broad work to a fresh subagent with `fork_context=false`. The subagent must rely only on the explicit task packet, not inherited parent-thread context.

Broad work includes:

- Codebase exploration beyond narrow inspection.
- Dependency or upstream source exploration.
- Test triage and log summarization.
- Web research and documentation lookup.
- Architecture and design-doc reconnaissance beyond root/shared files the main thread owns or must update.
- Broad search or file-reading tasks.
- Any investigation whose result can be summarized as findings, file paths, commands run, and recommended next steps.

## Narrow Main-Thread Inspection

The main thread may inspect narrowly when needed to orchestrate or verify:

- Read global or workspace instruction files.
- Read shared planning or design files the main thread owns or must update, including `doc/plan.md`, relevant system docs, and project-declared root or parent authority docs.
- Check manifests, file names, or directory layout to scope delegation.
- Read a short cited snippet to verify a handoff.
- Inspect diffs for changes the orchestrator must integrate.
- Answer from a single known file or one direct command when delegation adds no context benefit.

Keep narrow inspection small enough that it does not replace delegated exploration.

## Task Packet

Provide every subagent an explicit task packet:

- User goal.
- Workspace or repository path.
- Relevant instructions or constraints.
- Exact question to answer or implementation responsibility.
- Whether the subagent may edit files.
- Owned files, packages, or modules when edits are allowed.
- Expected handoff shape.

For coding work, tell workers they are not alone in the codebase, must not revert others' edits, and must adjust to existing changes.

## Edit Boundaries

Do not assign multiple subagents to edit the same files or the same package/subproject in parallel.

Keep shared/root artifacts under main-thread ownership unless explicitly delegated. If a subagent discovers a needed shared-contract change, it should report it; the main thread applies it.

Prefer concrete code-change worker subtasks with disjoint ownership over vague read-only exploration when implementation can be safely partitioned.

## While Agents Run

After delegating, continue useful non-overlapping local work. Wait only when the next critical-path step depends on the result.

When a handoff arrives, inspect the cited files, diffs, or commands needed for integration. Do not redo delegated exploration unless the handoff is incomplete or untrustworthy.

## Handoff Requirements

Require concise handoffs with:

- Exact files, URLs, symbols, or commands inspected.
- Findings relevant to the task.
- Changed files, if any.
- Verification performed and results.
- Recommended next steps.
- Unresolved questions, risks, and blockers.

Reject broad transcript dumps, unrelated source excerpts, and vague summaries as insufficient.
