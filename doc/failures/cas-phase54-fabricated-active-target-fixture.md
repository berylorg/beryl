# CAS Phase 54 Fabricated Active-Target Fixture

## Invalidated Approach

Exercise the mounted steering worker by converting an already active durable binding directly
into a live target with `into_active_live_event_target`.

## Evidence

The resulting router target has an exact CAS thread and turn but no `pending_activation` proof.
Every worker case that reached connection-wide steering-attempt acquisition therefore failed with
`TargetMismatch`; only the pre-claim worker-capacity case passed.

## Why It Failed

An active binding is durable storage state, not proof that this connection and target performed the
ordinary pending-turn activation handoff. Steering authority also depends on the exact submitted
turn, binding revision, execution snapshot, activation gate revision, and loaded generation
captured by that handoff.

Relaxing production validation or synthesizing those facts in a steering-only fixture would test
authority that Beryl never grants in normal execution.

## Required Course Correction

Mounted steering tests must establish their live target through the real ordinary
`turn/start` activation path and retain the resulting target while the turn remains active.
Direct active-target construction remains suitable only for tests whose operation does not require
pending-activation authority.
