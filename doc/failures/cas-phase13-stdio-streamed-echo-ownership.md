# Scope

Phase 13 paired bounded `turn/start` submission and echoed `UserMessage` lifecycle verification.

# Invalidated Approach

The transport design initially treated bounded outbound stdio buffering as sufficient to offer the
same streamed-input operation as the production WebSocket client while incremental echoed-input
verification was added above transport framing.

# Evidence And Failure

The retained stdio implementation owns stdout in a detached whole-line reader thread and forwards
materialized messages through a channel. It has no live managed-session constructor and cannot share
the exact request-scoped replayable-source verifier without moving stdout parsing and verifier
ownership across that thread boundary.

Allowing streamed `turn/start` on this path would make outbound text unbounded-safe while each CAS
`item/started` and `item/completed` echo remained subject to the whole-line allocation and ceiling.
That recreates the paired-boundary failure already recorded for whole-`Value` WebSocket ingress.

# Required Course Correction

- Support the specialized streamed-input operation only on the production WebSocket session whose
  incremental decoder shares exact request-scoped verifier ownership.
- Reject streamed `turn/start` on stdio before verifier installation, source reads, or any transport
  byte, and classify it as typed proven non-dispatch while keeping the session reusable.
- Keep generic bounded stdio outbound serialization intact for its retained compatibility tests.
- Before any future live stdio constructor can expose streamed input or generated-image admission,
  move stdout JSON parsing under equivalent session-owned incremental verifier and discard state.

This is an explicit capability boundary, not a fallback through whole-line echo parsing or a request
spool. Managed Beryl runtimes already use authenticated loopback WebSocket clients.

# Affected Authority And Proof

The correction is reflected in `crates/beryl-backend/doc/design.md`,
`doc/systems/cas-live-syndic-transcript/design.md`, root `doc/plan.md`, and the active rework tracker.
Focused tests must prove pre-byte rejection, reusable stdio authority, and unchanged WebSocket
streamed-input behavior.
