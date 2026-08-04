# CAS Phase 69 Claim Collision Is Not Nondispatch

## Invalidated Approach

Converge every failed or mismatched dispatch-claim reconciliation through the safe-reopen path.

## Evidence

Independent Phase 69 review found that a claim collision or post-claim witness mismatch reached
`settle_proven_nondispatch`, which reread the current durable operation without authenticating its
state and attempt against the coordinator's local authority.

## Why It Failed

A collision does not prove that no dispatch occurred. Safe reopening a claim owned by another or
unresolved attempt could consume the stopping barrier and permit a repeated interruption.

## Required Course Correction

The coordinator retains an explicit claim-unresolved state. Safe reopen requires both an exact
local unclaimed admission and durable `Admitted` state, or the exact locally generated attempt in
both local and durable claimed state. Claim collisions and reconciliation mismatches remain
fail-closed until terminal or authority-loss convergence.

## Affected Work

Root `doc/plan.md` Phase 69, the `beryl-app` stop coordinator, and its exact-authority regression
coverage own the correction.
