# CAS Phase 77 Public Owned Home Command

## Invalidated Approach

Expose a public movable live-command capability that owns `Arc<HomeStore>` and a non-cloneable
master-gate permit.

## Evidence

`ProjectionConnectionService::shutdown_inner` closes the gate, joins only service-owned workers,
and then requires `Arc::try_unwrap` before explicitly closing the home. A caller-owned movable
capability can outlive those joins and make ordinary service close fail with
`HomeOwnershipLeaked` after the rest of the service has already been dismantled.

## Why It Failed

Command admission and home-lifetime ownership are separate responsibilities. A scoped gate permit
can fence persistent-failure work, but it does not give the service cancellation or join authority
over an arbitrary external worker that retains a strong home owner.

## Required Course Correction

Keep the public synchronous home capability borrow-scoped. Any capability that must move to a
non-GPUI worker belongs behind a service-owned worker registration and shutdown/drain boundary;
ordinary close must account for that worker before attempting sole home ownership.

## Affected Work

Root `doc/plan.md` Phase 77 and the `beryl-app` process-shell command boundary own the correction.
