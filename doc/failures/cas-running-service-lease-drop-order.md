# Running-Service Lease Counts Must Follow Service-Owner Release

## Invalidated Approach

`RunningProjectionServiceLease::drop` decremented the process-slot lease count and notified the
recovery worker while its `Arc<ProjectionConnectionService>` remained an ordinary struct field.
The implementation assumed that returning from the custom `Drop` body and dropping that field were
indistinguishable from the waiting recovery worker's perspective.

## Failure

Rust drops ordinary fields only after the custom `Drop::drop` method returns. Recovery could wake on
the zero-count notification, consume the published epoch, and fail `Arc::try_unwrap` while the last
lease's service `Arc` was still alive. The Phase 84 two-cycle same-home test exposed this as an
intermittent terminal `EpochOwnership` failure during the second cut.

## Durable Rule

A synchronization count that authorizes unique ownership must not reach zero until the counted
owner has actually been released. Store the lease's service owner in an explicitly takeable field,
drop it first, and only then decrement the slot count and notify waiters. Do not repair this race by
retrying `Arc::try_unwrap` or by adding timing delays.
