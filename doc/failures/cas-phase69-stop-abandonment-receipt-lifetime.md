# CAS Phase 69 Stop Abandonment Receipt Lifetime

## Invalidated Approach

Remove the process-local stop entry immediately after durable abandonment publication.

## Evidence

The projection-loss path then observed the same incomplete turn without its stop receipt and tried
to publish generic abandonment again against an already changed binding.

## Why It Failed

Durable abandonment and authority-loss consumption are separate convergence steps. Deleting the
local receipt between them loses the proof that the stop coordinator already owns the incomplete
state.

## Required Course Correction

Keep a `DurablyAbandoned` marker until the exact router-loss path consumes it. Every abandoned
foreground-driver settlement retires the projection so that this consumption path necessarily
runs.

## Affected Work

Root `doc/plan.md` Phase 69, the stop coordinator, and connection-loss convergence own the
correction.
