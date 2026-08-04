# Reason For Investigation

Phase 49 of the Beryl-home rework needed the exact Codex App Server 0.144.1
`turn/steer` request, response, rejection, and user-message ordering facts before defining Beryl's
public streamed steering boundary. In particular, the investigation had to determine whether the
request response encloses the echoed `UserMessage` lifecycle in the same way as `turn/start`.

# Outcome

## Request And Response

Pinned `TurnSteerParams` carries an exact `threadId`, optional `clientUserMessageId`, ordered
`input`, optional metadata/context fields, and a required nonempty `expectedTurnId`. The handler
loads the exact thread, validates and maps the input, calls `steer_input` with the expected turn
precondition and client message id, and returns `{turnId}`. A successful Beryl boundary can
therefore require the returned turn id to equal its expected active turn without reading any
history payload.

The optional client message id is passed through the core steering path. The pinned integration
test supplies a distinct id and later observes the same id and exact ordered input on the steered
`UserMessage` item.

## Response And Lifecycle Ordering

Unlike the pinned fresh `turn/start` path, accepted `turn/steer` does not enclose its complete
`UserMessage` lifecycle before the request response. The integration test receives and validates
the successful `{turnId}` response first, then waits for the matching `item/started` notification.
The steered input may wait for a later sampling step, so the lifecycle cannot be assumed to arrive
inside the response wait.

Consequently, a request-scoped comparator that is removed when the steering response arrives
cannot correlate the later user-message lifecycle. The pinned protocol supplies a bounded opaque
correlation channel through `clientUserMessageId`; exact content still requires replay against the
original source rather than trusting that id alone.

## Exact Rejections

`NoActiveTurn` and `ExpectedTurnMismatch` produce exact invalid-request responses with diagnostic
messages but no machine-readable data. `ActiveTurnNotSteerable` produces structured error data with
the closed `review` or `compact` turn kind. The exact response itself proves that the request was
rejected, but human-readable message text is not a stable machine verdict for distinguishing the
first two causes.

# Sources

- Canonical repository: <https://github.com/openai/codex>.
- Requested release: tag `rust-v0.144.1`; resolved commit
  `44918ea10c0f99151c6710411b4322c2f5c96bea`; inspected 2026-07-28.
- [`TurnSteerParams` and `TurnSteerResponse`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/turn.rs)
  establish the request and success-response fields.
- [`turn_steer_inner`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/turn_processor.rs)
  establishes validation, expected-turn enforcement, structured non-steerable data, unstructured
  no-active/mismatch errors, and success identity.
- [`turn_steer` integration tests](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/suite/v2/turn_steer.rs)
  establish response-before-user-message ordering and client-id/content echo behavior.
