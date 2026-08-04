# Reason For Investigation

Phase 64 needs the exact Codex App Server 0.144.1 hard-stop-related
methods, handle identities, response timing, connection scope, and target
scope. In particular, Beryl must know whether a turn-owned command process can
be terminated with `command/exec/terminate`, whether thread cleanup is an
exact-operation primitive, and whether this release exposes a client-callable
exact child or subagent interruption method. The follow-up also asks whether a
turn-owned numeric process id is lifetime-unique or can be reused before a
frozen hard-stop request reaches core.

# Outcome

## Public Method And Handle Boundaries

- `command/exec/terminate` takes `{processId: string}` and returns `{}`. It
  reaches only a standalone process originally created by `command/exec` with
  that caller-supplied id. The manager key is
  `(originating connectionId, client processId)`, so the same originating
  connection is required. An omitted `command/exec.processId` creates an
  internal id that no follow-up request can address.
- `command/exec/terminate` does **not** reach a `processId` published on a
  turn's `CommandExecution` item. Standalone command sessions live in
  app-server's `CommandExecManager`; turn-owned commands live in the loaded
  thread's separate core `UnifiedExecProcessManager`.
- The individual public method for one turn-owned background process is experimental
  `thread/backgroundTerminals/terminate` with
  `{threadId: string, processId: string}`. Its result is
  `{terminated: bool}`. App-server parses `processId` as a decimal `i32` and
  looks it up only in that loaded thread's unified-exec manager. The id is
  exposed both on `CommandExecution.processId` and by
  `thread/backgroundTerminals/list`, whose entries also carry `itemId`.
- Experimental `thread/backgroundTerminals/clean` takes only
  `{threadId: string}` and returns `{}`. It is a coarse thread-session
  operation: core drains every entry then present in that thread's unified-exec
  process store. It has no turn id, item id, process snapshot, or per-process
  outcome and does not restrict cleanup to the selected turn.
- The three background-terminal methods require
  `initialize.capabilities.experimentalApi = true`. They can be issued through
  the same foreground connection that owns the selected turn, but upstream
  keys them by loaded `threadId`, not by originating connection. A different
  initialized connection to the same server can address the same loaded thread
  if it knows the id.

## Process Id Lifetime And ABA

Turn-owned unified-exec process ids are not lifetime-unique, even within one
loaded thread generation.

- Production allocation draws a random `i32` from the finite half-open range
  `1_000..100_000`. It rejects only numbers currently present in
  `reserved_process_ids`; there is no monotonic counter, generation component,
  tombstone, or never-reuse set.
- `ProcessStore::remove` deletes the number from both the process map and
  `reserved_process_ids`. Normal completion/error paths, state refresh,
  individual termination, pruning, and whole-thread cleanup can remove entries.
  The next allocation may therefore choose the same number immediately. No
  wraparound, manager reconstruction, or loaded-generation change is required.
- The test-only deterministic allocator is not safer as a lifetime identity:
  it chooses one above the current maximum reserved id, or `1000` when the
  current set is empty. It can likewise reuse an earlier number after removal.
- `thread/backgroundTerminals/list` derives `itemId` from the entry's core
  `call_id`, but `thread/backgroundTerminals/terminate` accepts only
  `threadId` and `processId`. Its lookup compares only the numeric id, and its
  response echoes only `terminated`; neither request nor response carries the
  frozen item id or another process-instance nonce.

Consequently, a frozen `(loaded generation, threadId, processId, itemId)`
cannot be atomically checked by this method. If the original entry exits or is
removed and a later command receives the same numeric id before handler
lookup, termination can hit the later process and still return
`{terminated:true}`. A preceding `list` comparison cannot close that
time-of-check/time-of-use race because terminate does not accept `itemId`.

## Response And Error Timing

- `command/exec` spawns its process runner and leaves the original response
  pending, so the originating connection remains able to issue
  `command/exec/terminate`. The terminate result is produced after the runner
  receives the control message and calls `request_terminate()`, not after
  process exit. The local kill error is ignored. The original `command/exec`
  response arrives later after exit and bounded output draining; source does
  not establish a cross-task wire-order barrier between the two responses.
- A missing or wrong-connection standalone id returns invalid-request
  `-32600` (`no active command/exec for process id ...`); a race with runner
  exit can instead return `command/exec ... is no longer running`. Follow-up
  termination is also rejected with `-32600` for the Windows restricted-token
  execution path.
- `thread/backgroundTerminals/terminate` calls the loaded thread manager
  directly and then returns its boolean. A missing process or a reported
  termination failure becomes `{terminated:false}`, not a JSON-RPC error.
  `{terminated:true}` proves the internal termination path accepted the target,
  but the local process path requests kill and marks internal exit without
  awaiting an operating-system exit observation.
- `thread/backgroundTerminals/clean` returns after `Op::CleanBackgroundTerminals`
  is accepted by the core submission channel. Core processes that op
  independently, drains the whole process store, and invokes non-confirming
  termination for each entry. There is no cleanup-completed notification or
  per-process response.
- Invalid or unloaded thread ids reject before the thread operation with
  `-32600`. A clean-op channel submission failure returns `-32603`.
  Experimental methods without the negotiated capability reject with
  `-32600` and `<method> requires experimentalApi capability`.

## Clean Submission Ordering

**Yes, within the same loaded core session:** a later `turn/start` or other
core operation that Beryl submits only after receiving the clean response
cannot be dequeued ahead of `Op::CleanBackgroundTerminals`.

- Each loaded `Codex` has one bounded `async_channel` submission queue.
  `submit_with_id` awaits `Sender::send`; pinned `async-channel` 2.5.0 makes
  that future ready successfully only after `try_send` has successfully pushed
  the value into its `ConcurrentQueue`.
- The clean request handler awaits `submit_core_op` before returning its empty
  response, and app-server sends that response only after the handler returns.
  The response is therefore sent after successful channel insertion. It may be
  sent before or after core dequeues or finishes the clean operation.
- The queue has FIFO behavior, and exactly one session submission loop receives
  from it. That loop awaits `clean_background_terminals`, including
  `close_unified_exec_processes`, before it performs its next `recv`.
  Consequently, a same-session core submission causally started after the
  response is both inserted after clean and dispatched only after clean's
  handler has finished.

This guarantee is narrower than cleanup completion acknowledgement. Operations
already queued before clean can delay it, and cloned concurrent senders can
place operations before or between clean and Beryl's later submission. They
cannot make Beryl's causally later submission overtake the completed clean
insertion. Active turn tasks and auxiliary tasks run outside the submission
loop and can still race process-store mutation, and a reloaded or replacement
thread session has a different channel. Completion of the clean handler also
still does not prove operating-system process exit.

## Child And Subagent Negative Finding

There is no client-request method named `interrupt_agent`, `close_agent`, or
another child-specific interrupt in the pinned app-server protocol. The public
`turn/interrupt` method can name a loaded child thread and turn, but the
separate source-pinned investigation shows that its core operation is
untargeted and has no atomic turn fence.

Core does contain a model-facing `interrupt_agent` tool. It accepts
`{target: string}`, where the target is an agent thread id or canonical task
name, captures the previous agent status, and calls
`AgentControl::interrupt_agent(ThreadId)`. That function merely submits
untargeted `Op::Interrupt` to the child thread. The tool result
`{previous_status}` follows submission, not child terminality; it carries no
turn id, and not-found or internally-dead targets are treated as successful
tool handling. This tool is not an app-server client request and cannot be
invoked deterministically through Beryl's foreground JSON-RPC connection.
Legacy model-facing `send_input` with `interrupt: true` uses the same path.
Model-facing `close_agent` is likewise not a client request and performs
subtree shutdown rather than exact current-turn interruption.

## Beryl Impact

- A turn `CommandExecution.processId` must not be routed to
  `command/exec/terminate`; that mapping crosses two unrelated process
  namespaces and cannot work. `thread/backgroundTerminals/terminate` is the
  relevant individual-process family, and its response decoder must preserve
  the `terminated` boolean, but the method is not an ABA-safe exact handle.
  Loaded generation, thread id, process id, and locally retained item id are
  insufficient unless Beryl owns an independent fence that prevents removal
  and reallocation through the backend lookup cut.
- `thread/backgroundTerminals/clean` may be represented only as an explicitly
  coarse thread-scoped cleanup target. Its accepted `{}` response is not
  completion evidence by itself, but it is an enqueue-order fence for later
  operations Beryl submits through the same loaded core session. Such a later
  core operation cannot overtake cleanup dispatch. The execution-time
  all-process scope is not a frozen set of handles associated solely with one
  selected turn, and off-queue task activity can still race the process store.
- The existing conclusion that pinned CAS 0.144.1 has no eligible exact child
  or subagent hard-stop primitive remains correct. The internal tool's exact
  agent-thread lookup does not add a client-callable turn target, terminal
  acknowledgement, or successor fence.
- Probing `command/exec/terminate` proves only support for its standalone,
  originating-connection namespace. It cannot prove that a provider
  `CommandExecution.processId` is addressable. Turn-process capability probing
  must probe the experimental background-terminal terminate family instead,
  but method support alone cannot make its reusable numeric id an exact
  process-instance authority.

# Sources

Canonical remote: `https://github.com/openai/codex.git`. Requested ref:
annotated tag `rust-v0.144.1`; the
[tag object](https://api.github.com/repos/openai/codex/git/tags/db75c19352d29ef29c17dbcf73a7244f1b1a8d10)
resolves to full commit `44918ea10c0f99151c6710411b4322c2f5c96bea`.
The pinned
[workspace version](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/Cargo.toml#L132-L134)
is `0.144.1`. Sources were accessed 2026-07-30.

- [Client method declarations](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/common.rs#L598-L615)
  establish all three experimental background-terminal methods; the
  [standalone command declarations](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/common.rs#L1056-L1074)
  establish `command/exec/terminate` and its process-id serialization.
- [Standalone command protocol types](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs#L30-L40)
  and
  [terminate types](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/command_exec.rs#L151-L165)
  define the caller-supplied, connection-scoped handle and empty response.
- [`CommandExecManager`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/command_exec.rs#L47-L79),
  [session creation](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/command_exec.rs#L142-L309),
  [control dispatch](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/command_exec.rs#L346-L443),
  and the
  [runner](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/command_exec.rs#L446-L557)
  establish connection identity and acknowledgement-before-exit timing.
- The
  [cross-connection integration test](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/suite/v2/command_exec.rs#L1045-L1122)
  proves that the same string on another connection cannot terminate the
  process and that disconnect terminates the originating connection's process.
- [`CommandExecution.processId`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L269-L279)
  and the
  [background-terminal protocol types](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/thread.rs#L979-L1040)
  define the distinct thread-owned identities and response shapes.
- [App-server handlers](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L1834-L1902)
  establish loaded-thread lookup, clean-op submission, listing, numeric parsing,
  and direct individual termination.
- The
  [production and test allocation policy](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/unified_exec/process_manager.rs#L86-L102)
  and
  [allocator and release path](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/unified_exec/process_manager.rs#L370-L406)
  establish finite random production ids and reuse after unreservation.
  [`ProcessStore::remove`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/unified_exec/mod.rs#L121-L131)
  removes both the entry and reservation; the
  [exit-state refresh](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/unified_exec/process_manager.rs#L791-L815)
  uses that removal after observed exit.
- [Core cleanup and individual termination](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/unified_exec/process_manager.rs#L1374-L1443)
  establish all-process draining, `itemId` projection, process-id-only lookup,
  and boolean outcomes. The
  [process handle implementation](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/unified_exec/process.rs#L199-L233)
  establishes request-style local termination rather than observed exit.
- [Session channel construction and submission](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/mod.rs#L537-L538)
  together with
  [`submit_with_id`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/mod.rs#L783-L794)
  establish the bounded channel and awaited send. The
  [single submission loop and clean branch](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/handlers.rs#L714-L739)
  establish sequential dequeue and awaited cleanup; the
  [turn task spawn](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/handlers.rs#L1248-L1318)
  identifies the off-loop task concurrency boundary.
- The pinned
  [lockfile entries](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/Cargo.lock#L567-L576)
  select `async-channel` 2.5.0 and `concurrent-queue` 2.5.0. In that release,
  [`Send` succeeds only after queue insertion](https://github.com/smol-rs/async-channel/blob/v2.5.0/src/lib.rs#L1164-L1189),
  while the
  [underlying queue's ordered push/pop example](https://github.com/smol-rs/concurrent-queue/blob/v2.5.0/src/lib.rs#L91-L107)
  and
  [`async-channel` push/pop paths](https://github.com/smol-rs/async-channel/blob/v2.5.0/src/lib.rs#L217-L230)
  establish the FIFO behavior used by the causal ordering conclusion.
- The
  [internal `interrupt_agent` handler](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/tools/handlers/multi_agents_v2/interrupt_agent.rs#L26-L87),
  its
  [tool specification](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/tools/handlers/multi_agents_spec.rs#L307-L324),
  and
  [`AgentControl::interrupt_agent`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/agent/control.rs#L247-L256)
  establish the model-only, agent-thread-targeted but turn-untargeted path.
- Read-only investigation searched the full pinned commit archive for
  `command/exec/terminate`, `thread/backgroundTerminals`,
  `interrupt_agent`, and `close_agent`, and inspected local Phase 64 routing
  sites. Relevant sibling memory consulted was
  `turn-interrupt-semantics.md` in this commit scope.
