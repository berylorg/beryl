# CAS Rebinding Before Local Terminal Convergence

## Invalidated Approach

A Phase 11 validation test attempted to publish a fresh valid CAS binding immediately after atomic
abandonment of an active projection whose submitted turn already had admitted CAS events.

## Why It Failed

Abandoning projection authority does not erase or finish the active Syndic turn. Rebinding that
still-active selected turn would allow the new projection to claim history that has not converged
and could replay work. Syndic correctly rejects the mutation with `TurnLifecycleConflict`.

## Required Course Correction

Phase 11 proves that delivery-unknown validation is independent of the current binding head by
publishing an exact unbound successor and retaining the old CAS thread's immutable retirement
history. It does not weaken the no-replay guard to manufacture a fresh valid binding.

Later execution recovery must first converge the proven-dead turn to its designed source-less
incomplete outcome, then establish whatever fresh projection is valid for the resulting durable
history. That end-to-end sequence belongs to the later live-turn and restart integration phases.

## Detection

Focused Phase 11 remediation testing exposed this invalid ordering on 2026-07-15.
