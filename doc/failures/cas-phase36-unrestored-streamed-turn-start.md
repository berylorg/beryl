# Scope

Phase 36 planning for the Beryl-home Checkpoint 3 submitted-input fixed-residency proof.

# Invalidated Approach

Treat the real production streamed `turn/start` path as already composed and add only a
measurement harness plus content-free test seams.

# Evidence

- `ManagedBackendSession::start_turn_with_streamed_input_options` reaches the sole
  `non_idempotent_streamed_turn_start` implementation in
  `crates/beryl-backend/src/session.rs`, which unconditionally returns proven non-dispatch with
  `ResponseFamilyUnavailable { method: "turn/start" }`.
- `WebSocketClientTransport::write_streamed_message` and `StreamedTurnStartParams` are final
  bounded components but have no production caller.
- The app connection worker and capacity-one source broker reach the backend session method, so a
  service-level harness would stop at that stub before source replay, transport dispatch, verifier
  installation, either lifecycle echo, or response classification.
- The ordered turn-stream operation union has no checked submitted-user lifecycle variants;
  correlated user messages currently take the unavailable-control path, and the app ingester has
  no first-echo activation publisher.
- The existing backend pre-dispatch test still names `turn/start` as an unrestored cutover gap.

# Why It Failed

The Phase 36 acceptance boundary requires a real WebSocket dispatch and both incremental echo
passes, but the production composition that installs the request-scoped verifier, writes the
streamed request, waits through ordered progress, classifies the non-idempotent result, and removes
the verifier does not exist. Restoring that path changes production availability and is an
independently implementable, verifiable, reviewable, and resumable boundary; it is not a
feature-gated diagnostic seam.

# Course Correction

Stop the measurement-only phase before source edits. Replan the unrestored full-profile streamed
`turn/start` composition as its own bounded production phase, retaining the deliberate stdio
non-dispatch gate and exact unknown-outcome semantics. Run the fixed-residency harness only after
that restoration passes its own verification and independent review.

# Remaining Risk

The restoration must preserve one response expectation, one request-scoped verifier, exact
request-id progression, synchronous ordered handling of both user-message echoes, activation before
response exposure, atomic durable publication of activation and checked user lifecycles, and
fail-closed cleanup for every pre-dispatch and possible-dispatch outcome.
