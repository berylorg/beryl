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

## Registration Identity And Verification Follow-Up

Readiness inspection on 2026-09-23 found that already-active target registration retained only a
CAS turn ID; frozen witnesses derived the Syndic turn only from pending activation. Compaction
also owns a separate provider-turn identity. Exact outage inventory therefore cannot be derived
solely from pending activation or successful interrupt proofs. Registration must preserve the
already-held Syndic identity before failure; descriptive identity does not grant interrupt authority.

The registration correction passed independent review, 53 focused unit tests and six connection-work
tests. Provider verification then exposed artificial-pause deadlocks: `ForwardingHubSink::submit`
holds the forwarding mutex through fragment acknowledgement, while target disposal, connection
invalidation, broker snapshots and page diagnostics can require that same mutex. Moving disposal to
another thread and polling the router does not resolve this dependency. Capture independent metrics
and weak page observers before the pause; release staging before attachment-dependent operations.
This evidence does not establish a production deadlock without the artificial pause.

Receiver-only abandonment is not whole-target disposal: retaining the route permitted publication
(`source_event_count` was 2, expected 1; run `1bb64fc6-09d6-4866-97f2-6522bc9c3bb1`). The accepted
schedule releases staging, observes resumed Pong, and withholds the JSON suffix until full target
disposal. Cancellation likewise releases staging while the server keeps the observation unsealed,
then invalidates and joins the connection. Neither schedule claims cancellation during paused staging.

The old unknown-outcome case also expected healthy continuation after `AfterPersist`. That fault is
actually committed-with-later-failure; capture correctly closed rather than continuing (run
`d8e88038-dcb9-44c9-b284-21b1c190f16a`). Genuine indeterminate coverage now uses
`AfterCommitBeforePersist` separately for staging and source publication. Each test captures the sole
registry handle at terminal target closure, verifies released resources and retained custody after
connection retirement, and carries that same handle through consuming service close. The close error
retains the home until explicit post-retirement reconciliation resolves `ExactNew`. Staging remains
unpublished; the publication cut exposes the complete atomic item. There is no same-service retry.
Broker seal-ack counters include rejected replies and cannot prove successful publication.

Independent review accepted these corrections. All eight provider-residency cases passed in run
`16f81b20-7966-4799-b613-4a625c118538`; both strengthened same-handle custody cases passed again in
`5265a37a-3ac8-42de-8c40-1b139aa4e62f`. The weak observer retains no page storage. Formatting and
whitespace checks passed. Failed-store ingress and ordinary outage integration remain unimplemented;
these test repairs authorize no change to production failure semantics.

## Frozen Inventory Custody

Failure-target freezing originally disposed retained projections before returning descriptive
witnesses. Outage inventory needs a bounded handoff before that disposal. The frozen batch now owns
those projections and exposes borrowed witnesses; consumption, drop and unwind release local
projection custody exactly once outside the router lock. Interrupt eligibility is unchanged.

Do not collect every historical zero-worker batch to create a complete live inventory. Service
membership can retain historical connections after worker capacity is released; an externally held
router can still return an empty batch. Accumulating those batches has no worker-capacity bound.
Keep their immediate per-connection disposal and nondispatch accounting. Only worker-backed batches
may wait for complete inventory collection under the existing retained-worker bound. The proposed
coordinator accumulation change was removed during review.

The accepted tests retain historical routers while pausing a later live freeze and verify unchanged
connection reference counts. Real projection tests verify borrowed identity before disposal, exactly
one disposal after consumption, abandoned/unwinding batches and a later collection error with an
actual retained worker reservation. The initial 54-test failure run passed
`3de4dd21-363b-431d-a110-8e03916d656f`; the final six affected disposal/membership tests passed
`d8005d77-f93c-453a-92e5-4ed494dd7d5e`, and strengthened historical-capacity coverage passed
`219542fa-5581-4c87-b1da-1bae7bb9eb6e`. Independent review accepted the final bounded ownership.
The inventory publication and ingester transition remain outstanding; a descriptive witness borrow
alone does not mount outage capture or grant effect authority.

## Driver Polling Blocks The Ingester-Only Transition

The planned durable-to-transient ingester switch assumed that releasing its own live-command permit
would let the failure coordinator drain work and publish frozen inventory before acknowledgement.
Independent readiness review and root source inspection on 2026-09-23 invalidate that assumption:

- `connection/driver.rs::run_driver` retains a separate drain-counted `poll_permit` across
  `backend.poll_ordered_turn_stream_progress`. Backend `session/ordered_turn_stream.rs` passes the
  bound sink directly into synchronous receive/parsing. The ingester's acknowledgement therefore
  releases the driver poll, while the coordinator's `wait_until_drained` precedes target freezing.
  Waiting for inventory before that acknowledgement creates a cycle even after the ingester drops
  its own permit. The existing terminal reply breaks this cycle by ending capture.
- The driver's closed-gate persistent-failure branch dispatches already-authorized failure
  obligations or waits; it does not resume provider polling. An ingester mode alone cannot capture
  subsequent observations.
- `ForwardingHubSink::submit` holds its attachment mutex across acknowledgement. Obligation
  installation and `execute_terminal_shutdown` acquire that mutex; shutdown requests ingester
  cancellation only afterward. Inventory publication must precede attachment-dependent obligation
  installation, and cancellation needs a route that does not wait on the blocked submission.
  `AckSlot::wait` intentionally keeps waiting for exact operation custody even when cancelled.

This is a Beryl lifecycle prerequisite, not renewed CAS ordering rejection. The buffering exception
and frozen-batch custody remain valid. The accepted resolution separates passive receive from durable-command admission and never waits
for inventory before acknowledging ingress. Existing dispatched requests keep their exact permits,
outcome custody and timeouts. One bounded assembly slot may hold a validated seal; if inventory is
still unavailable when a new begin needs it, eviction records sticky connection loss. Compact facts
without inventory record loss without another queue. Exact cancellation is signaled before the
forwarding lock, and passive transport/schema failure cannot depend on healthy retirement admission.
The system resource policy and [app live projection](../../crates/beryl-app/doc/design-live-projection-and-scheduling.md)
and [app live capture](../../crates/beryl-app/doc/design-live-capture.md) own the accepted protocol.
Independent design review passed. Driver primitives, passive ingestion and ordinary publisher
mounting remain separate implementation gates; design acceptance alone does not enable capture.
The invalidated cycle was established by source inspection and independent review, not a reproduced
runtime hang. CAS and the bounded buffering exception remain unchanged.

## Preserve Qualified Loss Through Slot Handoff

Independent review of the pre-inventory slot found that making every assembly overflow a sticky
connection gap widened loss to unaffected sibling targets even when a valid trailing route and
ready inventory identified the exact target. Keep bounded observation loss in the assembly until
seal qualification. Only loss without usable inventory/routing becomes a conservative connection
gap; qualified retention failure must not widen it. The corrected ready-inventory overflow test
proves sibling isolation. The 36 assembly/retention tests passed, and independent review accepted
the private slot. Exact home/service/failure ownership remains the shared capture owner's duty.
