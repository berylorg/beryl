# Outage Ingress Readiness

## Scope

Connecting the accepted transient outage buffer to ordinary store failure and service retirement.

## Invalidated Assumption

The private retention component can be connected directly to the ordered ingester as an
integration of already accepted components.

## Evidence

- `crates/beryl-backend/src/provider_observation.rs` supplies schema-only
  `ProviderObservationBegin` and leased field fragments. The validated route follows the body.
- `crates/beryl-app/src/cas_projection/connection/provider_broker/ingester/operations/publication.rs`
  acquires the exact route publication permit in `seal`, after staging the body.
- `crates/beryl-app/src/cas_projection/connection/provider_broker/ingester/core.rs` stops
  `run_loop`, cancels ingress and closes acknowledgement on persistent store failure.
- The [accepted buffer contract](../../crates/beryl-app/doc/design-live-capture.md#outage-buffer)
  accepts complete qualified fields and explicitly excludes fragment assembly and pre-route
  attribution. Its only current external source references are feature-gated test exports.

## Why It Blocks

The buffer cannot accept a provider fragment as a complete field or select its owning turn before
the route arrives. Keeping the existing terminal failure acknowledgement also prevents subsequent
outage capture. A bounded normalizer and capture lifetime are missing prerequisites, rather than
ordinary wiring. This does not establish that bounded outage capture is impossible.

## Required Resolution

Specify ownership and aggregate limits for unpublished observations before route validation, the
loss policy when a route never arrives, and the bounded capture-to-retirement cut. Define how that
capture proceeds behind the durable admission fence without restoring durable command authority.
Then derive and accept the missing component separately before returning to phase 479.

One bounded unpublished observation scratch owner is a candidate for review, not an accepted
design. Do not guess a target, retain raw operations and their capabilities, bypass home
reconciliation custody, or transfer outage facts to replacement. The existing
[durable witness decision](repair-outage-witness-custody.md#accepted-resolution) remains unchanged.

The Operator subsequently directed checking current CAS before proposing buffering. The
[0.154.0 release-source investigation](../memory/github.com/openai/codex/commit/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/lifecycle-route-wire-order.md)
confirms lifecycle item-before-route serialization. Decoder reordering cannot obtain unread
identities. The scratch-owner recommendation is withdrawn pending resolution of that producer
constraint; implementation is stopped as instructed. Text deltas use a different, route-first
shape and do not establish lifecycle readiness.
