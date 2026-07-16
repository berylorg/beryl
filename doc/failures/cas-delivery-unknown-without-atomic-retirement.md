# CAS Delivery Unknown Without Atomic Retirement

## Invalidated Approach

Phase 11 initially exposed a standalone accepted-input mutation that changed one delivering
steering fragment to terminal delivery-unknown while leaving its CAS projection current and its
thread input gate steerable.

## Why It Failed

A possibly dispatched `turn/steer` request makes the projection's represented history unprovable.
Terminalizing only the accepted input leaves a crash cut where Beryl can reopen the same active
projection, admit more steering, and continue from authority that the delivery outcome already
invalidated. A later separate stale-binding mutation cannot close that cut atomically.

## Required Course Correction

Delivery ambiguity must converge through the atomic active-binding abandonment transition. The
same commit retires the CAS thread, publishes stale projection provenance, moves the gate to the
pending submitted turn, changes every delivering steering route to permanent delivery-unknown
history, and reroutes only admitted or retryable work proven undispatched.

No standalone storage API may publish delivery-unknown while the referenced projection remains
active. Persistence-cut tests must prove that mixed abandonment exposes only its complete old or
complete new state.

## Detection

The Phase 11 completion review identified this authority split on 2026-07-15 before the app-owned
live delivery worker was implemented.
