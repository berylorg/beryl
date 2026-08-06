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

- **Durable:** A required project artifact, accepted evidence, or user-requested retained output.
- **Ephemeral:** A task-scoped temporary directory, intermediate, generated cache, download,
  process, listener, lock, or session mutation that can be removed after its last use.
- **Shared or ambiguous:** A resource not positively attributable to this task. Leave it alone.

Choose a task-specific location and identifiable name for ephemeral files. Keep intermediates,
logs, caches, and downloads within a bounded task scope; set size, count, time, or retention limits
where the tool supports them. Do not treat a tool's default shared cache or OS temporary root as a
task-owned cleanup target.

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

1. Preserve required durable outputs and the minimum accepted diagnostic evidence.
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
