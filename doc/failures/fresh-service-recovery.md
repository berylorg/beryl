# Fresh-Service Recovery Custody

## Terminal Close Error Escape

During phase 509 review, returning owned `HomeCloseError` from failed CAS retirement cleanup
invalidated the intended terminal-only custody boundary. With pending reconciliation that error
owns the failed store and exposes `into_open_store`; a caller could therefore reopen even though
runtime retirement had failed.

Keep the storage close error inside an opaque terminal-disposal owner. Expose no extraction of
the failed home or owned storage error after unsuccessful retirement. Verify pending reconciliation
combined with connection-join failure, including continued lock custody after the wrapper drops.
Successful retirement retains its separate exact handoff; ordinary storage recovery APIs remain
unchanged.

## Constructor Cleanup Evidence

Fresh candidate preparation cannot interpret a constructor error as proof that partial services
retired. The former constructor relied on destruction, which discarded cleanup outcomes. The
constructor now explicitly closes a partial service on returned failure and distinguishes
unconfirmed disposal while preserving the original construction error. Recovery preparation
withholds retry-home extraction in that case. Scheduler-start fault injection after partial
worker construction verifies both clean cleanup and failed cleanup, including disposal before
candidate abort. Internal panics still follow fatal process policy rather than in-process retry.
