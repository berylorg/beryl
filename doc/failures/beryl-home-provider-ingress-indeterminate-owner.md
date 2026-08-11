# Provider Ingress Indeterminate Custody Must Precede Acknowledgement

## Scope

Phase 100 of the Beryl-home rework, at the Syndic provider-observation staging boundary consumed by
`beryl-app`'s ordered provider broker.

## Invalidated Approach

Mechanically translate every `ProviderObservationStageOutcome` into the broker's existing applied
or rejected acknowledgement shape while preserving exact home-store mutation outcomes.

## Evidence

`ProviderObservationStager::{begin, control, fragment, seal}` can return
`Indeterminate { failure, reconciliation }`. The descriptor is move-only, retains the sole
operation reservation, carries no receipt, and authorizes no publication.

The consuming chain is `ProviderObservationStager` to `Ingester` to `BrokerReply` and `AckSlot`.
`BrokerReply` and `AckSlot` cannot retain the sole descriptor, while acknowledgement, cancellation,
operation-state release, or service disposal may outlive the staging object.

The focused `cargo check -p beryl-app --features test-faults` exposed the boundary while production
callers were adapted to the exact Syndic outcome API. The original target docs did not name a
descriptor-bearing terminal owner or handoff after an indeterminate staging result.

## Why It Fails

Mapping the result to an existing rejection drops the descriptor and its reservation. Keeping it in
the disposable service does not survive acknowledgement or recovery. Executing reconciliation in
the handoff path also exceeds the exact-outcome boundary.

## Course Correction

Authority now requires `Indeterminate` to carry move-only custody containing the sole descriptor and
its pre-reserved slot/byte charge. The immediate `Ingester` recipient synchronously and infallibly
installs custody in the per-home home-store registry before acknowledgement, cancellation
observation, operation-state release, or service disposal. The registry uniquely owns custody
across service and Fjall generations; acknowledgements carry no descriptor.

Custody installation alone authorizes no reread, retry, rollback, publication, reconciliation hook,
worker, or execution. Phase 100 installs custody; Phase 101 consumes registry-owned custody through
the operation-scoped domain reconciliation boundary.

## Remaining Risk

Source conversion is still incomplete. Phase 100 must audit every production `Indeterminate`
branch, prove unique registry custody and charge preservation, and rerun focused checks and fresh
completion review before closure.
