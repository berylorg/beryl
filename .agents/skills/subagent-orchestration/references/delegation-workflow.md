# Delegation Workflow

This reference is normative for preparing a task packet, spawning a subagent, managing an active subagent, and consuming its handoff. Apply the delegation gate, routing policy, authority boundaries, ownership rules, review requirements, parallelism constraints, and cleanup rules from the parent skill.

## Delegable Work Examples

Use these examples only after the work satisfies the parent skill's delegation gate:

- Workspace, repository, corpus, or artifact exploration beyond narrow inspection.
- Dependency, upstream source, evidence, or authority exploration.
- Test, log, data, event, or record triage and summarization.
- External research and documentation lookup.
- Architecture, structure, design, or authority reconnaissance beyond shared artifacts the main thread owns or must update.
- Independent review whose evidence can be checked by the main thread.
- Drafting, production, or implementation with a coherent deliverable and disjoint ownership boundary.
- Verification or investigation that can be summarized as findings, inspected sources, evidence, and recommended next steps.

## Task Packet

Provide every subagent an explicit task packet containing:

- The user goal.
- The workspace, repository, corpus, or artifact location.
- Relevant instructions and constraints.
- Controlling authorities or source material.
- One coherent bounded deliverable.
- Whether the subagent may edit files.
- Owned files, artifacts, packages, modules, or subjects when edits are allowed.
- The completion condition and required verification or evidence standard.
- The expected handoff shape.

For editing work, tell workers they are not alone in the workspace, must not revert others' edits, and must adjust to concurrent or existing changes.

Use the packet as the subagent's complete context when spawning with a fresh context. If a bounded conversational fork is justified by the model-routing reference, ensure the packet still states the deliverable, authority, ownership, completion, and handoff requirements explicitly.

## Active-Agent Handling

While agents run, perform only necessary nonduplicative orchestration or integration work. Do not manufacture main-thread work merely to avoid waiting.

When a handoff arrives, inspect the cited files, diffs, or commands needed for integration. Do not redo delegated exploration unless the handoff is incomplete or untrustworthy.

## Handoff Requirements

Require a concise handoff containing:

- Exact files, artifacts, sources, URLs, symbols, records, or commands inspected.
- Findings relevant to the task.
- Changed files or artifacts, if any.
- Verification performed and results.
- Recommended next steps.
- Unresolved questions, risks, and blockers.

Reject broad transcript dumps, unrelated source excerpts, and vague summaries as insufficient. If the handoff is incomplete, obtain the missing task-local evidence from the same agent before applying the parent skill's cleanup rule.
