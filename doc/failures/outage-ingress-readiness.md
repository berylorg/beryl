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

The implementation preserves these identities and passed independent semantic review plus 53
focused persistent-failure and approval-disposal unit tests. Six connection-work integration tests
also passed. The broader provider-residency run did not complete: diagnostic markers localized an
initial stall to `prove_target_abandonment`, after `barrier.wait_for_stage()` and before the return
of synchronous `harness.abandon_target()`.

Source inspection identifies the dependency: `ForwardingHubSink::submit` holds the forwarding
state mutex through `endpoint.sink.submit(operation)`, including the paused fragment acknowledgement.
Target disposal calls `current_router` through `ForwardingHub::current_attachment`, which requires
that mutex. Moving disposal to another thread and polling the router cannot resolve this: the
snapshot needs the same attachment mutex. The test thread must release staging before either can
complete. This evidence does not establish a production deadlock without the artificial pause.

Substituting receiver-only abandonment is not equivalent. With the registered route retained,
`provider_failures_and_unknown_outcomes_remain_atomic` completed in 3.435 seconds but failed the
no-publication assertion (`source_event_count` was 2, expected 1; nextest run
`1bb64fc6-09d6-4866-97f2-6522bc9c3bb1`). That substitution was removed. Do not weaken the assertion
or claim it proves whole-target disposal.

The accepted test correction releases staging, observes resumed Pong and withholds the JSON suffix
at the server until whole-target disposal. Post-disposal resource-zero and no-publication assertions
remain. Independent review accepted this schedule; four independently selectable receiver-loss,
target-abandonment, schema and fragment-failure cases passed in run
`1d0c8d73-1037-4526-b7ba-7df16d3c7999`. Alongside the 53 unit and six connection-work passes, this
accepts the registration prerequisite, not the full provider suite or outage mounting.

Moving the unknown-outcome test's acknowledgement snapshot before the pause exposes a separate
stale expectation: `AfterPersist` staging failure closes capture, while the test waits for Pong,
successful seal and continued publication on the same service. Run
`d8e88038-dcb9-44c9-b284-21b1c190f16a` failed in 7.025 seconds with provider closure. The current
ingester installs indeterminate custody then rejects; registry handoff grants no publication or
retry authority. Repair this test separately, preserving staging and publication fault coverage.
The paused transport case also needs a lock-order audit of snapshots and cancellation waits.
Both remain explicit before failed-store ingress implementation; no production continuation change
is justified by these stale test expectations.
