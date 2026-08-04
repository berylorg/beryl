# Goals

Expose compact, exact conversation status and selected-thread controls without mutating backend history, guessing unavailable backend state, or hiding disabled reasons.

## Non-goals

- Estimating context space from transcript text, model names, local tokenization, or accumulated spend.
- Applying model or reasoning changes before the next real user turn by starting synthetic backend turns.
- Terminating guessed OS processes for hard stop.

# Decisions

## GUI Supplement

- `gui.md` is the normative supplemental GUI composition file for configuring bundled status and popup widgets with this feature's cells, values, operations, and availability.

## Status Line Layout

- The status line strip is fixed to the bottom edge of the main window between the user input panel and the OS window edge.
- It uses the same edge-to-edge separator treatment as the main toolbar.
- It contains three left-to-right cells: model/reasoning, context space left, and turn state plus transcript-view turn position.
- The strip is UI chrome and is not part of the backend conversation transcript.

## Model And Reasoning Cell

- The model/reasoning cell displays the selected thread's active or pending model and reasoning effort.
- For a draft-only thread, it displays the current draft's explicit first-turn selection when present; otherwise it displays the effective backend defaults for that thread's execution root.
- Missing values render `Unknown` or unavailable. Beryl must not infer effective reasoning from model-list menu defaults.
- Backend-derived values already known may remain visible when a runtime is backend-unavailable; otherwise the cell renders unavailable/unknown without launching or probing a backend.
- The cell opens a model/reasoning popup only when a selected Syndic thread is idle and its runtime is available.
- With an active selected-thread turn or backend-unavailable selected runtime, the cell is non-clickable.
- The popup queries backend-supported models one bounded `model/list` cursor page at a time and
  restricts reasoning choices to the selected model's bounded supported-effort projection. It never
  waits for or retains an aggregated all-model response.
- A popup query is tied to the exact selected runtime and backend-session generation. Closing the
  popup, changing that identity, or receiving a stale cursor result cancels the query and releases
  its resident pages.
- Choosing a model or reasoning effort updates the selected thread's current-draft or next-turn defaults only. It does not mutate global Codex configuration or other threads.
- Existing-thread selections are carried on the next submitted user turn for that thread. The backend-owned thread default is then the source for later status presentation.
- Draft-only-thread selections are carried on the first submitted user turn. A draft without explicit selection follows current effective backend defaults until submission or explicit user choice.

## Context And Rate-Limit Cell

- Context space displays a percentage only when the selected thread has exact token usage with a positive model context window.
- The percentage is computed from exact selected-thread token usage as `((modelContextWindow - last.inputTokens) / modelContextWindow) * 100`, clamped to `0..100`.
- Exact token usage is an intrinsic Syndic thread observation. Selected-thread
  `thread/tokenUsage/updated` notifications may update a fixed-capacity compact same-thread cache,
  but a durable value becomes authoritative only after an exact authenticated publication to the
  Syndic thread-usage record. The status line may use that exact just-published value or a Syndic
  point read for the selected thread. GUI state and read-only app-server status metadata are never
  durable usage authority, and no path loads usage snapshots for every thread into memory.
- If no exact same-thread usage is known, the model context window is missing, or the selected thread changes to one without known usage, the cell displays `Unknown`.
- Switching threads must not submit input, start backend turns, or mutate backend conversation history to fill this cell.
- When exact account rate-limit status is available, the same cell appends the active-model short-window and weekly remaining percentages independently.
- Rate-limit bucket identity such as `limitId` and `limitName` is preserved. Beryl selects the bucket matching the active model and avoids merging unrelated model-specific buckets.
- A required `limitId` outside the backend's 256-byte protocol-identity domain or required
  `limitName` outside its 1,024-byte display-label domain makes that candidate unavailable; Beryl
  does not truncate either value and then claim an exact active-model match.
- Rate-limit notifications and status responses are scanned incrementally. Beryl retains only the
  bounded exact active-model match and fixed short/weekly window facts needed by this cell; it does
  not deserialize or cache the complete bucket collection. Ambiguous duplicate matches remain
  unavailable rather than growing retained candidates.
- Rate-limit segments are omitted independently when the exact window or active-model bucket is unavailable.
- Activating the context cell opens the context operations popup only when a backend conversation thread is selected, idle, and backend-available.
- With no selected thread, an active selected-thread turn, or an unavailable selected runtime, the cell is non-clickable.
- The context operations popup initially contains `Compact`, which starts backend context compaction for the selected thread. Request acceptance is not compaction completion.
- `Compact` is enabled only when the selected thread is idle and backend-available, has no already
  accepted next-turn input, and has no other operation in progress. Otherwise the row stays
  disabled with a localized reason; it never displaces queued user work or asks the backend to
  replace an active task.
- Manual compaction does not submit or clear the composer draft, add an ordinary conversation
  message, or start a model response.
- The app-wide context compaction timeout is a whole number of seconds in the inclusive range
  `1..=86400` and defaults to `180` when no value has been saved.
- One operation snapshots the active timeout when it is admitted, but its completion timer starts
  only after the backend accepts the compaction request. A later settings change affects only a
  later operation, and the setting controls completion waiting rather than request handling.
- Expiry ends only the current bounded completion wait and reports that the operation is still in
  progress. It does not mark compaction complete, interrupt it, repeat it, release queued input, or
  clear `compacting`. Exact later backend completion or loss evidence still settles the operation.
- Rejection, failure, interruption, unknown completion, and lost backend authority receive bounded
  operation feedback and never appear as successful compaction merely because the start request
  was accepted or no further progress is visible.

## Turn State, View Count, And Stop Controls

- Last-turn state displays `compacting` while selected-thread context compaction is active, `working` while a parent turn is active, `ok` after the latest completed turn, `error` after the latest failed or interrupted turn, and `Unknown` before any turn state is known.
- A durably admitted stop remains `working` until exact terminal or authority-loss evidence changes
  the turn state. Stop-request progress and failure belong to the operations popup; the status line
  does not invent a terminal result or add a second `stopping` last-turn state.
- The turn cell appends a secondary `View` segment that reports the backend turn number currently represented at the transcript viewport bottom as `<current>/<total>`, such as `working View 5/5`.
- `View` numbers are one-based chronological backend parent-turn numbers for the selected thread. They are not transcript presentation row numbers, loaded detail counts, or Markdown block counts.
- A synthetic discussion-context item never increments `current` or `total`. When it is the lowest intersecting presentation item, `current` remains the real source turn immediately preceding its branch-boundary position.
- The `View` segment is passive status chrome. It consumes transcript host facts for viewport ownership, source-turn ownership, and exact numbering that are already present in shell state.
- The status line does not inspect transcript residency internals, presentation records, renderer state, Syndic storage, backend history, or rendered text to derive `View` values.
- The `View` segment must not start selected-thread turn pagination, create background workers, call backend connectors, trigger synthetic turns, load transcript details, scan rendered text, or mutate transcript history/list state.
- The `total` value is shown only when transcript host facts prove an exact selected-thread turn total. Beryl must not use the retained transcript-window length as the total when older history may still be unloaded.
- With the current backend contract, exact total before the transcript owns complete selected-thread history requires a CAS/backend exact-total field. Without that field, total remains `-` until transcript history completeness proves it.
- The `current` value is the backend turn that owns the last real resident transcript content record intersecting the transcript viewport. If the viewport bottom is in virtual trailing scroll space, the current value is the final backend turn when at least one turn exists.
- Missing current or total values render as `-` in the count segment. With no known turns, including an exact zero-turn selected thread, the view segment is `-/-`; when total is exact and positive but the viewport turn cannot be identified it is `-/N`.
- The `View` segment must not make the turn cell interactive when stop controls are otherwise unavailable.
- Interrupted turns without actual error payload update status but do not automatically enqueue turn-error notices.
- Activating the cell opens the turn operations popup only when the selected runtime is backend-available and Beryl knows an interruptible backend turn id for the selected ordinary active turn or selected-thread compaction operation.
- Otherwise the cell is non-clickable.
- A compaction operation is visible as `compacting` before CAS publishes its turn id, but it is not
  interruptible during that interval. Beryl neither guesses a turn target nor turns a local stop
  request into thread-wide cleanup. After the exact compaction turn id is durably published, the
  ordinary exact stop controls target that provider operation.
- `Soft stop` requests backend interruption for the exact selected-thread active turn or compaction operation, then closes or reports request failure through popup feedback.
- Request acceptance is not terminal turn completion. Visible parent state converges only from
  exact provider terminal evidence or projection-authority-loss convergence; a hard-target response
  cannot terminalize the selected parent operation.
- Only a local failure proven before any interruption request byte crossed may reopen stop controls
  for the still-exact active operation. Input already preserved for the next turn is not
  retroactively offered as steering. A pinned provider rejection proves no core interruption but
  not that the selected target remains current, so it retires the uncertain projection and
  converges through authority loss instead of reopening controls. Timeout, transport loss,
  malformed response, or another outcome after possible dispatch leaves the exact operation
  stopping without an automatic or user-interface retry of the primary interruption; hard
  escalation remains available only while the exact live target, foreground session, and
  authenticated handles remain.
- `Hard stop` first performs the same selected-operation interruption as soft stop, then best-effort
  invokes only release-proven exact individual targets plus the separately disclosed coarse exact-
  thread cleanup target when eligible.
- Hard stop may interrupt known active subagent turns only when the provider exposes an exact
  targeted child-turn primitive and Beryl owns its required target fence. Unless retained exact-
  0.146.0 evidence proves both conditions, child or subagent interruption is reported as
  unsupported. Individual turn-process ids likewise remain unsupported as exact hard-stop handles
  unless exact-release evidence proves an ABA-safe identity. Coarse thread-scoped background-
  terminal cleanup remains eligible, is explicitly
  thread-wide, and runs last.
- Hard stop never terminates by guessed OS pid, process name, working directory, or local process tree.
- If a running tool or subagent cannot be mapped to an exact individual backend termination handle,
  Beryl does not target it individually and reports the unsupported limitation. When the separate
  coarse thread-cleanup target is eligible, its disclosed thread-wide effect may still include
  running background terminals without claiming individual identity.
- Hard stop is a held action. Holding the row for three seconds triggers it once; releasing early, leaving the row, closing the popup, focus loss, or selected active-turn target change cancels.
- Keyboard activation must provide the same held affordance for the focused row.
- While stopping, the soft row cannot create another primary request; another caller joins the
  existing exact stop. Hard escalation remains separately available when its exact prerequisites
  hold.
- Partial hard-stop failures and unsupported targets are reported through status-operation
  feedback. Pinned thread-cleanup success means only that CAS accepted the coarse cleanup request;
  it is not shown as observed process exit or completed cleanup.
- Hard escalation freezes one bounded snapshot of exact backend-exposed targets associated with the
  selected operation when it attaches to the durably admitted stop. It proceeds once after matching
  primary acceptance or local proven nondispatch while the exact foreground session survives;
  failure of one escalation target does not suppress the remaining frozen targets. Provider
  rejection or primary completion unknown instead reports every unattempted frozen target
  unavailable because no authorized hard-target dispatch path remains.
- When the primary hard-stop interruption is proven not dispatched, controls reopen only after the
  frozen escalation run finishes, only if the selected parent operation remains exact, and never
  when an interrupting-approval obligation joined the stop.
- For context compaction, pinned hard escalation has no child, command, or coarse thread-cleanup
  target beyond the primary operation interruption. It never borrows handles or cleanup
  eligibility from an ordinary parent turn.
- User input fragments queued before or during stop remain visible and ordered. If they cannot be delivered to the interrupted turn, they remain queued for the next eligible turn.
