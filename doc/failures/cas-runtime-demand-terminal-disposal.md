# Runtime Demand Requires Guaranteed Terminal Disposal

## Scope

Process-owned execution composition in Phase 325. This is an independently reviewed source
counterexample at revision `35b5b5f4`, not a reproduced production incident. Retention was deferred
until autonomous terminal disposal passed acceptance; both corrections are now accepted.

## Invalidated Approach

An admitted session currently owns required runtime interest, while a loaded projection or
compaction cleanup can outlive that session. Proposed retention shared the same interest across
the existing driver and ingester admissions, including stop custody. The managed runtime's own
startup session would remain unguarded. This covers the normal work lifetime, but existing
ordinary cleanup does not guarantee release of a retained admission after disposition poison.

## Decisive Evidence

Paths below are relative to `crates/beryl-app/src/cas_projection`.

- `connection/provider_broker/ingester/lifecycle.rs`: `mark_terminal` retains the worker when
  its disposition mutex is poisoned. A consuming joined owner can subsequently take it.
- `connection/attachment.rs`: `begin_ordinary_retirement` ignores failure to arm ordinary worker
  release. `connection/lifecycle.rs`: `signal_ordinary_retirement` requests cancellation and
  driver stop; it does not itself guarantee a later joined disposal.
- `try_reap_ordinary_retirement` requires finished workers. The immediate session retirement
  attempt can precede worker completion. Later reaping is driven by session cleanup, a new
  admission, or service/runtime shutdown, rather than a guaranteed terminal completion trigger.
- `accepted_input_scheduler/signal/handle.rs`: `wake_worker_release` produces no wake without an
  armed capacity waiter. Worker exit therefore cannot be assumed to schedule final disposal.
- `runtime_interest/managed.rs`: runtime retirement disposes app resources. Retaining the final
  runtime demand on the poisoned admission prevents idle runtime retirement from initiating that
  disposal. An unrelated future admission or explicit shutdown cannot prove autonomous release.
- `connection/driver.rs`: `run_driver` declares its worker after `DriverRetirementGuard`, so the
  worker drops first. Demand retained by that worker alone would not cover final guard cleanup.

Normal ingester completion closes approval and reservation custody before terminal publication;
the accepted permission-disposal correction remains valid. The new issue concerns runtime demand
and the availability of a consuming terminal owner, rather than the observation-capacity bound.

## Correction And Verification

Establish a guaranteed consuming completion path for retained failed admissions, including
attachment-lock poison, before retaining runtime demand through worker custody. Preserve the
existing exact revocation, join and failure semantics. Driver retention must extend through its
retirement guard. Verification must exercise last-session release and terminal cleanup with no
later admission, no capacity waiter and no explicit shutdown, as well as normal loaded-projection
and compaction handoffs.

The Operator authorized this correction on 2026-09-10. The existing runtime health worker supplies
the independent consuming trigger; no extra connection worker or scheduler retry is required.
The plan separates terminal disposal, runtime-demand retention and exact idle-session retirement.
Terminal disposal is independently accepted. Seven focused tests exercise real managed-process
release, active sibling preservation, three poisoned cleanup owners, exact runtime/process
filtering, contended settlement and a winning persistent-failure cut. Independent review caught a
blocking registry-removal lock after the initial snapshot; maintenance now defers that removal
when contended, retaining detached entries only until a later tick. Poison never becomes a clean
receipt or reauthorized execution.

All 118 selected lifecycle/runtime regressions, production compilation, changed-file formatting
and diff checks passed. All six guarded jobs and temporary directories were reclaimed. Required
runtime-demand retention through worker custody was subsequently accepted with the same admitted
interest shared across the existing connection worker pair. Publication retains an outer owner
until runtime locks are released, locators remain weak, and driver custody extends through all
retirement cleanup. The managed-runtime startup session remains unguarded.

Two real managed-process tests prove loaded-projection and retained-stop-custody survival after
session/view release, same-period reattachment and eventual process/token/worker release. All 169
selected runtime, stop, compaction and lifecycle regressions, production compilation, formatting
and diff checks passed. Independent review found no remaining publication, release-order or
ownership-cycle blocker. All five guarded jobs and temporary directories were reclaimed.
Exact idle-session retirement remains a separate acceptance boundary. This record does not
replace the controlling lifetime authority.

The controlling lifetime requirements remain in
[app projection and scheduling](../../crates/beryl-app/doc/design-live-projection-and-scheduling.md)
and the [CAS-live system](../systems/cas-live-syndic-transcript/design.md).
