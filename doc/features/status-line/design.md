# Goals

Expose compact, exact conversation status and selected-thread controls without mutating backend history, guessing unavailable backend state, or hiding disabled reasons.

## Non-goals

- Estimating context space from transcript text, model names, local tokenization, or accumulated spend.
- Applying model or reasoning changes before the next real user turn by starting synthetic backend turns.
- Terminating guessed OS processes for hard stop.

# Decisions

## Status Line Layout

- The status line strip is fixed to the bottom edge of the main window between the user input panel and the OS window edge.
- It uses the same edge-to-edge separator treatment as the main toolbar.
- It contains three left-to-right cells: model/reasoning, context space left, and last-turn state.
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

- Context space displays a percentage only when the selected thread has exact token usage with a positive model context window.
- The percentage is computed from exact selected-thread token usage as `((modelContextWindow - last.inputTokens) / modelContextWindow) * 100`, clamped to `0..100`.
- Exact token usage may come from selected-thread `thread/tokenUsage/updated` notifications, in-memory same-thread cache populated by those notifications, durable GUI-held last-known snapshots originally populated by notifications, or read-only app-server status metadata for the same thread.
- If no exact same-thread usage is known, the model context window is missing, or the selected thread changes to one without known usage, the cell displays `Unknown`.
- Switching threads must not submit input, start backend turns, or mutate backend conversation history to fill this cell.
- When exact account rate-limit status is available, the same cell appends the active-model short-window and weekly remaining percentages independently.
- Rate-limit bucket identity such as `limitId` and `limitName` is preserved. Beryl selects the bucket matching the active model and avoids merging unrelated model-specific buckets.
- Rate-limit segments are omitted independently when the exact window or active-model bucket is unavailable.
- Activating the context cell opens the context operations popup only when a backend conversation thread is selected, idle, and backend-available.
- With no selected thread, an active selected-thread turn, or a backend-unavailable selected runtime target, the cell is non-clickable.
- The context operations popup initially contains `Compact`, which starts backend context compaction for the selected thread. Request acceptance is not compaction completion.
- The app-wide context compaction timeout preference controls how long Beryl waits for backend-reported selected-thread compaction completion after the backend accepts compaction. It does not change launch, probe, connection, subscription, compact-start, active-turn, or other bounded JSON-RPC request timeouts.

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
