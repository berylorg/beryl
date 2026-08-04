# CAS Phase 38 Connection Retirement Is Not Target Abandonment

## Invalidated Approach

Use public whole-connection invalidation to represent exact live-event target abandonment in the
ordinary submitted-input residency fault suite.

## Evidence

Ordinary execution constructs and consumes its `LiveEventTarget` internally. The existing exact
receiver-abandonment helper requires that target, while the public session invalidation surface
retires connection-wide authority and every target routed through it.

## Why It Failed

Connection retirement exercises the router-wide shutdown taxonomy. It cannot prove that dropping
one execution receiver revokes and converges only its registered target while the surrounding
WebSocket connection remains healthy.

## Required Course Correction

Phase 38 uses a narrowly scoped `test-faults` controller at the existing ordinary target owner. The
controller may request and observe abandonment of only the exact receiver after real registration;
it exposes no content, alternate execution path, target handle, or normal-build behavior.

The fault suite separately verifies whole-connection transport loss through its real transport
boundary. These failure modes must remain distinct.

## Affected Work

`doc/plan.md` Phase 38 and its submitted-input residency failure suite own the correction.
