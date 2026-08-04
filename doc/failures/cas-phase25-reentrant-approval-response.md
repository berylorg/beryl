# Scope

Phase 25 compact approval handling on a provider-capable foreground connection.

# Invalidated Approach

Route an idle approval through the capacity-one ordered broker, enqueue it on the exact target, and
have that target send a normal response command back to the connection driver that owns the backend
session.

# Evidence

- `connection/driver.rs::run_driver` calls
  `ManagedBackendSession::poll_ordered_turn_stream_progress` while exclusively owning the session.
- Incremental ingress calls `BrokerSink::submit`, which blocks until the independent ingester returns
  the sole acknowledgement for that exact operation.
- A target response command uses the same driver's bounded command channel and cannot execute until
  the current poll and sink submission return.

# Why It Failed

If the broker acknowledgement waits for the target's approval response, and the target response
waits for the connection driver, the driver is waiting on work that only it can perform. A second
writer, cloned session, direct transport handle, or detached response worker would duplicate exact-
session response authority and violate the ordered connection boundary.

# Course Correction

The independently progressing ingester must return the exact bounded approval routing or policy
result through the current synchronous operation completion. The backend session that already owns
the connection then performs the sole denial write, changes shared response state only after that
write succeeds, and does not advance parser input first. Target loss, broker cancellation, and
receiver loss must return an explicit ownership-preserving result rather than silently enqueueing or
dangling a response-required request. Any separately required exact interruption may be scheduled
only through a seam that does not wait on the blocked sink submission.

Do not add a second transport writer, session clone, response thread, raw request spool, or target-
to-driver command cycle to make approval responses appear asynchronous.

# Affected Authority

- `doc/plan.md`, Phase 25
- `doc/systems/bounded-resource-dataflow/design.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/beryl-app/doc/design.md`
