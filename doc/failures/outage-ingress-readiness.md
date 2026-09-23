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

## Accepted Resolution

The Operator directed checking current CAS before proposing buffering. The
[0.154.0 release-source investigation](../memory/github.com/openai/codex/commit/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/lifecycle-route-wire-order.md)
confirms lifecycle item-before-route serialization. Decoder reordering cannot obtain unread
identities. Text deltas use a different, route-first shape and do not establish lifecycle readiness.

On 2026-09-23 the Operator selected continued CAS use, authorized necessary ordering-related
buffering, and resumed the rework; CAS replacement is deferred until rework completion. The
[CAS-live system](../systems/cas-live-syndic-transcript/design.md#durable-store-outage-buffer) and
[app capture contract](../../crates/beryl-app/doc/design-live-capture.md#outage-buffer) now define
bounded unpublished assembly, exact or conservative connection-scoped loss attribution, passive
outage ingestion, and full disposal before replacement. Healthy capture keeps Syndic staging.
The [durable witness decision](repair-outage-witness-custody.md#accepted-resolution) remains unchanged.

Assembly and the failed-store ingestion mode require separate acceptance before ordinary outage
integration. Buffered data cannot guess a target, retain request capabilities, bypass reconciliation
custody, become canonical history, or transfer to a replacement service. The remaining risk is
implementation of those transition and disposal cuts, not a requirement to fix CAS producer order.
