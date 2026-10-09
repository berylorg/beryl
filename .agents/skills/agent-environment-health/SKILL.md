---
name: agent-environment-health
description: Keep agent-created or agent-caused compute resources bounded and safely reclaimed throughout a task. Use for software and non-software work that creates temporary data, intermediates, logs, caches, downloads, processes, listeners, ports, locks, concurrent workers, or session-scoped environment changes, especially before heavy or long-running work, at handoff, or after success, failure, cancellation, or interruption. Do not use for general machine administration or resources whose ownership is uncertain.
---

# Agent Environment Health

## Invariant

Own the lifecycle and resource impact of every resource created by the agent or by a tool at the
agent's direction. Bound growth while work runs, reclaim ephemeral resources after their last use,
and report any deliberate residue exactly. Do not clean machine state whose ownership is unclear.

Apply this workflow to any project domain. A research run, media conversion, document production,
data analysis, or design workflow can create the same temporary data and process pressure as a
software build.

## Classify Before Creating

Before creating a material resource, classify it as one of these:

- **Durable:** A required project artifact, evidence with a retention requirement, or user-requested
  retained output.
- **Ephemeral:** A task-scoped temporary directory, intermediate, generated cache, download,
  process, listener, lock, or session mutation that can be removed after its last use.
- **Shared or ambiguous:** A resource not positively attributable to this task. Leave it alone.

Choose a task-specific location and identifiable name for ephemeral files. Keep intermediates,
logs, caches, and downloads within a bounded task scope; set size, count, time, or retention limits
where the tool supports them. Do not treat a tool's default shared cache or OS temporary root as a
task-owned cleanup target.

## Task Execution Artifacts

Unless the project declares another location, keep temporary execution inputs, outputs, logs,
and artifacts under `.aipm/tasks/<name>/`. A task is a bounded work item with a completion or
acceptance boundary, not an entire conversation or a new directory for every command. Choose a
descriptive name that distinguishes concurrent work. Create the directory only when files are needed.

Use separate run directories when verification repeats or relevant inputs change:

```text
.aipm/tasks/<name>/
  run-001/
    inputs.json
    tests.log
    artifacts/
  run-002/
    inputs.json
    tests.log
```

These filenames illustrate possible contents, not a required scaffold. Create only what the run
needs. When a manifest is useful for reproduction or evidence reuse, record the command, relevant
configuration, and source/dependency identities; use hashes where version identifiers cannot
identify the actual inputs. Keep credentials out. Preserve completed run inputs and results rather
than overwriting them with a later run. Do not add a narrative verification file by default.

Keep this temporary subtree out of version control and documentation retrieval indexes. It is
execution material, not design authority, a planning system, or a conversation archive. History
retrieval can replace duplicate discussion recaps, but cannot recover artifact contents that never
entered the conversation. Read only the needed manifest fields or log excerpts into model context.

Retain runs until their last acceptance, review, diagnosis, or recovery consumer is finished; a
pause or handoff may still need them. Then preserve lasting decisions, reusable findings, and
failure lessons in the project's established authorities, retain any explicitly required durable
outputs, and clean up the exact owned task directory. Replace active references to deleted runs
with the retained outcome or durable location. Directory placement alone does not prove ownership
or authorize cleanup of sibling tasks.

## Preflight Proportionally

For unusually heavy, long-running, or multi-process work, estimate the relevant pressure before
starting: storage, memory, CPU or accelerator use, network transfer, process count, ports, locks,
and expected retained evidence. Confirm that the task has a bounded workspace, a cleanup point,
and enough capacity for both the work and required outputs.

Limit concurrency to the available budget. Avoid launching resource-heavy workers merely because
they can run in parallel; reserve headroom for the system, other work, and orderly shutdown.
Record the exact identities of agent-launched processes, listeners, ports, locks, and task roots so
they can be verified and reclaimed later.

## Operate With Bounds

- Stream, batch, rotate, or cap logs instead of retaining unbounded output.
- Check resource growth at meaningful boundaries during long or high-volume work. Stop before the
  task risks exhausting shared capacity; preserve the minimum evidence needed to report why.
- Reuse only task-owned intermediates and caches; remove superseded material promptly.
- Stop retries, downloads, and generation loops at explicit limits; retain only evidence needed to
  diagnose or reproduce the result.
- Keep session-scoped mutations reversible. Restore process-scoped environment variables and
  task-local working settings when they are no longer needed.
- Release task-owned locks, listeners, ports, and child processes at their last use. Give launched
  processes a graceful shutdown path and an exact identity check before escalation.

## Clean Up Every Exit

Plan cleanup when creating the resource, not only at normal completion. Run it after success or
handled failure, when cancellation or interruption returns control, and before handoff when work
will continue elsewhere.

1. Preserve required durable outputs and diagnostic evidence that still has a consumer.
2. Stop and wait for task-owned workers; then release their task-owned listeners, ports, and locks.
3. Remove exact task-owned ephemeral directories, intermediates, bounded logs, caches, and
   downloads that have reached their last use.
4. Reverse task-owned session mutations.
5. Verify that the exact targets are gone or returned to their intended state.

If cleanup cannot finish, stop further cleanup outside the known task scope. Report the precise
residue, owner evidence, location or process identity, why it remains, its known impact, and the
recommended next action. Make handoff ownership explicit rather than implying that a later agent
may safely sweep the environment.

## Guardrails

Require positive ownership and exact resolved targets before deletion, termination, or rollback.
Inspect broad roots, globs, symbolic links, junctions, mount points, and other reparse points before
acting; do not let a task path escape into shared or unrelated storage. Prefer a narrow task root
over patterns that could match sibling work.

Never sweep shared OS temporary or cache roots, delete ambiguous data, terminate unknown processes,
or release a lock whose owner is not established. Preserve evidence that is required for accepted
results, failure diagnosis, audit, or handoff. Ask for direction when ownership, retention, or the
safe cleanup boundary is uncertain.

## Non-Goals

Do not use this workflow for general machine administration, software installation, system
configuration, repairing pre-existing resources, ambiguous or shared cleanup, external or cloud
cleanup without an explicit workflow, or product/runtime storage lifecycle design. Those concerns
need their own authority and explicit scope.
