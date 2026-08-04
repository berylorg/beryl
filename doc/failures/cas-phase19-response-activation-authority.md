# CAS Phase 19 Response Activation Authority

## Scope

Phase 19 completion of broker-only active CAS turn identity and `TurnActivated` publication when a
successful streamed `turn/start` response can be observed before the routed `turn/started` event.

## Invalidated Approaches

The first design let the ordinary capture caller reconcile a successful response and publish any
missing activation. A follow-up design moved that response-specific publisher into the broker but
still treated the response as a second way to create activation.

## Evidence

Backend streamed-input correlation rejects a successful response until both exact checked user
lifecycle echoes have been observed and their turn identity matches the response. Each checked echo
crosses the ordered provider broker before the connection-driver command can return the response to
its caller. The broker's first exact source permit already binds the turn and atomically publishes
active identity plus `TurnActivated`.

The ordinary fake server also hid this ordering fact: its preparation helper moved an explicitly
after-response `turn/started` action before the response. Once corrected, a fixture with both checked
echoes before the response and `turn/started` after it completed through the existing broker
activation without another publisher.

## Why It Failed

Response-side publication created a fallback authority for a fact already owned by the ordered
source lane. That duplicated the activation decision, weakened the single durable frontier, and
made ordinary capture responsible for storage and target state that it does not own. Moving the
same fallback behind a broker method changed its location but did not remove the duplicate trigger.

## Course Correction

The first checked source operation remains the only activation publisher. The connection-driver
command boundary now withholds an exact successful response until the broker proves that the exact
target, CAS turn, healthy home generation, and durable activation still agree. The proof waits for
an in-flight source publication, publishes nothing, and fails the target or connection closed on
identity, durability, loss, or generation mismatch. Ordinary capture receives only the classified
start outcome and has no activation permit, reconciliation, or publication path.

## Affected Authority

The corrected contract is controlled by `doc/systems/cas-live-syndic-transcript/design.md`,
`crates/beryl-app/doc/design.md`, and `doc/plan.md` Phase 19. The implementation boundary is
`connection/driver.rs`, `connection/target_command.rs`, the router activation proof, and the ordered
provider broker. Router tests cover waiting and fail-closed proof outcomes; ordinary integration
tests retain `turn/started` after the exact response and verify the broker-owned durable source
sequence.
