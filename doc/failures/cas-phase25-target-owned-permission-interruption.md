# Scope

Phase 25 permission-approval denial after exact app routing succeeds.

# Invalidated Approach

Queue the compact approval on the exact live-event target and let that target's consumer enqueue the
separately required permission-denial interruption after it observes the approval.

# Evidence

- Broker acknowledgement lets the backend write the denial and mark the shared response state
  `AutoDenied` before the target is required to consume its queue.
- Dropping `LiveEventTarget` closes the exact target and releases its queued operations.
- A queued permission approval can therefore be dropped after successful broker acknowledgement and
  denial but before its consumer requests `turn/interrupt`.

# Why It Failed

Target-queue consumption is presentation lifetime, while permission-denial interruption is a safety
obligation of the exact backend connection and turn. Treating target loss as sufficient settlement
leaves the CAS turn running after a permission denial even though the protocol-specific denial does
not itself interrupt. Retiring or invalidating only the UI target does not perform the required
backend operation.

# Course Correction

Exact route validation must create one bounded, already-authorized post-ack interruption obligation
owned by the broker/connection driver rather than by the droppable target event. Broker
acknowledgement unblocks the backend; the sole session owner writes the denial and marks
`AutoDenied`; only after the ordered poll or enclosing client request returns may the driver settle
the obligation. If the stop's sole attempt has not crossed a request byte, it dispatches once after
denial. If the approval arrived while that exact attempt was already awaiting its response, the
durable interrupting-approval cause joins the in-flight cut and the driver sends no second request.
A successfully auto-denied target-local approval result remains executable progress, while
denial-write or connection failure releases the volatile obligation but leaves durable stop
abandonment responsible for convergence. Later target drop may discard the presentation event but
cannot cancel this safety work or authorize safe reopen.

The bounded interruption slot uses a closing-reserved state. Whole-connection cancellation may
mark a reservation for closure after the approval has moved into exact routing, but it cannot
erase or reconstruct that obligation. The sole ingester still installs and returns the exact
completion once, after which connection failure releases the obligation.

Shutdown signals idempotent broker cancellation through the connection-owned handle before waiting
for the runtime mutex. Foreground calls may hold that mutex while the driver is blocked awaiting
the broker acknowledgement, so placing cancellation behind the mutex would deadlock the lifecycle
that must settle the ordered operation. Driver take, stop, and join remain mutex-serialized after
the independent cancellation signal.

Do not add a second transport writer, wait for target consumption before broker acknowledgement,
clone the approval request as interruption authority, or reroute the obligation to another target.

# Affected Authority

- `doc/plan.md`, Phase 25
- `doc/systems/bounded-resource-dataflow/design.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/beryl-app/doc/design.md`
