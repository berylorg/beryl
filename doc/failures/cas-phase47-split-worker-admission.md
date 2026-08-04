# Split Projection-Worker Admission

## Scope

The app-owned connection driver and provider ingester used by one foreground CAS projection
connection.

## Invalidated Approach

Acquire one local worker slot while constructing the provider ingester and acquire the second slot
later while constructing the connection driver.

## Evidence

- `crates/beryl-app/src/cas_projection/connection/provider_broker/ingester.rs` previously reserved
  its worker independently before spawning the ingester.
- `crates/beryl-app/src/cas_projection/connection/driver.rs` independently reserved the driver
  worker after the provider broker existed.
- Concurrent connection admissions can therefore each retain one slot while every candidate waits
  for or fails to acquire its required second slot.

## Why It Failed

One usable projection connection requires the worker pair. Admitting the two workers separately
allows partial candidates to consume the entire pool without any candidate having enough capacity
to become usable. A count limit would then reject progress even though no complete connection owns
the configured concurrency.

## Course Correction

The projection service owns one count-bounded worker pool and atomically acquires the complete
driver-and-ingester permit pair before either worker starts. Construction and spawn failures release
the pair, and the live workers retain one permit each until retirement and join.

## Affected Authority

- `crates/beryl-app/doc/design.md`
- `doc/plan.md` Phase 47
- Projection-service configuration and worker-pool tests

## Remaining Risks

- Later changes must not split pair acquisition across helper constructors.
- Failure and shutdown tests must prove that partial construction cannot leak either permit.
