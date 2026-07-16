# CAS Injection Authority Loss Is Not Transport Loss

## Invalidated Approach

The injection outcome classifier reused `ManagedBackendError::invalidates_connection_authority`
to decide whether a failed `thread/inject_items` request was `TransportLost`.

## Why It Failed

Connection invalidation is a broader safety decision than transport classification. A request
timeout, malformed matching response, or response-deserialization failure invalidates the loaded
connection because its state is no longer authoritative, but none of those facts proves that the
transport itself was lost. In particular, a timeout after request dispatch is completion-unknown.

Conflating the predicates made the same post-dispatch timeout classify differently depending on
whether it occurred in injection or another non-idempotent request boundary.

## Required Course Correction

Injection preserves its accepted four outcomes: success, exact structured rejection, actual
transport loss, and completion-unknown. Only concrete write/read/closed/process/WebSocket
transport failures enter `TransportLost`. Timeout, invalid JSON, response-shape failure, and other
non-transport authority loss enter `CompletionUnknown`.

Every unsuccessful outcome still consumes the fresh-idle target and prohibits in-place retry, so
this correction changes diagnostic truth without weakening recovery safety.

## Detection

The Phase 12 package-wide backend verification exposed the stale classifier through the retained
wrong-response-id timeout proof.
