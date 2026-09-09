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
