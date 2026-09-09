# Stop Custody Can Outlive Connection Worker Reservations

## Scope

Revision-bound stop work observation in the app process owner. This records a source-proven
failure of the proposed bookkeeping bound; it is not evidence of a reproduced live incident.

## Invalidated Approach

Bound removed-but-owned stop records using the configured connection worker capacity, assuming
one election owner and one post-election driver tail per reserved connection cover every owner.

## Decisive Evidence

- `ProjectionConnectionService::coordinate_stop` in
  `crates/beryl-app/src/cas_projection/service/commands.rs` obtains `StopOwnership::Primary`
  before calling `dispatch_exact_stop`. The caller can be suspended between those steps.
- `ProjectionConnection::coordinate_stop` in
  `crates/beryl-app/src/cas_projection/connection/lifecycle.rs` holds no runtime mutex across
  that handoff. `StopDispatchOwner` retains its election and live-command permits, but no
  `ProjectionWorkerPermit`.
- Normal terminal publication is fenced: `acquire_source_publication_inner` in
  `connection/router/permit.rs` waits for the exact stop election before `terminal_consumed`.
  That normal-terminal schedule does not establish this failure.
- `EventRouter::acquire_target_loss` in `connection/router/loss.rs` waits for publication and
  steering owners, but not the stop election. `ProviderBroker::publish_target_loss` calls
  `StopCoordinator::abandon_for_authority_loss`, which can abandon and remove the local stop
  while the suspended caller retains its owner.
- `execute_ordinary_shutdown` joins the driver and ingester and detaches the connection. Their
  worker permits are released independently of that caller. A live-command permit counts
  outstanding work for gate closure; it does not reserve connection worker capacity.
- `prepare_session_admission_with_workers` can acquire the returned permits while the same
  healthy service remains open. Repeating loss, retirement and admission can retain removed
  owners from arbitrarily many old connection generations relative to that worker capacity.

Independent source review confirmed the loss path and the separate normal-terminal fence.
The initial finding preceded implementation; verification below exercises the accepted correction.

## Accepted Course Correction

The Operator approved retaining the existing worker reservation through admitted stop custody and
disposal. [App live-control authority](../../crates/beryl-app/doc/design-live-control.md#custody-and-release)
now specifies that lifetime, including caller handoff and driver cleanup. The plan places its
implementation and verification before stop observation. The reservation-lifetime prerequisite
is independently accepted; control observation remains subsequent work.

Do not retain an unbounded duplicate catalog, silently omit retired owners, or substitute an
arbitrary observation cutoff for the claimed derivation. Existing source-unavailable behavior
does not prove the missing owner-lifetime bound.

## Verification

Three focused external tests exercise the real broker loss path using exact receiver abandonment,
local stop removal, ordinary retirement, replacement denial before socket admission, and reuse
after rejected handoff or unwind. A raw socket close alone can defer consumer closure behind the
held election; the test explicitly injects consumer-source loss instead of waiting for that fence.

The driver test pauses after actual interrupt settlement has consumed the primary owner and before
backend unbind. After the original ingester exits, a read-only weak-reference probe proves both
original admission units remain retained. Scheduler activity cannot satisfy those identities.
Driver completion and joined retirement then release the pair.

Production compilation and 43 selected stop, approval, worker-capacity and terminal tests passed.
Independent semantic review accepted the exact retention and disposal ordering. A stale footprint
fixture was corrected for the already accepted non-idle source record; production footprint
behavior did not change. Guarded job memory peaked at 2.08 GiB; all owned children exited and test
temporary directories were removed.

## Permission Disposal Prerequisite

The accepted stop correction does not cover a joined permission obligation, whose primary owner
is absent. Its proposed observation bound uses one driver-held obligation plus one reserved or
pending slot per connection. Normal ingester exit closes the slot, but caught-panic exit does not.
Independent source review found this release-order counterexample; no live reproduction is claimed.

- `Ingester::run` in `connection/provider_broker/ingester/core.rs` catches a panic from
  `run_loop` and returns a terminal receipt without executing its final `approval.close`.
  An already installed joined `Pending` can remain in the shared broker slot. Its ingester
  worker is terminal but initially retained under the undecided worker disposition.
- `ProjectionConnection::request_ordinary_retirement` in `connection/lifecycle.rs` elects
  ordinary retirement and marks connection authority retired before dropping its locks and
  separately calling `signal_ordinary_retirement`. Suspend the elected caller between those cuts.
- `begin_ordinary_retirement` arms ordinary ingester release. For the already terminal ingester,
  `ProviderBrokerWorkerOwner::arm_ordinary_release` in `provider_broker/ingester/lifecycle.rs`
  immediately releases its worker admission.
- The sole driver observes retired authority and exits. Its retirement guard in
  `connection/driver.rs` does not call `broker.request_cancel` when another caller already elected
  ordinary retirement. The driver admission is released while the elected caller remains paused.
- Both worker units are now reusable but the joined obligation remains in its retained slot.
  Moving the driver's worker variable outside its retirement guard does not close this schedule:
  the guard itself skips cancellation when retirement was previously elected.

All source paths above are relative to `crates/beryl-app/src/cas_projection`. The stop reservation
acceptance remains valid; permission-slot disposal before capacity reuse was a separate gap.
The public page bound could not rely on worker capacity while that interval existed.

The Operator approved making approval-slot closure and disposal
precede ingester terminal publication on caught-panic exit as well as normal exit. Verify an
installed joined obligation, failed ingester, paused ordinary retirement, driver exit and exact
capacity reuse. A reserved preparation must also release its observation during unwind. Preserve
the existing ordered obligation and interruption rules; introduce no extra pool or observation
cutoff. The prerequisite is independently accepted and stop/permission observation has resumed.
Updating driver drop order alone would have been insufficient.

`Ingester::run` now closes the slot and cancels its abandoned reservation after the caught execution
boundary and before its receipt. Two focused tests use the real broker worker and caught-panic
path, with a routed joined obligation or a reserved slot. Before cancellation or join, they prove
the slot is fully closed while its worker is still reserved, then prove exact ordinary release and
replacement admission. The failure receipt remains unclean. Independent review, production
compilation and 49 selected broker, approval, worker, stop and terminal tests passed; peak guarded
job memory was 2.07 GiB and all owned processes and test temporary directories were reclaimed.

## Accepted Observation

Stop pages merge the existing local registry with compact live custody derived from the retained
worker reservation. Permission identity follows reservation, preparation, pending slot, driver and
disposal, including joined admission without a primary stop owner. Neither source retains payloads
or execution authority. Closed healthy generations reject observation; these pages do not inventory
the separately owned persistent-failure cleanup.

The initial routing implementation moved prepared custody inside `commit_if_current`. Rejection
could then drop its coordinator token while holding the command gate, reversing the existing
coordinator-to-command-gate order. Routing now borrows preparation to construct metadata under the
gate and transfers custody after releasing the gate and router lock. Rejected preparation also
drops after both locks. The deterministic gate regression pauses disposal while the coordinator
is held and proves command authorization remains available; independent review checks router-lock
release and unwind ordering as well.

Independent semantic review, production compilation and 62 selected stop, approval, worker,
terminal, page and persistent-failure tests passed. Exact protocol tests cover local removal while
driver cleanup remains, permission handoff and final disappearance; unit checks cover bounded
pagination, revision exhaustion and poisoning. Changed-file formatting passed; package formatting
has existing unrelated composer/projection differences. Peak guarded job memory was 2.08 GiB.
All owned children exited and 13 task temporary directories were removed.
