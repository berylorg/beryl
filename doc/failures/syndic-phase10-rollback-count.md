# Syndic Phase 10 Rollback Count Proof

## Scope

Checkpoint 3 Phase 10 native CAS projection selection.

## Invalidated Approach

Derive the `thread/rollback` `numTurns` argument by subtracting Syndic turn depths between the
currently represented prefix and the requested parent prefix.

## Evidence

- Syndic depth counts every immutable conversation-DAG turn.
- CAS rollback counts backend model turns, while provider-operation Syndic turns such as
  compaction need not create a corresponding CAS turn.
- The Phase 9 binding schema records the represented Syndic prefix and exact CAS turn
  correlations, but no cumulative native CAS turn position for that prefix.

## Why It Failed

The two counts agree for a simple sequence of ordinary model turns but are not the same semantic
quantity. Supplying the depth difference could roll CAS back past or short of the exact selected
parent, silently violating native-lineage and submitted-history authority.

## Course Correction

- Record the cumulative native CAS turn count represented by every usable binding.
- Advance it only when an exactly correlated CAS turn becomes terminal.
- Seed a fork from the proven source CAS turn position, preserve it across resume, and reduce it
  only through an exact rollback.
- Derive rollback distance from those CAS-native counts. If the exact position is absent or the
  backend's bounded argument cannot represent the distance, native rollback is unprovable and
  recovery policy decides the next step; never estimate from Syndic depth.
- Permit an exact rollback to the empty native prefix, whose native CAS turn count is zero.

## Affected Authority And Proofs

The correction affects Phase 10 in `doc/plan.md`, the CAS-live Syndic system contract,
`syndic-storage` binding and CAS-turn correlation records, reopen validation, and focused
coordinator tests.
