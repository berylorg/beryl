# Reason For Investigation

Checkpoint 3 Phase 13 needs to know whether Codex App Server 0.144.1 may treat its
status-only `turn/completed` notification as the ordered end of the preceding live item
lifecycle stream. The question is specifically about one continuously subscribed healthy
connection, not replay after subscription, recovery after transport loss, or client-side
processing order.

# Outcome

## Direct Answer

Yes for a normally finishing ordinary turn in the pinned source, with important scope limits.
Every same-thread item lifecycle event that precedes core `TurnComplete` is translated and
enqueued to each currently subscribed connection before the status-only `turn/completed`
notification. The per-connection writer then writes those queued messages sequentially.

This is release-specific source behavior, not an explicit stable protocol promise. The public
documentation describes streaming item events and then finishing with `turn/completed`, and calls
`item/*` the source of truth, but does not state a normative FIFO or terminal-barrier guarantee.

## Exact Ordering Chain

- Core `Session::emit_turn_item_started` and `Session::emit_turn_item_completed` await
  `Session::send_event`, which persists and then awaits insertion into the thread's unbounded
  `async_channel` event queue.
- A regular task awaits `run_turn`; sampling drains its ordered in-flight tool futures before it
  returns. The shared task runner then flushes the rollout and only afterward
  `Session::on_task_finished` awaits emission of `EventMsg::TurnComplete`.
- One listener task per loaded core thread calls `conversation.next_event()` and awaits
  `apply_bespoke_event_handling` before reading the next event. Listener commands may interleave,
  but a second task does not fan out the same thread's core events.
- The `ItemStarted` and `ItemCompleted` branches await
  `ThreadScopedOutgoingMessageSender::send_server_notification`. The `TurnComplete` branch is
  reached later and awaits `handle_turn_complete`, which emits `TurnCompletedNotification` with
  `items = []` and `itemsView = "notLoaded"`.
- All of those notifications use the same `OutgoingMessageSender` channel. The single outbound
  router receives envelopes in queue order and enqueues them to one FIFO writer channel per
  connection. Stdio writes one complete newline-delimited JSON message at a time; WebSocket and
  control-socket transports send one queued text frame at a time.
- Multiple subscribed connections are fanned out sequentially. Other threads and global messages
  may interleave, but they do not reverse the relative order of one thread's awaited sends for a
  given connection. There is no synchronized delivery instant across different connections.

The detached response handlers in bespoke handling do not create a later competing translation
path. In the synthetic command-decline path, item completion is enqueued before the approval `Op`
is submitted back to core; core cannot finish that turn first. Terminal cancellation resolves a
still-pending handler with the turn-transition error, and that handler returns without emitting a
synthetic completion.

## Enqueue, Write, Receipt, And Processing

Ordinary notification sends await channel admission, not transport write completion. A separate
`send_server_notification_to_connection_and_wait` method exists, but item and turn lifecycle paths
do not use it. Consequently:

- CAS queue order and the healthy connection writer's write order are established.
- CAS does not wait for or acknowledge client receipt, JSON parsing, durable admission, or UI
  processing before it enqueues `turn/completed`.
- A client that reads and processes the ordered stdio/WebSocket stream serially observes the item
  notifications before `turn/completed`; a client that dispatches received messages concurrently
  must impose its own processing order.
- The conclusion requires uninterrupted subscription with the relevant methods not opted out.
  Subscription added mid-turn does not replay earlier notifications. WebSocket queue exhaustion
  disconnects the slow connection, while stdio backpressures; neither is the healthy case.

## Related But Different Ordering

`apply_bespoke_event_handling` calls `ThreadWatchManager::note_turn_completed` before it emits
`turn/completed`. That status mutation may enqueue `thread/status/changed` with `idle` first. This
does not weaken item-before-turn ordering, but clients must not use thread idle as a substitute for
the turn terminal event or infer a general cross-notification contract from it.

## Tests And Contract Strength

Focused source tests verify that the terminal notification is status-only. Core item tests include
a plan-stream case that records event indices and asserts both item completions precede
`TurnComplete`; app-server integration tests also consume particular item completions before the
corresponding `turn/completed`. Transport tests verify FIFO backpressure behavior for the stdio
writer queue. No inspected test or protocol comment states the broader ordering rule as a stable
public contract for arbitrary future releases.

One bounded risk remains outside the normal Phase 13 completion claim. Forced interruption waits
for graceful task shutdown, but after its timeout calls `JoinHandle::abort()` without awaiting the
join before emitting core `TurnAborted`, which app-server also maps to `turn/completed`. The source
does not explicitly prove that an already in-flight item send cannot finish across that forced-abort
boundary, and no focused test was found for that race. This does not block Phase 13 ordinary
successful/failed completion; Phase 15 interruption should retain fail-closed late-event handling
and add an exact-target regression proof before treating interrupted `turn/completed` as an
absolute no-later-item barrier.

## Phase 13 Recommendation

Use the exact 0.144.1 healthy-stream order for normal ordinary-turn capture: ingest notifications
serially, durably admit every earlier item event, and then reconcile the status-only
`turn/completed`. Keep transport loss, unsubscribe, opt-out, queue failure, and connection
replacement on the existing incomplete or unknown-terminal paths. Do not replace the terminal
event with idle status, and do not claim that CAS has waited for Beryl persistence merely because
it enqueued or wrote the terminal notification.

## Exact Pinned Terminal Wire Order

Follow-up source inspection for Checkpoint 3 Phase 37 established the serializer order needed by
an incremental closed grammar. `TurnCompletedNotification` writes `threadId` and then `turn`.
Inside `turn`, the pinned `Turn` serializer writes these fields in order:

1. `id`
2. `items`
3. `itemsView`
4. `status`
5. `error`
6. `startedAt`
7. `completedAt`
8. `durationMs`

The production terminal emitter supplies `items = []` and `itemsView = "notLoaded"`. Terminal
status is `completed`, `interrupted`, or `failed`; `inProgress` belongs to the shared `Turn` schema
but is not a terminal value. Successful and interrupted terminal production carries no error,
while failure carries the last turn error. `startedAt`, `completedAt`, and `durationMs` are each a
nullable signed 64-bit integer. The protocol type adds no nonnegative constraint.

When present, the error object serializes `message`, `codexErrorInfo`, and `additionalDetails` in
that order. The pinned closed `codexErrorInfo` vocabulary is context-window exhaustion, session
budget exhaustion, usage-limit exhaustion, server overload, cyber policy, HTTP connection failure,
response-stream connection failure, internal server error, unauthorized, bad request, thread
rollback failure, sandbox error, response-stream disconnection, too many failed response attempts,
an active turn that is not steerable (`review` or `compact`), and `other`. The HTTP and response
stream variants carry an `httpStatusCode` field whose value is an unsigned 16-bit integer or null.
This enum is externally tagged: unit variants serialize as camel-case strings, while data-bearing
variants serialize as a one-key camel-case outer object containing their payload object. The active
turn variant similarly contains a required `turnKind`. No `type` field is present. No terminal item
snapshot is available from this wire shape.

# Sources

- OpenAI `codex` repository, canonical remote `https://github.com/openai/codex.git`, requested tag
  `rust-v0.144.1`, tag target and full commit
  `44918ea10c0f99151c6710411b4322c2f5c96bea`, clean checkout verified 2026-07-16 with
  `git rev-parse HEAD`, `git remote get-url origin`, `git tag --points-at HEAD`, and
  `git status --short`.
- Core production and queue path: `codex-rs/core/src/tasks/regular.rs` (`RegularTask::run`),
  `codex-rs/core/src/session/turn.rs` (`run_turn`, `try_run_sampling_request`, `drain_in_flight`),
  `codex-rs/core/src/tasks/mod.rs` (`Session::start_task`, `Session::on_task_finished`,
  `Session::handle_task_abort`), and `codex-rs/core/src/session/mod.rs` (`Session::send_event`,
  `send_event_raw_with_persistence`, `deliver_event_raw`, `emit_turn_item_started`, and
  `emit_turn_item_completed`).
- App-server translation path: `codex-rs/app-server/src/request_processors/thread_lifecycle.rs`
  (`ensure_listener_task_running`), `codex-rs/app-server/src/bespoke_event_handling.rs`
  (`apply_bespoke_event_handling`, `handle_turn_complete`,
  `emit_turn_completed_with_status`, and approval response handlers),
  `codex-rs/app-server/src/thread_state.rs`, and `codex-rs/app-server/src/thread_status.rs`.
- Outbound queue and transport path: `codex-rs/app-server/src/outgoing_message.rs`
  (`ThreadScopedOutgoingMessageSender`, `OutgoingMessageSender`),
  `codex-rs/app-server/src/lib.rs` (single outbound router task),
  `codex-rs/app-server/src/transport.rs` (`route_outgoing_envelope`), and
  `codex-rs/app-server-transport/src/transport/{stdio.rs,websocket.rs,unix_socket.rs}`.
- Protocol shape and tests: `codex-rs/app-server-protocol/src/protocol/v2/turn.rs`,
  `codex-rs/app-server/src/bespoke_event_handling.rs` test
  `test_handle_turn_complete_emits_completed_without_error`,
  `codex-rs/core/tests/suite/items.rs`, `codex-rs/app-server/tests/suite/v2/turn_start.rs`, and
  `codex-rs/app-server/src/transport_tests.rs` test
  `to_connection_stdio_waits_instead_of_disconnecting_when_writer_queue_is_full`.
- OpenAI, [Codex App Server](https://developers.openai.com/codex/app-server), accessed 2026-07-16.
  The turn workflow describes streaming events followed by `turn/completed`, and the item section
  identifies `item/*` notifications as source-of-truth lifecycle events, but neither section states
  an explicit normative ordering guarantee.
