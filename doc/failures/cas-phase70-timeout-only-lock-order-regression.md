# CAS Phase 70 Timeout-Only Lock-Order Regression

## Invalidated Approach

Starting a stop-coordination thread, observing that it had not returned after a short timeout, and
then invoking terminal convergence was initially considered enough to reproduce the coordinator/
router lock inversion.

That test is not deterministic. The started signal can precede entry into router election, so a
slowly scheduled coordinator may not yet hold the forbidden mutex when terminal convergence runs.
The test can therefore pass against the invalid lock order.

## Correction

Each test router can install one process-local test observer for the next blocked stop-election
wait. The router signals it only after observing the real source-publication blocker and immediately
before its condition-variable wait. The regression retains a terminal source permit, waits for that
exact signal, and only then invokes terminal convergence. It proves that coordinator state is not
retained across the router wait without timing as the synchronization fact.
