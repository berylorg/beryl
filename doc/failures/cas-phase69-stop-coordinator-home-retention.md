# CAS Phase 69 Stop Coordinator Home Retention

## Invalidated Approach

Let the process-owned stop coordinator retain a strong `Arc<HomeStore>` for its lifetime.

## Evidence

Complete `beryl-app` library nextest coverage failed existing home-generation retirement cases with
`HomeOwnershipLeaked` after the coordinator was mounted in `ProjectionConnectionService`.

## Why It Failed

The coordinator outlived individual operations and therefore became an unintended owner of the
healthy home generation. Process service lifetime is broader than home-generation authority.

## Required Course Correction

The coordinator retains `Weak<HomeStore>` and upgrades it only for an operation-scoped authority
read or mutation. A failed upgrade is an authority-loss error, not a reason to preserve the home.

## Affected Work

Root `doc/plan.md` Phase 69 and the `beryl-app` stop coordinator own the correction.
