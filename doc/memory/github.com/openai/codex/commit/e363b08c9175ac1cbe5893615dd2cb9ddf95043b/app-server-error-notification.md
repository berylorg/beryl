# Reason For Investigation

Beryl needs exact Codex App Server v0.146.0 evidence for handling the server `error` notification without confusing it with a JSON-RPC response error or treating a retryable stream error as terminal.

# Outcome

At requested tag `rust-v0.146.0` (annotated tag object `be449751a978f02e5bbba886999662956c7f38f5`, peeled commit `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`), `ErrorNotification` has `error`, `willRetry`, `threadId`, and `turnId` fields. `error` is a `TurnError`; its `codexErrorInfo` and `additionalDetails` fields are optional.

`willRetry = true` represents an intermediate stream error that app-server automatically retries and does not interrupt the turn. A status-affecting non-retryable error is stored in the turn summary, then copied into the failed turn emitted through `turn/completed`.

The inspected schema and emission paths correlate errors by thread and turn identifiers. They do not promise exactly-once delivery or global notification ordering.

# Sources

- Canonical repository: https://github.com/openai/codex. Requested tag `rust-v0.146.0`; `git ls-remote https://github.com/openai/codex.git 'refs/tags/rust-v0.146.0*'` resolved annotated tag object `be449751a978f02e5bbba886999662956c7f38f5` and peeled commit `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`. Accessed 2026-08-15.
- Protocol schema at `codex-rs/app-server-protocol/src/protocol/v2/notification.rs`, `ErrorNotification`; and `codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs`, `TurnError`. Inspected through https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/v2/notification.rs and https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs. Accessed 2026-08-15.
- App-server emission and completion handling at `codex-rs/app-server/src/bespoke_event_handling.rs`, `EventMsg::StreamError`, `handle_error_notification`, `handle_error`, and `handle_turn_complete`. Inspected through https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server/src/bespoke_event_handling.rs. Accessed 2026-08-15.
