# CAS Phase 40 Unrestored Thread Unsubscribe

## Invalidated Approach

Treat the mounted recovery cancellation path as complete once its admitted page and both broker
rings release, without first exercising the fresh target's consuming abandonment cleanup.

## Evidence And Failure

The Phase 40 production-path test pauses after the exact recovery page is leased and before its
first durable read. Cancelling there correctly crosses the capacity-one reply ring as
`ThreadInjectionSourceError::Cancelled`, is classified as proven not dispatched with zero transport
bytes, and releases the page and both rings.

The coordinator then revokes the fresh target's local lease and calls
`ManagedBackendSession::unsubscribe_thread`. That production method unconditionally returns
`ResponseFamilyUnavailable { method: "thread/unsubscribe" }` in
`crates/beryl-backend/src/session.rs`. The final app result is consequently
`ProjectionExecutionError::AbandonmentFailed` whose primary cause is the correct typed
`InjectionNotDispatched` cancellation and whose cleanup cause is the unavailable response family.
No `thread/unsubscribe` request reaches the test CAS peer.

The incremental foreground response machine already supports `ResponseFamily::ThreadUnsubscribe`
and the closed `notLoaded`, `notSubscribed`, and `unsubscribed` result states. The generic bounded
request dispatcher also already owns exact request identity, response expectation, ordered-event
progress, timeout, rejection, and transport classification. Only the production request surface
and its focused lifecycle evidence remain unrestored.

## Why It Failed

Target abandonment and recovery-buffer release are separate obligations. Proving the latter while
silently accepting unavailable remote cleanup would leave source and package authority
contradictory: the CAS-live system requires consuming release to attempt bounded unsubscribe, and
the backend package contract claims that normalized operation is exposed.

This is an architectural restoration gap, not a flaky test or a reason to substitute connection
retirement, raw JSON, a compatibility shim, or a test-only cleanup route.

## Course Correction

The required correction was to restore the final bounded full-profile `thread/unsubscribe` request
through the existing response-family machine: remove the unconditional stub, add the closed request
params and typed result extraction to the production bounded-request path, cover exact statuses,
rejection, pre-dispatch failure, post-dispatch ambiguity, ordered interleaving, and app recovery
cancellation/abandonment, then rerun the Phase 40 residency and release gates with the blocking
cancellation test enabled.

## Resolution

Phase 40 restored `thread/unsubscribe` directly through the final bounded foreground request
dispatcher. The method now installs the exact response family, writes only the closed `threadId`
params, extracts one of the three normalized statuses, rejects request-only sessions before request
identity changes or bytes, and preserves the dispatcher's rejection, pre-dispatch, timeout,
interleaving, and connection-retirement semantics.

The focused backend request suite covers all three statuses, ordered progress, structured
rejection, proven pre-dispatch write failure, request-only rejection, and timeout after dispatch.
The mounted recovery cancellation case now observes the real unsubscribe request and returns the
original typed cancellation after exact consuming cleanup; the admitted page and both fixed rings
release completely. No connection-retirement substitute, raw request path, compatibility shim, or
test-only production route was added.

## Authority

- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/beryl-app/doc/design.md`
- `doc/plan.md`
