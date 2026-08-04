# Beryl-Home Publication Could Skip Retained Maintenance Health

## Scope

`beryl-home-store` state-dependent success publication during the final Fjall boundary cutover.

## Invalidated Approach

The health admission object exposed dependency-health observation and exact Beryl-generation
confirmation as separate callable operations. Most reads, writes, and domain lifecycle paths called
both in order, but receipt revision projection plus sidecar admission and verification called only
generation confirmation.

## Evidence

The Phase 43 independent completion review traced those three paths to `admission.confirm()` without
a preceding `Database::health()` observation. An autonomous Fjall maintenance terminal could
therefore remain retained only inside the database while a receipt-derived revision or retained
sidecar token was returned as current.

## Why It Failed

The cached Beryl gate and Fjall's retained maintenance terminal are distinct authorities. Checking
only the former proves that no already-surfaced Beryl failure or generation replacement intervened;
it does not surface a dependency terminal that arose autonomously.

## Course Correction

The two primitives are now private behind one package publication operation that always observes
the exact admitted Fjall database before confirming the exact Beryl generation. Every existing
state-dependent success path uses that operation. Receipt and sidecar errors retain explicit
storage-health provenance.

A non-production Fjall fixture installs an actual retained maintenance terminal without exposing a
raw database handle and deliberately leaves the Beryl gate healthy. Focused tests prove receipt
projection, sidecar admission, and sidecar verification each reject publication and move the store
to `verifying`.

## Durable Lesson

When publication depends on two health authorities, do not expose their checks as independently
optional steps. Encode their required order in one boundary and test omitted-call regressions with
the lower authority failed but the upper cache still healthy.

## Affected Authority

- `doc/systems/beryl-home-storage/design.md`
- `crates/beryl-home-store/doc/design.md`
- Root `doc/plan.md` Phase 43
