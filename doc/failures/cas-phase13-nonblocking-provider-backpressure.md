# Scope

Phase 13 sequence item 5 provider-fragment backpressure during synchronous JSON-RPC request and
notification ordering.

# Invalidated Approach

Let `ProviderObservationSink::fragment` return a transient `WouldBlock` fragment to the synchronous
backend receive path, then surface that condition as an ordinary parse or polling result.

# Evidence

- The WebSocket payload reader, incremental JSON/schema state, current fragment, and an in-flight
  JSON-RPC request response currently live inside one synchronous receive operation.
- Returning `WouldBlock` without retaining that complete bounded continuation abandons the
  observation and loses the pre-response ordering cut. Retrying the request path cannot reconstruct
  already committed parser input or safely reread the notification.
- Busy retry would make no progress guarantee, while reading another transport byte would violate
  the fixed-buffer backpressure contract.

# Why It Failed

A transient nonblocking result requires a persistent request and transport continuation API across
every synchronous backend method. Item 5 does not need that additional scheduling model: each
stream-capable connection already has one dedicated worker, and the later capacity-one broker has an
independently progressing consumer.

# Course Correction

Make provider-fragment exchange a blocking ownership-transfer operation on the dedicated connection
worker. The bound sink must have an independently progressing consumer. It waits until the exact
fragment is accepted and an empty lease returns, or returns that same fragment with a typed terminal
timeout, cancellation, receiver-loss, or closure cause. The backend reads no further transport byte
while the exchange waits, and terminal failure abandons the unpublished observation exactly once.

# Affected Authority

- `crates/beryl-backend/doc/design.md`.
- `doc/failures/cas-phase13-serde-provider-ingress.md`.
- `doc/plan.md`, Phase 13 sequence items 5 and 7.
