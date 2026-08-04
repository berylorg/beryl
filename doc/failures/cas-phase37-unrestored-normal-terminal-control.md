# Scope

Phase 37 planning for the final ordinary submitted-input fixed-residency proof.

# Invalidated Approach

Treat the production ordinary execution path as complete after streamed `turn/start` restoration,
then build a measurement-only harness around request replay, both checked user-message echoes, the
ordered broker, and durable publication.

# Evidence

- Full-profile ingress classifies `turn/completed` as an unavailable compact-control family in
  `crates/beryl-backend/src/incoming_json/provider/machine/classifier.rs`.
- That classification becomes `ForegroundIngressError::KnownControlUnavailable`, closes the
  WebSocket ingress path, and retires the connection.
- `OrderedTurnStreamOperation` has no normal terminal operation, and the app broker ingester has no
  terminal branch.
- `SourcePublicationPermit::finish_terminal` has no live production caller; only router tests reach
  it.
- `execute_ordinary_turn` returns a normal terminal outcome only after
  `LiveEventPoll::ProvenTerminal`. Connection loss instead converges an incomplete `StreamLost`
  outcome.
- The CAS-live design requires `turn/completed` to act as the ordered, durably published normal
  terminal fence, while the Beryl-home tracker incorrectly records that publication as complete.

# Why It Failed

An exact `turn/start` response proves successful input dispatch and correlation, but it does not
complete an ordinary execution. Treating subsequent source loss as success would bypass the absent
normal terminal architecture and falsify Phase 37's success and release evidence.

# Course Correction

Restore normal terminal control as an independent production acceptance boundary before resuming
submitted-input measurement:

- incrementally normalize the pinned `turn/completed` status and exact route;
- carry it through the sole capacity-one ordered broker;
- atomically publish the terminal source and binding effects with bounded item audit;
- call `SourcePublicationPermit::finish_terminal` only after durable success or exact
  reconciliation; and
- prove a raw-WebSocket ordinary execution reaches `OrdinaryTurnExecutionOutcome::Terminal`.

Do not use a test-only terminal, direct router call, synthetic injection, or source-loss outcome as
a substitute.

# Affected Authority

- `doc/plan.md`, new prerequisite Phase 37 and deferred residency Phase 38
- `doc/rework/beryl-home/REWORK.md`, Checkpoint 3 terminal-publication status
- `doc/systems/cas-live-syndic-transcript/design.md`
- `doc/systems/syndic-conversation-history/design.md`
