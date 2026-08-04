# CAS Phase 70 Completion-Unknown Connection Authority

## Invalidated Approach

Retiring app-side connection authority after a completion-unknown primary interruption only when
the attached error matched a generic transport-invalidating predicate was initially treated as
sufficient because backend session logic already retires completion-unknown requests.

That boundary is invalid. A valid JSON-RPC error can still leave interruption dispatch completion
unknown while failing the generic transport predicate. Keeping the router and registry briefly
authoritative after that classification permits stale projection work on a connection whose exact
operation outcome is no longer knowable.

## Correction

Every `CompletionUnknown` primary stop disposition unconditionally invalidates the app connection
and converges through authority loss. Only `ProvenNotDispatched` retains error-specific retirement,
because that class proves no request byte crossed and may safely preserve the same connection when
the concrete error permits it.
