# Reason For Investigation

Phase 64 needs the exact Codex App Server 0.144.1 `turn/interrupt` admission,
rejection, dispatch, response, terminal-ordering, and late-item behavior. In
particular, Beryl must know whether a backend error can safely prove both that
interruption was not performed and that the requested turn remains live, and
whether an interrupt response or interrupted `turn/completed` is a no-later-item
barrier.

# Outcome

## Direct Answer

For an ordinary nonempty `turnId`, the handler uses app-server's translated
thread state as a precondition and then submits an untargeted core
`Op::Interrupt`.

- With a loaded thread and a matching active-turn snapshot, it records the
  request in a thread-scoped pending-interrupt queue, records the requested turn
  id as request context, and submits `Op::Interrupt`. Successful submission
  produces no immediate response. A later core `TurnAborted` or `TurnComplete`
  drains the pending queue and sends `{}`.
- With a loaded, stably terminal or otherwise non-running thread and no active
  snapshot, it rejects with invalid-request code `-32600`, no `data`, and
  `no active turn to interrupt`. The rejection precedes core submission.
- With an active snapshot whose id differs from the requested id, it rejects
  with code `-32600`, no `data`, and
  `expected active turn id <requested> but found <actual>`. The rejection
  precedes core submission.
- With a syntactically valid but absent or unloaded thread id, it rejects with
  code `-32600`, no `data`, and `thread not found: <id>`. `get_thread` consults
  only the live in-memory thread map; persisted cold state is not loaded.
  A syntactically invalid id instead receives code `-32600`, no `data`, and
  `invalid thread id: <parser error>`.

Two race qualifications prevent treating those bullets as an atomic
exact-target primitive.

- If there is no app-server active snapshot, the handler permits dispatch when
  core `AgentStatus` is `Running` and the requested id is not the remembered
  last terminal id. It does not otherwise prove that requested id in this
  fallback branch.
- After the snapshot check, core receives only `Op::Interrupt`; the requested
  turn id is not carried into core cancellation. The core handler aborts the
  current task, if any. The pending response queue contains request ids rather
  than target turn ids and is drained by any thread `TurnAborted` or
  `TurnComplete` without comparing the event turn id.

Consequently, pinned `turn/interrupt` has a useful front-end turn-id
precondition but does not implement an atomic guarantee that core cancels only
the supplied turn.

## Response And Error Shapes

This is the stable `turn/interrupt` client method backed by the protocol's v2
parameter and response types; it is not experimental. The request envelope is
`{"method":"turn/interrupt","id":...,"params":{"threadId":"...","turnId":"..."}}`,
where the id may be a string or integer and both params are strings. App-server
neither sends nor expects a `jsonrpc` member.

Success is the empty result `{}`; on the official executable the response
envelope is `{"id":...,"result":{}}` and omits `jsonrpc`.

For a normal nonempty id, the empty response is deferred until a terminal core
event. It can be paired with:

- `turn/completed` status `interrupted` after `TurnAborted`;
- status `completed` after a natural successful `TurnComplete`; or
- status `failed` after a `TurnComplete` for which app-server retained a turn
  error.

Thus `{}` proves neither interrupted lifecycle nor even that the terminal event
was caused by this request. It proves that the interrupt op was accepted into
the core submission channel and that a later terminal event drained the
thread-scoped pending request.

Handler errors use the wire envelope
`{"error":{"code":...,"message":"..."},"id":...}`. `data` is omitted. In
addition to the `-32600` precondition errors above, failure of the core
submission call removes the pending request and returns internal-error code
`-32603`, no `data`, and `failed to interrupt turn: <error>`. Ordinary typed
request decoding separately rejects malformed params before the handler runs,
but in this pinned release `deserialize_client_request` maps that Serde failure
to invalid-request code `-32600`, not invalid-params code `-32602`.

An empty `turnId` is a special startup-interrupt path. It bypasses all
active-turn checks, submits the same untargeted `Op::Interrupt`, and returns
`{}` immediately after successful submission because startup cancellation has
no turn event. This path is not an exact ordinary-turn operation.

The wire shapes are protocol facts at this commit, while the handler-path
meanings, deferred timing, rejection verdicts, and ordering conclusions in this
note are release-scoped to the pinned `0.144.1` source. Method-name or shape
support in another release does not carry those semantic proofs forward.

## Machine-Readable Rejection Evidence

There is no method-specific machine-readable error `data` at this release.
Human-readable text is the only distinction among malformed typed request,
absent thread, no active turn, and active-turn mismatch.

The correlated `-32600` code nevertheless has one narrow source-pinned meaning
for this method: typed-request decoding emits it before entering the handler,
and every `-32600` path in `turn_interrupt_inner` returns before
`submit_core_op(Op::Interrupt)`. It therefore proves that app-server did not
enqueue its core interrupt op. This is a remote no-core-submission fact, not
Beryl's local proof that no request byte crossed the transport.

The handler-produced correlated `-32603` has the same no-core-enqueue result by
a different path. `submit_with_id` returns an error only when
`tx_sub.send(sub).await` fails; `turn_interrupt_inner` then removes the pending
request and emits `failed to interrupt turn: <error>`. That request's
`Op::Interrupt` was not accepted by the submission loop and cannot have caused
an interrupt effect. This is a release-pinned, method-path conclusion, not a
generic meaning of JSON-RPC code `-32603`; it still says nothing about target
liveness.

Code `-32600` does not identify which precondition failed and cannot prove that
the requested target remains live. In fact, the declared cases include absent,
already terminal, no-active, and different-active-turn states. No inspected
error code or data payload supplies a closed "same target is still active and
safe to retry" verdict.

## Response, Terminal, And Item Ordering

For a nonempty request already admitted to the pending queue, both terminal
branches perform these awaited sends in order:

1. cancel pending per-turn server requests;
2. send all pending `turn/interrupt` responses in queue order;
3. update thread watch status; and
4. send the translated `turn/completed`.

On the same healthy subscribed connection, response and notification use the
same outbound router and per-connection FIFO writer. The matching success
response is therefore enqueued and written before the resulting
`turn/completed`. The focused integration test also waits for the interrupt
response and then the interrupted terminal notification, although its test
reader buffers nonmatching messages and is not itself a wire-order proof.

If the terminal event wins app-server's thread-state lock before the interrupt
request is admitted, the later request rejects rather than entering the pending
queue. That error response and the already-progressing terminal notification
have no source-defined relative order.

The listener reads one core event, tracks it, fully awaits its typed
translation, and only then reads the next core event. Target item events already
ahead of `TurnAborted` or `TurnComplete` in that core queue are therefore sent
before the interrupt response. A target item event received after the terminal
event is processed after `turn/completed`; the item branches contain no
terminal-state suppression and will still translate it.

Core gives a task 100 ms to finish gracefully, then calls
`JoinHandle::abort()` without awaiting the join before it emits `TurnAborted`.
The focused core tests exercise inert never-ending tasks, observe the synthetic
raw interrupted-turn marker before `TurnAborted`, and find no immediately
queued extra event. They do not exercise an in-flight item send or wait for a
delayed post-terminal event. The pinned source and tests therefore do not prove
a general no-later-item barrier after either the interrupt response or
interrupted `turn/completed`.

The safe release-scoped stream model is:

- item notifications processed before the core terminal event;
- interrupt success response;
- `turn/completed`; and
- any genuinely late item notification that core delivers afterward.

Unrelated notifications may interleave. Transport loss, unsubscribe, and
connection replacement remain outside this healthy-stream ordering.

## Beryl Impact

The Phase 64 requirement to keep interrupted forced-abort history incomplete
unless a no-later-item barrier is proven is supported by the source.

The current backend and app design text also names a closed machine-readable
exact-target rejection that may authorize safe reopen. That verdict is not
available from Codex App Server 0.144.1. A remote `-32600` or the pinned
handler-produced `-32603` may establish that no core interrupt op was submitted,
but safe reopen additionally needs separately ordered Beryl evidence that the
same exact target is still live. It cannot be derived from the code, absent
`data`, or diagnostic message.

Likewise, a design that requires app-server itself to atomically bind
cancellation and response to the supplied turn id is unimplementable against
this pinned release. Beryl can enforce exact authority before issuing the
request, avoid retries, and reconcile lifecycle from the separate exact turn
stream, but it cannot strengthen the upstream untargeted core op or
thread-scoped terminal response queue into an atomic backend guarantee.

The present Beryl source has only compatibility decoding for the empty
acknowledgement; production `ManagedBackendSession::interrupt_turn` remains
response-family-unavailable pending the stop implementation. No existing
production implementation must be preserved around the unavailable verdict.

Local routing inputs inspected were `doc/plan.md` Phase 64,
`doc/systems/backend-runtime/design.md` Exact Interruption Boundary,
`doc/systems/cas-live-syndic-transcript/design.md` Exact Stop Operations,
`crates/beryl-backend/doc/design.md`, the placeholder in
`crates/beryl-backend/src/session.rs`, and the interrupt compatibility decoder.
Relevant sibling memory consulted before upstream source was
`notification-ordering.md`, `initialize-config-model-responses.md`,
`jsonrpc-response-wire-order.md`, and `turn-steer-delivery-correlation.md` in
this commit scope.

# Sources

Canonical remote: `https://github.com/openai/codex.git`. Requested ref:
annotated tag `rust-v0.144.1`; the
[tag object](https://api.github.com/repos/openai/codex/git/tags/db75c19352d29ef29c17dbcf73a7244f1b1a8d10)
resolves to full commit `44918ea10c0f99151c6710411b4322c2f5c96bea`.
The pinned [workspace version](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/Cargo.toml#L132-L134)
is `0.144.1`. All upstream sources below were accessed 2026-07-30.

- [`TurnInterruptParams` and `TurnInterruptResponse`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/turn.rs#L203-L214)
  define the two string params and empty success result.
- [The pinned request/response example](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/README.md#L923-L932),
  [`ClientRequest` method binding](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/common.rs#L811-L821),
  and
  [JSON-RPC request/id declarations](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/rpc.rs#L1-L55)
  establish the method envelope, string-or-integer id, and omitted `jsonrpc`.
- [`turn_interrupt`, `load_thread`, and `turn_interrupt_inner`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/turn_processor.rs#L193-L200),
  [loaded-thread lookup](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/turn_processor.rs#L279-L292),
  and the
  [complete interrupt handler](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/turn_processor.rs#L1268-L1327)
  establish every precondition, exact diagnostic, pending-queue mutation,
  startup special case, submission, deferred response, and submission-error
  cleanup.
- [`deserialize_client_request`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/message_processor.rs#L91-L100)
  establishes that malformed typed request parameters are rejected with
  invalid-request code `-32600` before method dispatch.
- [`ThreadManager::get_thread`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/thread_manager.rs#L1127-L1139)
  proves that the lookup consults loaded threads rather than loading cold
  persisted state.
- [App-server error constructors and codes](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/error_code.rs#L2-L28),
  [JSON-RPC error fields](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/rpc.rs#L69-L91),
  and
  [outgoing response/error envelopes](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/outgoing_message.rs#L20-L44)
  establish the codes, omitted `data`, and success/error wire shapes.
- [Core submission](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/mod.rs#L744-L793),
  [untargeted interrupt dispatch](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/handlers.rs#L714-L729),
  and
  [`interrupt_task`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/mod.rs#L3960-L3967)
  prove that `Op::Interrupt` carries no turn id and aborts the current task, if
  any.
- [Core task abortion and terminal production](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/tasks/mod.rs#L492-L519)
  and the
  [100 ms forced-abort path](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/tasks/mod.rs#L829-L905)
  establish cancellation, the unawaited handle abort, interrupted marker, and
  `TurnAborted` emission.
- [Per-thread listener serialization](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_lifecycle.rs#L302-L342)
  and
  [thread state and terminal tracking](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/thread_state.rs#L77-L155)
  establish translated active/terminal state and one-event-at-a-time handling.
- [Terminal branches](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L182-L198),
  [item translation](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L939-L994),
  [aborted-turn branch](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L1045-L1061),
  [terminal notification construction](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L1224-L1245),
  and
  [completion, interruption, and pending responses](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L1409-L1511)
  establish response-before-terminal enqueue order, all terminal statuses, lack
  of response turn-id comparison, and lack of late-item suppression.
- [Response and targeted-notification enqueue paths](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/outgoing_message.rs#L503-L628),
  the
  [single outbound router](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/lib.rs#L870-L878),
  [per-connection queue routing](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/transport.rs#L140-L170),
  and the
  [WebSocket FIFO writer](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/transport/websocket.rs#L288-L328)
  establish healthy-connection write order.
- App-server integration tests cover
  [active-turn interruption](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/suite/v2/turn_interrupt.rs#L33-L139),
  [completed-turn rejection](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/suite/v2/turn_interrupt.rs#L142-L220),
  and
  [approval cancellation during interruption](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/suite/v2/turn_interrupt.rs#L223-L350).
  The
  [test client's buffered selector](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/common/test_app_server.rs#L1537-L1665)
  explains why those sequential waits do not independently prove wire order.
- Core tests for
  [forced and graceful abort](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/tests.rs#L9521-L9646)
  establish the interrupted marker and immediate common-path event sequence but
  do not exercise a late in-flight item producer.
