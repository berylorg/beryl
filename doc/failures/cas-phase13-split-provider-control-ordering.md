# Split Provider And Compact-Control Ordering

## Context

Phase 13 originally combined two individually bounded mechanisms on one CAS connection:

- compact normalized controls were retained in the backend's fixed deferred-message FIFO while a
  synchronous request waited for its response;
- streamed provider observations blocked the connection reader while an independently progressing
  app broker staged their fragments in Syndic.

Each mechanism bounded its own residency, but they did not share one source-order completion
boundary.

## Invalidated Shape

Deferring compact controls while independently completing later provider work is invalid.

For example, CAS may send `turn/started`, then a large `item/started`, then the matching
`turn/start` response. If `turn/started` remains deferred while the provider sink stages or
publishes the item, the later provider effect overtakes the earlier turn identity. If the provider
sink instead waits for caller-side draining after the response, the connection reader cannot reach
that response and a second pre-response provider observation can deadlock behind the first sealed
handle.

A compact provider receipt in the deferred FIFO does not solve the problem. It either permits more
than one sealed observation to accumulate or holds the only broker slot while the connection reader
waits for a response that lies after the blocked observation.

Binding the ordered sink only for future reads is also insufficient. Initialization and
compatibility requests can already have placed compact controls in the bounded legacy FIFO. If
binding leaves those controls there and the ordered poll reads a newer transport message, the same
overtaking defect survives across the bind point.

Routing `turn/started` through the broker but acknowledging it after only enqueueing it to the old
target consumer is likewise insufficient. A following provider seal must finish before the matching
response becomes visible, while caller-side active-turn and activation publication waits for that
response. The provider consumer therefore cannot wait for durable activation without forming the
same response cycle, and it cannot publish first without overtaking the routed start.

Returning a live compact-control normalization error without closing transport is likewise invalid.
The malformed control has already been consumed, so permitting another poll would let a later
message cross a failed ordering point. Live normalization failure closes fail-closed just like
bind-time normalization failure.

## Follow-up Phase 18 Finding

The same defect survives after durable turn activation when checked streamed-user lifecycle
controls are acknowledged after only target-queue admission. The broker processes that compact
control before a later provider seal, but the legacy `LiveCapture` worker publishes it only after a
separate queue handoff. The provider consumer can therefore run while the older user-message start
or completion has no durable source event.

This is not repairable by reading the current source count and selecting the next sequence in
Syndic. Durable storage has no fact representing the acknowledged-but-unpublished target-queue
entry, so that selection can let the provider event overtake the correlation. Waiting for an
unspecified caller-side frontier is also not authority and can recreate the response cycle this
broker was introduced to remove.

Evidence is the synchronous acknowledgement in
`crates/beryl-app/src/cas_projection/connection/provider_broker/ingester.rs`, which routes every
compact control other than `turn/started` through the target queue, together with caller-owned
publication and local source frontiers in
`crates/beryl-app/src/cas_projection/ordinary/capture.rs`. The sealed consumer cannot observe that
local queued predecessor through `SyndicStorage`.

Publishing only the checked user-message predecessors is still insufficient. Once the sealed
consumer advances durable source state, caller-owned normal terminal or source-less loss
publication would use the same stale local frontier. A queued durable-publication receipt would
merely preserve two competing publishers behind an adapter.

The course correction requires one ordered-source ownership boundary. The broker durably publishes
checked user-message start, checked user-message completion, sealed provider observations, and
normal terminal control before acknowledgement; none of those source-producing operations enters
the target FIFO. Abnormal target loss converges only after any permit resolves and from the
then-current durable frontier. The ordinary caller retains no source sequence, revision,
publication-time, provider-event, or terminal authority.

## Terminal Cutover Phase-Boundary Finding

Separating compact-source ownership from sealed-provider publication is also invalid. With broker
terminal publication enabled while materialized provider events remained caller-published, the
broker committed terminal state before older target-FIFO provider events became durable. Later
provider publication could no longer advance the closed source frontier.

`cargo nextest run -p beryl-app --test phase13_ordinary_turn --locked` exposed nine failures before
fail-fast cancellation. Buffered-terminal cases retained only one of three expected source events,
assistant canonical items were missing after partial deltas, completion mismatch was not retained,
and reacquisition cases observed a provider-complete terminal where the lost provider prefix
required incomplete history.

The broker cannot wait for caller publication: that caller waits behind the response whose earlier
provider observation the connection reader must finish before exposing, recreating the prohibited
receipt cycle. The accepted replacement merges real sealed-observation consumption with compact
and terminal ownership. One independently progressing ordered ingester completes activation,
checked user lifecycle, sealed provider publication, and terminal publication in wire order. The
old materialized provider publisher is disconnected at that cutover and removed separately.

## Recovered-Home Phase 18 Finding

Pinning compact publication to the connection broker's admission-time home generation and
`SyndicStorage` handle is also invalid. A live loaded projection may survive a same-home recovery,
prove the recovered durable pending state, and rebind to the newer generation without replacing its
CAS connection. The focused
`generation_rebind::repaired_structural_sidecar_failure_rebinds_the_exact_live_projection` test
then observed generation mismatch followed by a foreign-domain read when broker publication used
the connection's stale authority.

The course correction is target-local. Target registration captures the rebound projection's exact
healthy home generation. Every activation or source-publication permit retains that generation,
checks it against the current healthy home, and only then reacquires the current typed Syndic handle.
Storage commands still verify the same expected generation, so a later recovery race fails closed.
The broker never performs recovery or substitutes its connection-admission generation.

## Replacement

One admitted backend connection is bound at construction to one exact Beryl home identity. Its
app-owned connection service retains the exact `Arc<HomeStore>` and only the narrowed shared-runtime
capabilities required by ingress. Generation-sensitive compact publication uses the exact target
permit generation and a same-generation reacquired typed Syndic handle rather than the connection's
admission-time handle.

The backend connection reader forwards compact controls and provider-observation operations through
one capacity-one, connection-ordered broker. One independently progressing ingester completes and
acknowledges each command before the backend advances later parser input, refills its fixed parser
window, or publishes a later response. That fixed window may already contain bounded read-ahead;
its admitted capacity is deliberate backpressure slack rather than a deferred semantic queue. The
sole fragment lease returns through one preallocated acknowledgement slot rather than a second
queue.

Sink binding synchronously normalizes and submits every bounded pre-bind compact control in original
FIFO order before it can succeed. Only after that FIFO is empty may the bound session read newer
transport bytes. A bind-time normalization or acknowledgement failure closes the connection
fail-closed; it never leaves a partially bound session that can overtake the retained prefix.

The broker owns at most one provider observation in building, sealed, or publishing state. Provider
seal stays inside this ordered boundary: it obtains one non-cloneable exact-target publication
permit, binds the sealed Syndic handle to the validated trailing route, and lets the final consumer
publish before acknowledging the seal. No provider receipt, sealed handle, or reconstructed provider
event enters a backend or target FIFO.

The provisional target carries the compact pending-turn authority established before dispatch.
When `turn/started` binds that target, the broker first publishes and reconciles the exact active CAS
turn plus `TurnActivated` source event without holding the router lock. Only then may it expose the
compact control to the old target consumer and acknowledge it. The old consumer can temporarily
reconcile the exact result, but it supplies no first-publication authority; its removal remains a
separate cutover boundary.

The app generates one durable 128-bit observation identity at begin and retains it through every
retry. An ambiguous store result crosses same-generation verification and an exact batch point-read:
`Next` completes, `Expected` retries the same operation, and `Conflict` fails closed. Neither
identity rotation nor generation-changing recovery is a broker workaround for unknown durability.

Target close and publication are one linearized race. A close that wins before permit acquisition
abandons the unpublished handle. A permit that wins prevents final target removal until publication
reports success or failure. No router lock is held during storage work, and a permit cannot be
cloned, reused, or redirected.

Broker cancellation is also an ownership handoff, not permission for the ingester to exit after an
empty timed receive. A submitter may already have passed its cancellation check and still enqueue
the sole operation. The ingester therefore remains alive until the backend-owned sender closes, or
receives and returns that raced operation through the acknowledgement slot before exiting. Closing
the acknowledgement slot while an operation can still arrive would strand a fragment lease and
turn ordinary shutdown into a waiter panic.

The home service cannot equate retired connection authority with released connection residency.
A stale admitted-session shell may still keep an already-retired driver, broker, page pool, and home
reference attached. The service registry retains every live attached shell until shutdown visits
and detaches it; pruning is allowed only after the shell is gone or its runtime has been released.
Otherwise explicit home close can miss an internal owner and degrade into an ownership-leak error.

If the transport fails after provider capture begins, abandonment records `TransportLost`; it does
not reuse the schema-failure reason merely because parser unwinding drops the capture. The transport
error still closes the connection, while the unpublished staging sink receives the exact lifecycle
reason and releases its current lease.

## Consequence

The active ordered-source cutover owns home-scoped service admission, narrowed capability
injection, the unified ordered broker, staging and reconciliation, target publication permits,
cancellation, shutdown, and the real atomic publication consumer. The following removal boundary
deletes the disconnected materialized provider event and caller-driven operational item/delta path.

The backend incremental grammar and Syndic unpublished staging remain valid. Only their previously
underspecified app integration and ordering boundary changes.
