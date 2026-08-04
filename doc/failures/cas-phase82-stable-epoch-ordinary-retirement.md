# CAS Phase 82 Stable-Epoch Ordinary Retirement

## Scope

Ordinary retirement and service-registry cleanup after the stable backend core was separated from
its replaceable service epoch.

## Invalidated Approach

Keep the old opportunistic registry `retain` reaper after moving the store-bearing ingester and
forwarding endpoint into a stable hub epoch, and call every retained registry shell's shutdown
path unconditionally during consuming service close.

## Evidence

With disk pressure removed, all 25 active-steering library tests failed deterministically with
`ProjectionConnectionServiceCloseError::ConnectionShutdown`. Their explicit fixture invalidation
had already joined the driver and ingester and detached the hub, but the service registry still
held the content-free shell; service close interpreted a second `current_epoch` lookup failure as a
new connection-shutdown failure.

The focused `dropping_unused_session_reclaims_mounted_connection_permits` test also timed out.
Last-session Drop correctly remained nonblocking and signaled retirement, and the driver returned
its permit, but the finished ingester and its permit remained in the epoch until an opportunistic
reap. That reap ran only from registration, after the next connection pair had already been
reserved, and it called connection lifecycle boundaries while holding the service-registry lock.

## Why It Failed

Retired authority, finished workers, detached runtime, and service-registry membership are distinct
facts. A stable shell may remain registered after a successful explicit shutdown, while a merely
signaled shell may still own a joinable ingester and home reference. Treating both as a live
shutdown target creates double settlement; waiting until after capacity reservation to distinguish
them can strand the capacity needed to make progress. Reaping under the registry also violates the
Phase 82 lock boundary.

## Course Correction

Pre-admission reaping alone is insufficient: without another admission or explicit close, a
finished ordinary ingester would still keep capacity charged indefinitely. The epoch's sticky
ordinary-versus-exact-failure election therefore controls a shared ingester-admission disposition.
Ordinary retirement arms release before cancellation; terminal settlement drops the permit, and an
ordinary arm arriving after terminal drops the retained escrow immediately. Exact failure arms
retain-for-adoption with the cut identity, and its explicit join must recover that exact permit.
Unresolved, poisoned, or conflicting disposition retains conservatively.

Ordinary reaping snapshots registry membership and releases the registry before inspecting or
joining a connection. It reaps only a core whose ordinary retirement won and whose driver and
ingester are already finished, then reacquires the registry solely to remove that exact
pointer-identical detached shell. Admission performs this before connection-pair reservation;
consuming ordinary service close performs it before draining remaining live connections. This is
secondary lifecycle cleanup, not permit-release authority.

Implicit Drop still only signals and wakes. Persistent-failure, quiesced, disabled, and
adoption-owned cores never enter the ordinary reaper. A shell already settled by explicit ordinary
shutdown is removed as completed rather than shut down a second time.

Worker checkout and hub detachment also require one connection-local shutdown settlement. Without
it, concurrent explicit callers can observe `None` after the first caller checks out a worker and
misclassify that in-progress join as a stopped-worker failure. The settlement admits exactly one
executor, caches clean versus failed terminal classification, and makes the reaper participate in
the same serialization boundary.

## Required Proof

Focused tests must prove that a dropped unused session cannot block the next minimum-capacity
admission, sequential connection churn leaves bounded registry membership, explicit invalidation
followed by service close succeeds without double shutdown, reaping performs no connection call
under the service-registry guard, ordinary disposition releases before and after terminal,
exact-cut adoption retains before and after terminal, conflicting dispositions cannot double
release, simultaneous explicit shutdown callers observe one clean settlement, and all Phase 82
inert/adoption ownership tests remain green.
