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
