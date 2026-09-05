# Goals

Expose compact, exact conversation status and selected-thread controls without mutating backend history, guessing unavailable backend state, or hiding disabled reasons.

## Non-goals

- Estimating context space from transcript text, model names, local tokenization, or accumulated spend.
- Applying model or reasoning changes before the next real user turn by starting synthetic backend turns.
- Terminating guessed OS processes for hard stop.

# Decisions

The normative GUI composition for this feature is [Status Line GUI](gui.md).

## Status Line Layout

- The status line strip is fixed to the bottom edge of the main window between the user input panel and the OS window edge.
- It uses the same edge-to-edge separator treatment as the main toolbar.
- It contains three left-to-right segments: `M/R` model/reasoning, Context, and Turn.
- The outer `M/R` and Turn segments are content-sized. Context is the flexible segment and takes the remaining status-line width.
- Turn accommodates its longest state label, `compacting`.
- The strip is UI chrome and is not part of the backend conversation transcript.

## Model And Reasoning Cell

- The model/reasoning cell displays the selected thread's active or pending model and reasoning effort.
- When the workspace is on a pending new-thread draft, it displays the draft's explicit first-turn selection when present; otherwise it displays the current effective backend defaults for the draft execution root.
- Missing values render `Unknown` or unavailable. Beryl must not infer effective reasoning from model-list menu defaults.
- Backend-derived values already known may remain visible when a runtime target is backend-unavailable; otherwise the cell renders unavailable/unknown without launching or probing a backend.
- The cell opens a model/reasoning popup only when an idle backend thread is selected or the workspace is on a pending new-thread draft.
- With an active selected-thread turn or backend-unavailable selected runtime target, the cell is non-clickable.
- The popup lists backend-supported models and restricts reasoning choices to the selected model's supported efforts.
- Choosing a model or reasoning effort updates selected-thread pending turn defaults or pending-new-thread first-turn defaults only. It does not mutate global Codex configuration, other workspaces, or other threads.
- Existing-thread selections are carried on the next submitted user turn for that thread. The backend-owned thread default is then the source for later status presentation.
- Pending-new-thread selections are carried on the first submitted user turn. A draft without explicit selection follows current effective backend defaults until submission or explicit user choice.

## Context And Rate-Limit Cell

- The Context space percentage readout displays a percentage only when the selected thread has exact token usage with a positive model context window.
- The percentage is computed from exact selected-thread token usage as `((modelContextWindow - last.inputTokens) / modelContextWindow) * 100`, clamped to `0..100`.
- Exact token usage may come from selected-thread `thread/tokenUsage/updated` notifications, in-memory same-thread cache populated by those notifications, durable GUI-held last-known snapshots originally populated by notifications, or read-only app-server status metadata for the same thread.
- If no exact same-thread usage is known, the model context window is missing, or the selected thread changes to one without known usage, only the Context space percentage readout displays `Unknown`. This state is independent of the token-counter readouts.
- Switching threads must not submit input, start backend turns, or mutate backend conversation history to fill this cell.
- When exact account rate-limit status is available, the same cell appends the active-model short-window and weekly remaining percentages independently.
- Rate-limit bucket identity such as `limitId` and `limitName` is preserved. Beryl selects the bucket matching the active model and avoids merging unrelated model-specific buckets.
- Rate-limit segments are omitted independently when the exact window or active-model bucket is unavailable.
- After the weekly rate-limit readout when present, Context appends the selected user-visible thread's accounting tree in this exact shape: `I/IC/O: main: <uncached>/<cached>/<output> sub: <uncached>/<cached>/<output>`.
- `main` is cumulative usage for the selected user-visible root thread itself. `sub` is the backend-accounted component sum of all unique transitive delegated descendants attached to that root. The counters never represent global or account-wide usage, and Beryl does not derive `sub` from Activity rows, thread inventory, or locally summed per-thread snapshots.
- For each group, uncached input is `max(cumulative input count - cumulative cached-input count, 0)`, cached input is the cumulative cached-input count, and output is the cumulative output count.
- Beryl displays no counter value below zero; any negative count is displayed as `0`.
- Counter formatting uses base-1000 thresholds. Values below `1000` display as integers. At or above a threshold, Beryl uses the largest applicable supported scale (`k`, `M`, then `B`) and rounds the scaled value to the nearest tenth, with halfway values rounding upward. When rounding reaches `1000` and a larger suffix exists, Beryl promotes the value to that suffix; `B` is the highest suffix. Beryl removes an unnecessary trailing `.0`.
- A complete schema-version-1 usage-tree snapshot supplies both groups. A confirmed complete tree with no descendant usage displays `sub: 0/0/0`.
- When no complete tree snapshot is available, `main` may use independently exact selected-root cumulative usage from the legacy token-usage projection while `sub` displays `—/—/—`. A `legacyPartial` tree snapshot is not presented as complete. If neither a complete tree nor independently exact legacy root usage is available, both groups display `—/—/—`.
- Beryl retains only one usage-tree snapshot, for the exact currently selected root thread. Selecting another root or a pending new-thread draft clears the retained tree before any replacement read or update is accepted. Activation reads through for the selected root; a late result is accepted only when its root and newer durable revision still match the current selection. A legacy or unavailable read uses only independently exact selected-root usage when known and otherwise presents the documented unavailable counters. A pending new-thread draft does not inherit counters from another thread.
- Context space continues to use only the selected root's current-context fields from root `self` usage. Descendant totals and tree totals do not affect the context-space percentage.
- Activating the context cell opens the context operations popup only when a backend conversation thread is selected, idle, and backend-available.
- With no selected thread, an active selected-thread turn, or a backend-unavailable selected runtime target, the cell is non-clickable.
- The context operations popup initially contains `Compact`, which starts backend context compaction for the selected thread. Request acceptance is not compaction completion.
- The app-wide compaction warning preference controls when Beryl reports `Compaction is taking longer than expected` after the backend accepts compaction. The default is 180 seconds. The persisted `context_compaction_timeout_seconds` key and the settings-tool `contextCompactionTimeoutSeconds` field retain their numeric values but represent this warning threshold. The setting does not change bounded JSON-RPC request timeouts.
- Crossing the warning threshold is nonterminal: Beryl continues observing compaction, keeps accepted input queued in order, and preserves exact-target stop controls. It must not declare failure, force the thread idle, release automatic continuation, or discard input solely because time elapsed.

## Compaction Observation And Recovery

- Beryl owns one compaction observation operation per active target. Its identity includes workspace, execution target, backend session, thread, and a local operation generation; once known, it also includes the exact backend compaction turn id. Late results from another identity or generation cannot complete the current operation.
- The worker subscribes on its own client before requesting compaction. It creates one fresh operation UUID and submits `thread/compact/start` once with that identity and the advertised CAS observation-session identity; loss of acknowledgement or completion evidence never authorizes an automatic repeat of that mutation.
- Compaction activity and target-thread idle are observations, not sufficient proof of successful compaction. Beryl completes successfully only after correlated successful terminal compaction evidence and confirmation that the target is idle. Failed or interrupted terminal outcomes remain distinct from success; retryable backend errors remain nonterminal.
- Before the warning threshold, the worker consumes lifecycle events normally. At the threshold, and at most once every 30 seconds afterward while completion remains uncertain, it makes bounded read-only receipt/status requests through the backend boundary. These requests use the ordinary request timeout, run off the GUI thread, and do not overlap for an operation.
- Successful start acknowledgement anchors the warning timer. If acknowledgement is lost or malformed after possible dispatch, the dispatch time anchors reconciliation scheduling and the surface reports unconfirmed acceptance; elapsed time cannot itself establish that the backend accepted the request.
- Correlated terminal stream evidence may trigger one metadata-only idle check before the warning threshold, because CAS can emit idle before its terminal notification. This check cannot repeat before the warning schedule begins and cannot reuse an earlier idle snapshot. Once the scheduled reconciliation interval applies, terminal-triggered checks share its request budget rather than creating another polling loop.
- Reconciliation reads the exact CAS compaction receipt and metadata-only target-thread status. A receipt keyed by the original operation UUID can recover the compaction turn id even when acknowledgement and all client lifecycle events were missed. An idle snapshot, a different operation, reconstructed history status, or a missing receipt cannot establish success.
- Each reconciliation attempt makes at most one receipt read and one metadata-only thread read. After a successful terminal receipt, the idle confirmation must come from a subsequent status read or later correlated stream observation. The worker retains only matched operation evidence and current status; it does not load or scan transcript history.
- A quiet stream or a failed status read keeps the outcome unconfirmed. Transport loss may reconnect and resubscribe for observation using the same target and generation, at most once per reconciliation interval. It must not resubmit compaction. Backend unavailability is shown explicitly and gates conflicting submissions rather than pretending the thread is idle.
- After confirmed success and idle, Beryl releases queued input exactly once through the ordinary next-turn path. Confirmed failure uses normal visible delivery-failure handling; confirmed interruption preserves accepted input for the next eligible turn and suppresses automatic lifecycle continuation. Unconfirmed completion preserves queued input without marking it delivered or failed.
- After interruption, the next explicit composer submission joins the preserved human input in accepted order and requests a fresh idle check before release. If the thread is still active or unavailable, the input remains held with localized feedback; ordinary terminal or idle notifications cannot resume it automatically. Generated lifecycle continuation is removed from the held queue.
- Stop continues to target the exact backend turn when known. Before that identity is known, the existing unavailable-stop behavior applies. Worker cancellation during workspace teardown or application shutdown releases its observation client and scheduled work without claiming backend completion; existing managed-backend shutdown policy still owns process cleanup.
- The Turn cell remains `compacting` during a long running operation. The warning uses existing surface-notice presentation and does not play a completion sound. Diagnostic evidence follows the diagnostics feature contract.

## CAS Compaction Receipt Contract

- The Beryl-maintained CAS fork exposes process-local compaction observation version 1. Initialization advertises `compactionObservationVersion` and `compactionObservationSessionId` together; the session id is a fresh UUID for each CAS process. Receipt-aware compaction requires this version and exact session identity. Missing support disables compaction with an explicit compatibility reason rather than falling back to uncorrelated observation.
- `thread/compact/start` accepts optional `operationId` and `observationSessionId`; supplying either requires both. The operation id is a fresh client UUID. An observed-start response identifies the exact thread, operation, observation session, and server-generated compaction turn. Legacy callers that omit both fields retain ordinary compact-start behavior; Beryl uses observed starts exclusively.
- `thread/compact/read` accepts the exact thread, operation, observation session, and optional expected turn id. It returns those identities, the known turn id when available, and one of `unknown`, `accepted`, `running`, `completed`, `failed`, or `interrupted`. Supplied identities that contradict a retained receipt are rejected; session change is an explicit unknown runtime-change result. A missing receipt never proves that the mutation was rejected or completed.
- Receipt state comes from raw Core lifecycle events before client notification delivery, independently of client subscriptions. Registration binds the operation to its exact server submission id, thread, and listener generation before the submission can execute. The server ensures the thread listener is running before reservation and enqueue. It enqueues once, and acceptance bookkeeping must not overwrite already observed running or terminal evidence.
- Core exposes a narrow prepared-compaction submission handle: it allocates the normal server submission id without enqueueing, exposes that identity for receipt registration, and is consumed to enqueue `Op::Compact` once through the existing submission path. It does not expose arbitrary caller-selected turn ids or change persisted history. Receipt locks are not held across channel submission.
- `accepted` proves successful enqueue; `running` requires the exact `TurnStarted`. `completed` requires both an exact completed `ContextCompaction` item and an exact error-free `TurnComplete`. A terminal completion carrying an error is `failed`; an exact `TurnAborted` is `interrupted`, including replacement or hook abortion. Retryable error notifications alone do not terminate the receipt. A bare error-free terminal without the completed compaction item remains `unknown` with incomplete-terminal-evidence provenance.
- Raw event updates must match thread, server turn identity, any supplied payload turn identity, and the listener generation. Late or unrelated events cannot update another receipt. Client disconnect preserves receipts. Listener replacement, clear, or failure invalidates affected unfinished receipts with observation-gap provenance; it cannot create terminal proof. Already retained terminal proof survives listener cleanup within the same CAS observation session.
- The registry retains at most 128 receipts globally. Thread, operation, observation-session, and server turn identities are validated UUIDs. Failed receipts retain at most 4 KiB of UTF-8 error text and a bounded classification, with explicit truncation; they do not retain arbitrary Core events or opaque error payloads. Interrupted and unknown reasons use bounded enums.
- Reserved, accepted, and running receipts are never evicted to admit another operation. Admission reclaims the oldest terminal or invalidated receipt when needed and rejects before mutation if all slots are live. A retained duplicate operation UUID is rejected without submitting again, even on a different thread. Eviction ends observational retention, not the client's prohibition on resubmission.
- Definite enqueue failure or cancellation before enqueue removes only the exact reservation. Cancellation after possible enqueue preserves uncertain evidence and must not authorize a repeat mutation. Terminal incomplete evidence and observation-gap records are reclaimable. Process restart changes the observation-session id and clears the registry; Beryl cannot carry terminal proof across that identity boundary.
- Receipt reads are bounded and read-only: they do not resume a thread, install a listener, compact, load history, or otherwise alter backend conversation state. Unknown reasons distinguish absent/expired receipt, submission pending without enqueue proof, runtime change, observation gap, and incomplete terminal evidence. A completed receipt does not assert that the thread is currently idle.

## Last-Turn State And Stop Controls

- Last-turn state displays `compacting` while selected-thread context compaction is active, `working` while a parent turn is active, `ok` after the latest completed turn, `error` after the latest failed or interrupted turn, and `Unknown` before any turn state is known.
- Interrupted turns without actual error payload update status but do not automatically enqueue turn-error notices.
- Activating the cell opens the turn operations popup only when the selected runtime target is backend-available and Beryl knows an interruptible backend turn id for the selected ordinary active turn or selected-thread compaction operation.
- Otherwise the cell is non-clickable.
- `Soft stop` requests backend interruption for the exact selected-thread active turn or compaction operation, then closes or reports request failure through popup feedback.
- Request acceptance is not terminal turn completion. Visible state converges from backend stream events, explicit termination responses, transport failure, or backend process exit.
- `Hard stop` first performs the same selected-operation interruption as soft stop, then best-effort terminates known running execution associated with that selected turn through exact backend-exposed handles.
- Hard stop may interrupt known active subagent turns, terminate process-backed command execution handles, and request thread-scoped background-terminal cleanup when those targets are known and supported.
- Hard stop never terminates by guessed OS pid, process name, working directory, or local process tree.
- If a running tool or subagent cannot be mapped to an exact backend termination handle, Beryl leaves it untouched and reports the unsupported target when relevant.
- Hard stop is a held action. Holding the row for three seconds triggers it once; releasing early, leaving the row, closing the popup, focus loss, or selected active-turn target change cancels.
- Keyboard activation must provide the same held affordance for the focused row.
- While in flight, stop rows suppress duplicate submissions until the request finishes or fails.
- Partial hard-stop failures and unsupported targets are surfaced through status-operation feedback.
- User input fragments queued before or during stop remain visible and ordered. If they cannot be delivered to the interrupted turn, they remain queued for the next eligible turn.

# Engineering Rigor

Profile: `personal-utility/v1`

Modifiers: none
