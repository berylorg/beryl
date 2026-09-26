# Shutdown Confirmation Admission Ordering And Retention

## Invalidated Composition

On 2026-09-26, source inspection during ordinary close/Exit composition showed that the accepted
read-only shutdown observation cannot simply be revalidated inside
`ProcessAdmissionGate::fence_if_quiescent`. The conditional fence holds the process-admission mutex
through its callback. Existing work-revision readers acquire locks above that mutex in dispatch
paths. This finding concerns the proposed integration; no such callback has been mounted.

## Decisive Evidence

- `crates/beryl-app/src/process_admission.rs`: `fence_if_quiescent` holds `inner` across validation;
  `ProcessExecutionPermit::commit` acquires the same mutex.
- `crates/beryl-app/src/cas_projection/connection/router/command.rs`:
  `authorize_turn_start` holds the router state lock while invoking
  `LiveCommandPermit::commit_execution_if_current`.
- `crates/beryl-app/src/cas_projection/persistent_failure/gate/permit.rs`:
  `commit_execution_if_current` holds the live-command gate through acquisition of process
  admission. The dispatch lock order is router, command gate, process admission.
- `crates/beryl-app/src/cas_projection/service/process_work/required.rs`:
  `validate_required_work_revision` calls `validate_connection_work_revision`.
- `crates/beryl-app/src/cas_projection/service/work_facts.rs`: connection revision validation
  locks the connection registry, then `read_work_stamp` checks the live-command gate and reads
  each connection's work stamp.
- `crates/beryl-app/src/cas_projection/connection/router/work_facts.rs`: `work_stamp` locks router
  state. Thus validation under process admission would acquire both command and router locks in
  the reverse direction. A dispatch worker holding either while awaiting process admission can
  prevent the shutdown callback from returning, leaving both workers blocked.

Context-compaction and active-steering dispatch use the same execution admission relationship.
The source lock cycle is sufficient to reject this composition; no deliberate hanging test was
run. Existing observation and conditional-fence tests do not exercise this proposed combination.

Independent concurrency review confirmed both cycles. It additionally checked
`persistent_failure/gate/authorizer.rs`: `authorize` holds the live-command gate while obtaining
`process.execution_permit()`, which takes process admission without increasing reservations.
Therefore the conditional fence's zero-reservation check cannot exclude this conflict. A source
search confirmed that the conditional fence has no mounted caller. No production changes or
new Cargo verification were made for this source-only assessment.

## Recommended Correction And Remaining Boundary

Keep observation outside process admission. Introduce a dedicated admission-safe validation path
whose checks under the admission lock never wait for a lock that dispatch can hold while seeking
admission. Nonblocking exact revision and provenance checks can refuse busy or changed sources
without fencing or changing execution authority. Audit every nested source, including health,
durable revision, sessions, connections, controls and cleanup custody; changing only the router
read is insufficient. Do not substitute a speculative fence followed by reopening on cancellation.

The Operator clarified that bounded technical corrections proceed autonomously; this finding
does not require another approval. Derive each prerequisite and lock-order evidence from the existing
[window lifecycle contract](../../crates/beryl-app/doc/design-shell-lifecycle.md#window-detachment-and-process-shutdown)
and [main-window behavior](../features/main-windows/design.md#ordinary-window-close).
Verification must cover contention with actual dispatch lock order, stale/foreign evidence,
unchanged permits after refusal, and work appearing before the idle admission cut. Native
confirmation, restore modes and process-owner mounting remain separate unfinished work.

The durable-read prerequisite was accepted on 2026-09-26: home-store observed coherent election
validates exact observation/store identity and unchanged mutation interval, then holds nonblocking
mutation, reconciliation and health guards through the caller's publication. Observation precedes
the durable reads; returning successfully does not retain proof for a later publication. All 27
focused tests passed as run `399f44a3-f217-49d1-baac-d8afffed4616`, together with the default package
check and independent concurrency/integrity review. Nonblocking runtime-only revision checks
and a borrowed backend response read guard were subsequently accepted. Complete shutdown
admission remains unaccepted; point-in-time reads alone do not supply atomic publication proof.

## Retaining Every Source Guard Violates The Connection Bound

On 2026-09-26, the phase 603 prototype retained authority, forwarding attachment, router and
response guards for every registered connection through observed home election and process-fence
publication. Five focused admission tests passed, including guard retention, stale home intervals,
contention and unchanged execution authority on refusal. Those tests did not cover registry growth.
Independent resource/concurrency review rejected the whole-registry guard collection, and the
unaccepted prototype was removed. The accepted backend response guard remains available.

The app [shutdown connection contract](../../crates/beryl-app/doc/design-live-projection-and-scheduling.md)
requires bounded retained handles independently of historical registry size. The regression
`tests/unit/shutdown_connection_traversal.rs::failed_inventory_exceeds_worker_capacity_with_one_borrowed_handle`
demonstrates nine retained failed connections with worker capacity four. Four guard vectors
indexed by that complete registry violate the contract even without cloning connection Arcs.

Terminal classification alone does not establish a bound on the remaining mutable entries:

- `connection/driver.rs::DriverContext::retire_attachment` calls nonwaiting `retire_locked`.
  In `connection/authority.rs`, `complete_retirement_locked` leaves retirement incomplete while
  cleanup or promotion owners remain.
- Driver exit and `connection/provider_broker/ingester/lifecycle.rs::mark_terminal` can return
  worker custody. `ConnectionCleanupOwner` and `ConnectionPromotionReservation` retain command
  and connection custody without worker permits, so retired mutable entries can exceed capacity.
- Cleanup/promotion acquisition uses an already-held command permit under connection authority;
  retaining process and command-gate locks does not by itself freeze every custody transition.
- `persistent_failure/coordinator/worker.rs::freeze_and_dispatch_targets` closes/drains admission
  and seals router state. Its bounded worker-backed capture cannot be copied into a side-effect-free
  shutdown decision or refusal.

These source paths are relative to `crates/beryl-app/src/cas_projection/`. A fully retired, detached
entry with completed retirement and no remaining owners can be folded into counters, but that
does not cover all supported mutable retired entries.

Recommended architectural correction: establish a service-owned connection-work observation and
election boundary covering membership and custody transitions, allowing exact revision validation
and publication with constant retained observation state. Its ownership, mutation coverage and
lock ordering need explicit design authority before implementation. Do not impose an arbitrary
historical-entry quota, discard failed-join evidence, speculate retirement or drain admission before
confirmation. Phase 603 is paused for Operator direction on this architectural boundary; ordinary
close/Exit mounting remains dependent on its acceptance.
