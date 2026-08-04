# CAS Phase 84 Verifying Scheduler Fatalization

## Scope

Accepted-input scheduler work racing same-home, same-generation health verification under the
process recovery supervisor.

## Invalidated Approach

Scheduler read and coordinator errors treated only an exact `failed` health gate as recoverable.
Every other non-healthy state, including the exact current generation in `verifying`, was classified
as a local fatal scheduler failure. Successful verification merely incremented supervisor
diagnostics because preserving the service owner was assumed to preserve every service worker.

## Evidence

Ordinary service construction publishes one initial scheduler recovery wake. A command can move the
same home generation to `verifying` before that wake finishes its first bounded scan. The scan then
observes a `verifying` health-gate error, closes the master command gate for local failure, and exits
fatally. The process supervisor independently verifies the home healthy and preserves the
pointer-identical service, but its scheduler is already dead; later explicit shutdown reports
`SchedulerShutdown`.

The Phase 84 full fault-injection library suite exposed this deterministically in
`successful_same_generation_verification_preserves_current_service`: the scheduler performed one
startup steering page read, stopped fatally, and left the original service and home generations
otherwise unchanged.

The follow-up classifier audit found the same cause could be lost before scheduler settlement:
ordinary input replay converted typed home-health authority errors into a generic invariant string.
Once verification completed, neither the string nor a later healthy sample could prove that the
worker had observed the exact `verifying` generation.

## Course Correction

Exact same-home, same-generation `verifying` is a typed nonterminal scheduler disposition. Scanner
and worker paths signal or join the existing supervisor flight, retain or safely restart bounded
durable scan work, and park without closing the master gate, exiting, or polling.

Healthy completion is an exact process-slot operation over both current home and service
generations. Only that match may emit a dedicated same-generation-verified scheduler wake. The wake
resumes every applicable lane without reusing the broader recovery signal or granting unrelated
active-steering retry eligibility. A stale completion cannot wake a replacement service, while
failed verification emits no resume and proceeds through the existing persistent-failure cut.

Nested replay, projection, admission, and publication errors retain the typed health-gate source
until worker settlement. Classification never attempts to recover erased provenance from error text
or from a later mutable-health sample.

## Durable Rule

A transient typed health state owned by a process supervisor cannot be collapsed into a permanent
local worker failure. Preserving an owner is insufficient when its internal workers have independent
lifecycle state: successful verification must publish an exact-epoch resume event, and every worker
that can observe the transient state must retain a nonterminal disposition until that event or the
failure cut settles it.
