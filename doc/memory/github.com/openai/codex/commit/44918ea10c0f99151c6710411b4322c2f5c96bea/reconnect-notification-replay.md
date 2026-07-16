# Reason For Investigation

Phase 13 ordinary-turn capture needs to know whether Codex App Server 0.144.1 can
continue one authoritative live notification stream after a WebSocket interruption,
late subscription, or `thread/resume`. In particular, Beryl must not treat a later
`turn/completed` notification as proof of a complete captured turn if CAS can deliver
it without replaying earlier `turn/started`, `item/started`, delta, and
`item/completed` notifications.

# Outcome

## Conclusion

Codex App Server 0.144.1 has no turn/item notification replay protocol. A connection
receives only events for which its current connection id was in the thread's subscriber
set when the single app-server thread listener consumed that core event. Reconnect
creates a new connection id, and `thread/resume` adds that id for future events; neither
operation transfers the old subscription nor replays missed notifications.

Therefore a newly resumed, reconnected, or otherwise late subscriber can observe a
future `turn/completed` notification without having observed every earlier item
notification. It can also miss the terminal notification entirely when completion
occurs before the new subscription is installed. The terminal notification cannot
repair the gap because 0.144.1 deliberately sends it with an empty `turn.items` vector
and `itemsView = "notLoaded"`.

## Delivery Path

- Core owns one unbounded event receiver per live `Codex` session. App-server's one
  per-thread listener calls `conversation.next_event()`, updates its in-memory turn
  snapshot, reads the subscriber ids that exist for that event, and constructs a
  `ThreadScopedOutgoingMessageSender` containing that fixed id vector before translating
  the event.
- Thread-scoped notifications are sent only to that vector. There is no notification
  sequence cursor, last-seen token, acknowledgement, retained per-connection log, or
  resend operation.
- A healthy initialized subscriber with no notification opt-outs and a writable outbound
  queue receives the translated events in listener/channel order. A missing destination
  is dropped. A full WebSocket outbound queue disconnects the slow connection rather
  than retaining notifications for later delivery.
- `thread/start` and `thread/fork` synchronously auto-attach their requesting connection.
  Cold `thread/resume` does the same. App-server also best-effort attaches all connections
  already initialized when a new core thread is created. Merely initializing after an
  existing thread was loaded does not subscribe the late connection; there is no
  `thread/subscribe` request in this release.

## Scenario Results

- Healthy subscriber: receives future events selected while its connection id remains
  subscribed. Ordered WebSocket delivery means a terminal received on the same healthy
  stream follows earlier messages enqueued on that stream, but CAS provides no replay
  or independent delivery acknowledgement.
- Late subscriber: receives nothing for an already loaded thread until it calls
  `thread/resume`. Resume supplies an optional materialized state snapshot and installs
  the subscription for later events; it does not emit the missed lifecycle sequence.
- Same-process reconnect: the replacement WebSocket has a newly allocated connection id.
  Disconnect cleanup removes the old id from every thread. Events consumed between that
  cleanup and the later resume target neither connection; events already targeted at the
  disconnected id are dropped and are not retargeted.
- CAS process restart: connection ids, subscriber maps, listeners, active snapshots, and
  outbound queues are process memory and disappear. A new process can reconstruct a
  public thread snapshot from the rollout, but it emits no historical item or terminal
  notification sequence. When no live turn exists, read/resume projection changes a
  persisted stale `inProgress` turn to `interrupted`.
- Running-thread resume: the resume command is serialized on the existing listener. It
  combines persisted history with the listener's active-turn snapshot, installs the new
  connection id, sends the response, and then permits future event handling. The only
  explicit replay after this response is unresolved reverse server requests such as
  approvals or dynamic-tool calls, not notifications.

## Recovery Reads And Their Limit

- Stable `thread/read` with `includeTurns = true` rebuilds public `Turn` and `ThreadItem`
  snapshots from persisted rollout history. It is a state read, not an event-log read.
- Experimental `thread/turns/list` with `itemsView = "full"` also rebuilds the entire
  rollout on every request and, for a loaded running thread, merges the listener's current
  active-turn snapshot before pagination. `thread/resume.initialTurnsPage` uses the same
  page builder; ordinary resume includes reconstructed `thread.turns` unless
  `excludeTurns` is true.
- Experimental `thread/items/list` is registered, but the 0.144.1 local thread store does
  not implement `list_items`; the processor maps that unsupported result to JSON-RPC
  method-not-found with `thread/items/list is not supported yet`.
- These reads coalesce and materialize state. `ThreadHistoryBuilder` ignores streaming
  agent-message delta events, upserts changing operational items, and can synthesize
  public item ids. Reads cannot reproduce original delta boundaries, item lifecycle
  notifications, notification timing, or a gap-free source-event sequence. A final full
  item may recover visible text after persistence, but that is not notification replay.

## Local Beryl Impact And Phase 13 Recommendation

Beryl's current native-lineage resume sends `excludeTurns = true`, and its public
`thread/read` wrapper requests metadata only. It therefore receives no historical item
snapshot during ordinary projection resume. Its connection driver correctly retires the
whole live authority on stream failure, and ordinary capture closes the exact turn as
incomplete when its routed target closes.

Keep that fail-closed Phase 13 behavior. Do not reconnect or resume a replacement CAS
connection into the same live-capture target, and do not accept a later terminal
notification as proof that the prior item stream was captured. Any future repair design
would need a separately authorized snapshot-reconciliation protocol, exact durable
item/text comparisons, explicit handling of missing partial deltas, and a new source
boundary; it must not be described or tested as CAS notification replay.

The source is conclusive about absence of replay. The unresolved exactness risk is the
content and persistence frontier of a running-turn snapshot under every mid-delta and
terminal race. That frontier was not live-probed here and is unnecessary for the current
fail-closed recommendation; it must be probed before Beryl relies on read-based repair.

# Sources

## Exact Upstream Release

- OpenAI `codex`, canonical remote `https://github.com/openai/codex.git`, requested tag
  `rust-v0.144.1`, exact commit
  `44918ea10c0f99151c6710411b4322c2f5c96bea`, inspected 2026-07-16.
- The release README documents automatic subscription on `thread/start` and
  `thread/fork`, unsubscribe semantics, and turn streaming, but promises no replay:
  [`codex-rs/app-server/README.md#L140-L171`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/README.md#L140-L171)
  and
  [`README.md#L455-L467`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/README.md#L455-L467).
- OpenAI, [Codex App Server](https://developers.openai.com/codex/app-server), accessed
  2026-07-16. It describes item notifications as live source-of-truth events but has no
  reconnect cursor, subscription replay, missed-event recovery, or delivery guarantee.

## Connection, Subscription, And Dispatch Source

- Fresh connection ids and WebSocket close events:
  [`app-server-transport/src/transport/mod.rs#L194-L198`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/transport/mod.rs#L194-L198)
  and
  [`websocket.rs#L181-L228`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/transport/websocket.rs#L181-L228).
- In-memory bidirectional connection/thread indexes, subscribe, unsubscribe, and disconnect
  cleanup: `ThreadStateManager` in
  [`thread_state.rs#L252-L314`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/thread_state.rs#L252-L314),
  [`#L448-L520`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/thread_state.rs#L448-L520),
  and
  [`#L542-L576`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/thread_state.rs#L542-L576).
- Per-event subscriber snapshot and translation:
  `ensure_conversation_listener` and the listener loop in
  [`thread_lifecycle.rs#L138-L186`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_lifecycle.rs#L138-L186)
  and
  [`#L277-L342`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_lifecycle.rs#L277-L342).
- Running resume's serialized snapshot/subscription and the pending-request-only replay:
  [`thread_lifecycle.rs#L520-L699`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_lifecycle.rs#L520-L699)
  and
  [`outgoing_message.rs#L352-L371`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/outgoing_message.rs#L352-L371).
- Targeted notification fan-out and disconnect-on-full behavior:
  [`outgoing_message.rs#L589-L626`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/outgoing_message.rs#L589-L626)
  and
  [`transport.rs#L134-L169`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/transport.rs#L134-L169).
- Auto-attachment of initialized connections when a core thread is created, including the
  explicit no-resync-on-lag behavior:
  [`app-server/src/lib.rs#L1125-L1146`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/lib.rs#L1125-L1146).
- Item translation and empty terminal payload:
  [`bespoke_event_handling.rs#L939-L994`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L939-L994)
  and
  [`#L1224-L1245`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L1224-L1245).

## Recovery Source And Tests

- Stable read and experimental paging shapes:
  [`app-server-protocol/src/protocol/v2/thread.rs#L1272-L1365`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/thread.rs#L1272-L1365).
- `thread/read`, `thread/turns/list`, active-turn merge, and full/summary/not-loaded item
  views:
  [`thread_processor.rs#L2230-L2468`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L2230-L2468)
  and
  [`#L4018-L4095`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L4018-L4095).
- Unsupported local `thread/items/list` path:
  [`thread_processor.rs#L2471-L2530`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L2471-L2530),
  [`thread-store/src/store.rs#L105-L121`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/thread-store/src/store.rs#L105-L121),
  and the non-overriding local implementation in
  [`local/mod.rs#L240-L322`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/thread-store/src/local/mod.rs#L240-L322).
- Lossy materialization reducer and synthetic ids:
  [`thread_history.rs#L81-L90`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/thread_history.rs#L81-L90),
  [`#L316-L384`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/thread_history.rs#L316-L384),
  and
  [`#L1452-L1455`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/thread_history.rs#L1452-L1455).
- Upstream behavioral tests inspected:
  `thread_resume_keeps_in_flight_turn_streaming` in
  [`thread_resume.rs#L2774-L2874`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/suite/v2/thread_resume.rs#L2774-L2874),
  stale-turn read/resume projection in
  [`#L2527-L2645`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/suite/v2/thread_resume.rs#L2527-L2645),
  and WebSocket disconnect retention in
  [`connection_handling_websocket.rs#L449-L478`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/suite/v2/connection_handling_websocket.rs#L449-L478).

## Local Beryl Use Sites

- Metadata-only resume and read:
  `ManagedBackendSession::resume_thread` at
  `crates/beryl-backend/src/session.rs:649` and `ThreadResumeParams::new` with
  `exclude_turns: true` at `crates/beryl-backend/src/thread_lineage.rs:309`;
  `read_thread_metadata_response` at `crates/beryl-backend/src/session.rs:672`.
- Sole connection stream reader and fail-closed retirement:
  `run_driver` at `crates/beryl-app/src/cas_projection/connection/driver.rs:349`, especially
  the poll failure retirement at line 369.
- Ordinary capture consumes item lifecycle events separately, reconciles only the received
  terminal snapshot, and closes incomplete on target loss:
  `LiveCapture::handle_event` at
  `crates/beryl-app/src/cas_projection/ordinary/capture.rs:121` and `run_capture` at
  `crates/beryl-app/src/cas_projection/ordinary/execute/capture_loop.rs:227`.
- Controlling local authority consulted:
  `doc/systems/cas-live-syndic-transcript/design.md` under `Live Event Capture` and
  `Recovery And Idempotency`, `doc/plan.md` Phase 13, and
  `doc/failures/cas-shared-session-mutex-poller.md`.

## Reproduction Commands

```text
git rev-parse HEAD
git describe --tags --exact-match HEAD
git remote get-url origin
rg -n "ThreadUnsubscribe|thread/unsubscribe|ConnectionId|subscribed_connection_ids" codex-rs/app-server codex-rs/app-server-protocol codex-rs/app-server-transport
rg -n "thread_read|thread_turns_list|thread_items_list|replay_requests_to_connection_for_thread" codex-rs/app-server codex-rs/thread-store
rg -n "exclude_turns|poll_turn_stream_envelope|TurnCompleted" crates/beryl-backend crates/beryl-app
```
