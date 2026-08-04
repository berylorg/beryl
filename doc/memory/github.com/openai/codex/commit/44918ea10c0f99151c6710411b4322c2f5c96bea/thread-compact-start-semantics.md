# Reason For Investigation

Phase 71 needs release-pinned authority for `thread/compact/start` before Beryl restores its
context-compaction implementation. The missing facts are request/session ownership, acknowledgement
timing, turn-id correlation, lifecycle and thread-status order, active/unloaded behavior,
interruption, failure and retry behavior, and whether the standard terminal turn notification is
enough to prove the exact compaction reached a later non-active boundary.

The source instance is the official OpenAI Codex repository at commit
`44918ea10c0f99151c6710411b4322c2f5c96bea`, the commit peeled from tag `rust-v0.144.1`.

# Outcome

## Direct Answer

`thread/compact/start` is an enqueue acknowledgement, not a start or completion response. The
handler resolves an already loaded core thread, creates and submits `Op::Compact`, then returns
`{}` after the core submission channel accepts the operation. The response contains no turn id and
the handler does not wait for core to handle the operation, create the task, emit `turn/started`, or
complete compaction.

The core submission id is created as a UUIDv7 before channel submission and becomes the public
compaction turn id. App-server discards that id from the `{}` response. The first standard wire
exposure is `turn/started`; `item/started` then repeats it as `turnId` and carries a separate UUIDv7
context-compaction item id.

For one exact successful compaction on a continuously subscribed healthy stream, the relevant
notification order is:

1. `thread/status/changed` to `active`.
2. `turn/started` with the generated compaction turn id.
3. `item/started` for `contextCompaction` with that turn id.
4. `item/completed` for the same item id after replacement history is installed and token usage is
   recomputed.
5. Possible non-lifecycle messages, including the local-compaction warning.
6. `thread/status/changed` to `idle`.
7. `turn/completed` with the same turn id and status `completed`.

The `{}` response has no source-enforced position within that notification sequence. It is sent
after operation enqueue, but core and the per-thread listener run concurrently and can emit
lifecycle messages before the request task enqueues the response. The focused app-server test
reads until it finds the response and buffers nonmatching messages, so its later item assertions do
not establish response-before-notification wire order.

An exact successful `turn/completed` is sufficient release-pinned proof that app-server already
crossed the later idle boundary for that compaction. `note_turn_completed` mutates and publishes
idle before `handle_turn_complete` sends the terminal notification. A client need not wait for an
additional idle notification after the exact successful terminal. This is an instantaneous ordered
boundary, not a promise that no later request can start another turn.

That conclusion does not apply uniformly to failed terminals. A final core `Error` makes the
thread status `systemError` before the error notification. The later `TurnComplete` clears running
facts but preserves `has_system_error`, so `turn/completed` status `failed` follows `systemError`,
not idle. A failed terminal proves that the exact task ended, but it is not exact idle proof.

## Request And Session Ownership

The initialized request is placed in app-server's exclusive per-thread request queue. That queue
serializes request handlers across connections only until each handler has sent its response; it
does not hold the thread key until the asynchronous compaction task terminates.

The `ConnectionRequestId` supplies response routing and request trace context. `load_thread` then
uses the process-wide `ThreadManager` to obtain the loaded `CodexThread`; that thread owns the core
submission channel and task. The response is sent only to the requesting connection.

Lifecycle ownership is different. One listener task consumes the loaded thread's core event queue,
and for every event it snapshots the thread's currently subscribed connection ids. Typed
`turn/*` and `item/*` notifications go only to those subscribers. `thread/compact/start` does not
subscribe its caller or start a listener. `thread/start` and `thread/resume` do auto-attach their
calling connection, so a fresh Beryl maintenance client must resume/subscribe on that connection
before starting compaction if it intends to await lifecycle evidence there.

If the thread-id string is invalid, the handler rejects with invalid-request code `-32600` and
`invalid thread id: ...`. If the UUID is valid but the thread is absent from the live
`ThreadManager`, it rejects with `-32600` and `thread not found: <id>`; it does not load persisted
state. Submission-channel failure produces internal-error code `-32603` with
`failed to start compaction: ...` and no `{}` acknowledgement.

## Active Turns, Replacement, And Restart

There is no active-turn rejection in `thread_compact_start_inner`. Core's generic `spawn_task`
first calls `abort_all_tasks(Replaced)`, then starts the new `CompactTask`. Consequently:

- Starting compaction while an ordinary turn is active replaces that turn. The old turn emits an
  interrupted terminal (reason `Replaced` is not exposed in the v2 terminal status) before the new
  compaction turn starts.
- Starting a second compaction while the first is active replaces the first, whose terminal status
  is `interrupted`, and creates a fresh UUIDv7 turn id for the second.
- The replacement path may wait up to the 100 ms graceful-interruption window before forcibly
  aborting the old task and starting the new one.
- Same-thread request serialization does not prevent a later request from being handled while the
  compaction task is running, because the compact request releases its queue slot after `{}`.

After a failed terminal the core active task has been cleared and the thread remains loaded, so a
new compact request can be submitted. Its later `TurnStarted` changes `systemError` directly to
`active` and clears the system-error fact; there need not be an intervening idle notification.
Retry is not an idempotency proof: response loss leaves enqueue/execution uncertain, and a
post-compact hook can interrupt after the context-compaction item completed and replacement history
was already installed.

After app-server process restart or thread unload, compact-start rejects until the thread is
resumed. Lifecycle notifications are not replayed to a later subscriber. If the request connection
or subscription is lost after `{}` but before the exact terminal is observed, the old operation's
wire outcome is unknown; resume/history inspection may show durable state, but it does not recreate
the exact live lifecycle proof and therefore cannot justify a blind exactly-once retry.

## Interruption And Failure

Once `turn/started` exposes the compaction turn id, ordinary `turn/interrupt` can target it.
App-server validates its active-turn snapshot, submits untargeted core `Op::Interrupt`, and defers
the interrupt `{}` response until a core terminal event. For an admitted interrupt, the listener
orders the interrupt response before status clearing and before `turn/completed`. The resulting
terminal is normally `interrupted`, but a natural completion that wins the race can drain the
pending interrupt and yield `completed` or `failed` instead.

On interruption, `item/started` may have no matching `item/completed`. A post-compact hook can also
produce the inverse-looking case: the item completed after installation, then the hook stopped the
turn and the terminal is `interrupted`. Therefore neither item completion alone nor interrupted
terminal alone states whether replacement history was installed.

Forced interruption calls `JoinHandle::abort()` after the graceful timeout without awaiting the
join before emitting `TurnAborted`. Even though compaction has no intentional detached item
producer, this source does not establish a general no-later-item barrier for a send already racing
the forced-abort boundary. The exact interrupted terminal follows app-server's active-state clear;
that resolves to idle in the ordinary no-error interrupt path, but can remain `systemError` if a
prior final `Error` already set that fact. The interrupted terminal alone is therefore not an
unconditional idle proof. Consumers should also retain fail-closed handling for a genuinely late
item event.

For non-interruption compact errors, local and remote implementations emit a core `Error`; the
`CompactTask` deliberately converts errors other than `TurnAborted` into a normal task return.
Core therefore emits `TurnComplete`, while app-server uses the saved error to translate the terminal
as `failed`. Final failure normally leaves the compaction item started but uncompleted. Intermediate
retryable stream errors do not terminate the task and may precede eventual success.

## Beryl Integration Impact

The current rework tree already has the right low-level vocabulary but no live compaction request
path. `ManagedBackendSession::compact_thread` returns `ResponseFamilyUnavailable`, and the status
worker immediately reports that foreground response handling is unavailable. The incoming-response
grammar accepts the exact empty result, while provider observation ingress recognizes
`contextCompaction` and its id.

When the live path is restored, the reusable release-pinned correlation is:

- subscribe the exact worker connection by metadata resume before compact-start;
- treat `{}` only as accepted core enqueue;
- learn the generated turn id from `turn/started` and bind it to the `contextCompaction`
  `item/started` on the same thread and turn;
- require the same item id on `item/completed` for successful installation evidence;
- use the exact same-turn terminal for final outcome;
- accept a `completed` terminal as proof the idle boundary already occurred, handle an
  `interrupted` terminal using the preceding status/error evidence, and handle a `failed` terminal
  as non-running `systemError`, not idle.

# Sources

- Canonical repository: <https://github.com/openai/codex.git>. Requested ref
  `rust-v0.144.1`; peeled and inspected commit
  `44918ea10c0f99151c6710411b4322c2f5c96bea`; accessed 2026-07-31. A temporary shallow checkout was
  verified with `git rev-parse HEAD`, `git remote get-url origin`, and `git status --short`.
- Protocol and documented surface: [`ClientRequest` declaration and per-thread serialization
  scope](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/common.rs#L583-L588),
  [`ThreadCompactStartParams` and empty response](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/thread.rs#L937-L946),
  and the official [compaction workflow description](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/README.md#L674-L690).
- App-server request path: [`MessageProcessor` queued initialized-request dispatch and response
  send](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/message_processor.rs#L810-L844),
  [method dispatch](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/message_processor.rs#L1104-L1108),
  and [result response routing](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/message_processor.rs#L1424-L1434).
- App-server thread handler: [`load_thread`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L776-L790),
  [`submit_core_op`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L1071-L1080),
  and [`thread_compact_start_inner`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L1820-L1832).
- Request ownership after acknowledgement: [`RequestSerializationQueues`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_serialization.rs#L16-L161)
  shows the cross-connection per-thread key and that the next handler runs after the previous
  request future, not after its core task.
- Core id/enqueue and task creation: [`Codex::submit_with_trace` and
  `submit_with_id`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/mod.rs#L745-L791),
  [`new_submission_id`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/mod.rs#L904-L912),
  [`handlers::compact`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/handlers.rs#L456-L461),
  and [`Session::spawn_task`/`start_task`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/tasks/mod.rs#L314-L451).
- Core completion, replacement, and forced interruption: [`Session::on_task_finished`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/tasks/mod.rs#L563-L803)
  and [`handle_task_abort`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/tasks/mod.rs#L829-L909).
- Compact implementations: [`CompactTask`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/tasks/compact.rs#L16-L82),
  [local manual compaction](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/compact.rs#L122-L375),
  [remote compaction](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/compact_remote.rs#L74-L301),
  [remote-v2 compaction](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/compact_remote_v2.rs#L85-L322),
  and [token-budget compaction](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/compact_token_budget.rs#L20-L97).
- Core event identity and queueing: [`Session::send_event`, raw persistence/delivery, and item
  emitters](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/mod.rs#L1767-L2014)
  and [`ContextCompactionItem::new`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/protocol/src/items.rs#L408-L425).
- Subscription and listener ownership: [`ensure_conversation_listener`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_lifecycle.rs#L138-L186),
  [`ensure_listener_task_running` event loop](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_lifecycle.rs#L213-L345),
  [`thread/start` auto-attach](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L1272-L1294),
  and [`thread/resume` auto-attach](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L2837-L2858).
- Thread-scoped fanout: [`ThreadScopedOutgoingMessageSender`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/outgoing_message.rs#L108-L166)
  drops typed notifications when its subscriber snapshot is empty.
- App-server lifecycle translation: [`TurnStarted` and `TurnComplete`
  branches](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L152-L203),
  [`Error` and item branches](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L879-L995),
  [`TurnAborted` branch](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L1045-L1066),
  [terminal builders](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L1216-L1449),
  and [saved terminal error](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L1541-L1566).
- Status ordering and failed-state distinction: [`note_turn_started`, `note_turn_completed`,
  `note_turn_interrupted`, and `note_system_error`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/thread_status.rs#L145-L197),
  [publication](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/thread_status.rs#L223-L245),
  and [`loaded_thread_status`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/thread_status.rs#L428-L450).
- Interruption: [`turn_interrupt_inner`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/turn_processor.rs#L1348-L1418)
  supplies target validation, deferred response, and submission-failure cleanup.
- Focused tests: [app-server compact start, empty response, item identity, and invalid/unknown
  rejection](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/suite/v2/compaction.rs#L259-L393),
  [the test reader's nonmatching-message buffer](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/common/test_app_server.rs#L1537-L1665),
  and [core same-turn lifecycle-id assertion](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/tests/suite/compact.rs#L415-L456).
- Current Beryl local use sites inspected on 2026-07-31: `crates/beryl-backend/src/session.rs`
  (`ManagedBackendSession::compact_thread`, lines 851-859),
  `crates/beryl-backend/src/incoming_json/response.rs` (`ResponseFamily::ThreadCompactStart`),
  `crates/beryl-backend/src/incoming_json/provider/machine/response/success.rs` (empty-result
  machine, lines 126-128), `crates/beryl-backend/src/provider_observation.rs`
  (`ProviderItemKind::ContextCompaction`), and `crates/beryl-app/src/shell/status_operation.rs`
  (`run_context_compaction_worker`, lines 91-104).
- Investigation commands were bounded to the named source instance and local wrappers: shallow
  `git clone --depth 1 --branch rust-v0.144.1 --single-branch`, identity/status verification,
  focused `rg -n` symbol searches, and line-bounded `Get-Content` inspection.
