# Catalog Maintenance During Initial Restoration

## Scope

The graph-owned Catalog source coordinator and selected-window initial restoration.

## Invalidated Assumption

Releasing Catalog maintenance with the existing CAS worker gate immediately after graph
publication is safe for initial restoration. Exact restoration readers and original claim
outcome adoption fence their source preparation against intervening Home commits. Catalog
repair creates legitimate competing commits before the native restore set publishes.

## Evidence

The canonical selected-window native case
`native_ordinary_failed_home_preserves_two_unsaved_residents_without_exit` reached its watchdog.
Startup control-point diagnostics reported `startup graph published; preparing restore`, then
`startup preparation failed: restore draft or coherent source revision changed`. The broader
app diagnostic repeatedly failed selected-window startup while threadless cases passed.
The interrupted broad run is diagnostic evidence, not accepted regression qualification.

## Correction

The owning App and Home contracts require a separate Catalog initial-publication fence created
before graph publication. Wakes coalesce and readiness stays unavailable until the whole native
restore set publishes. Failure/cancellation drains the fenced worker. Headless graph opening
keeps immediate release; recovery keeps its existing release after resident publication and
admission reopening. Preserve exact restoration validation and original command outcomes.

## Remaining Qualification

Phase 744 must verify the fence, actual selected native startup/recovery, cancellation and
ordinary graph disposal before acceptance.
