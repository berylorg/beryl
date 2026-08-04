# CAS Phase 70 Provider Activity Snapshot Linearization

## Invalidated Approach

Recording normalized hard-stop activity immediately after `SourcePublicationPermit::finish()` was
initially treated as sufficient because the activity effect came only from a successfully published
provider frame.

That ordering is invalid. Successful `finish()` cleared the router's source-publication-in-flight
fence and woke the target-operation election before the process-local activity record was applied.
A concurrent stop could therefore acquire the election and freeze its hard-stop snapshot in the
gap, omitting a command whose provider publication had already committed.

Recording activity before `finish()` is also invalid: a later publication failure must not create
activity, and calling the coordinator while retaining the router lock reverses the existing
stop-coordinator-to-router lock order.

## Correction

Successful publication uses a two-stage non-cloneable completion boundary. The first stage commits
provider publication but retains the source-publication election fence without retaining the router
lock. Beryl then records the compact exact activity effect, and consuming the post-commit token
clears the fence and wakes stop election. Failed publication records no activity and releases the
permit through its existing failure path.

The hard-stop attachment snapshot therefore linearizes after every provider publication that won
before the stop election, while publication that loses the election cannot leak into the already
frozen snapshot.

## Lock-Order Correction

The first two-stage implementation still routed activity recording through the coordinator's main
state mutex. That is invalid because stop coordination may retain that mutex while waiting for the
router election. A committed provider publication would then retain the router source fence while
waiting for the coordinator mutex, and stop would retain the coordinator mutex while waiting for
the source fence.

Published activity therefore has its own coordinator-owned mutex. Provider recording touches only
that activity mutex. Snapshot attachment and terminal clearing may acquire activity only after the
main stop-state mutex, and no activity-to-main path exists. The post-commit token can consequently
release the router fence without a coordinator/router lock cycle.
