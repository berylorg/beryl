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

The later app-ingress implementation review exposed a second ordering failure in the accepted
correction. `ProviderObservationStager::seal(self, ...)` consumes and drops the old stager before it
returns `ProviderObservationStageOutcome::Indeterminate`. The app-owned `Ingester` can therefore
install returned custody only after the service-local stager has already been released, contrary to
the required registry-before-release cut.

The Phase 100 completion review exposed a third lifecycle failure. Ordinary `Drop` of uninstalled
home custody released its exact reservation, while dropping a store retained by a failed close
released the home lock and allowed same-process reopen despite an installed descriptor. Correct
recipient match arms and `must_use` warnings cannot make a freely droppable ownership graph retain
the sole descriptor or home lifetime.

## Why It Fails

Mapping the result to an existing rejection drops the descriptor and its reservation. Keeping it in
the disposable service does not survive acknowledgement or recovery. Executing reconciliation in
the handoff path also exceeds the exact-outcome boundary.

An app-only seal correction also cannot work. Installing custody inside the staging callback would
consume or erase the exact `CommandOutcome` before Syndic returns it, while returning the consumed
stager or a release guard changes the current Syndic outcome contract. Renaming the custody type or
delaying only the acknowledgement does not change the earlier drop.

Likewise, treating explicit `install` as a convention cannot work. Rust destruction remains a valid
control path after cancellation, unwind, service disposal, or ignored close failure; allowing that
path to release either the descriptor or lifetime lock makes the process-local registry advisory.

## Course Correction

Authority now requires `Indeterminate` to carry move-only custody containing the sole descriptor and
its pre-reserved slot/byte charge. The immediate `Ingester` recipient synchronously and infallibly
installs custody in the per-home home-store registry before acknowledgement, cancellation
observation, operation-state release, or service disposal. The registry uniquely owns custody
across service and Fjall generations; acknowledgements carry no descriptor.

Custody installation alone authorizes no reread, retry, rollback, publication, reconciliation hook,
worker, or execution. Phase 100 installs custody; Phase 102 consumes registry-owned custody through
the operation-scoped domain reconciliation boundary.

The Operator selected a seal-specific move-only custody guard. On seal `Indeterminate`, Syndic
returns that guard with the failure; the guard privately owns both the sole home-store custody and
the inert consumed stager. Its only terminal `install` operation installs home custody first and
drops the stager second. It exposes no stager, receipt, sealed handle, successor, retry, publication,
or reconciliation capability and is transferred synchronously to the app-owned `Ingester` rather
than retained by a service.

The Operator subsequently approved a non-discardable lifecycle correction. Explicit custody
installation remains the required recipient path, while custody destruction performs the same
infallible registry installation as a fail-closed fallback. Reserved custody retains the shared
registry and home-lock custodian directly; an installed descriptor-bearing scope self-retains that
bounded registry core. Dropping a store or failed-close value may release disposable Fjall and
service state but cannot unlock the home or permit same-process reopen while a scope remains. This
adds no reconciliation execution, background worker, or global registry.

## Resolution

Phase 100 proved explicit and drop-fallback installation, home-lock retention across ordinary store
and failed-close destruction, unique guard ownership, home-before-stager fallback ordering, and the
absence of reconciliation execution or a global keeper. Phase 102 separately proved bounded
operation-scoped natural-record reconciliation from registry-owned custody. The completed tests
`phase100_provider_observation_seal_custody` and `phase102_targeted_reconciliation`, together with
the home-store custody and targeted-reconciliation suites, close this failure record's remaining
risk. Current follow-on risk belongs to the deferred terminal-repair and fresh-service recovery
phases, not to this custody correction.
