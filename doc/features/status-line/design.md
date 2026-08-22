# Goals

Expose compact, exact conversation status and selected-thread controls without mutating backend history, guessing unavailable backend state, or hiding disabled reasons.

## Non-goals

- Estimating context space from transcript text, model names, local tokenization, or accumulated spend.
- Applying model or reasoning changes before the next real user turn by starting synthetic backend turns.
- Providing a hard-stop, escalation, child-termination, or background-cleanup command.

# Decisions

## GUI Supplement

- [`gui.md`](gui.md) is the normative supplemental GUI composition file for configuring bundled
  status and popup widgets with this feature's cells, values, operations, and availability.

## Model And Reasoning Cell

- The model/reasoning cell displays the selected thread's active or pending model and reasoning effort.
- For a draft-only thread, it displays the current draft's explicit first-turn selection when present; otherwise it displays the effective backend defaults for that thread's execution root.
- Missing model or reasoning values render `Unknown`. Beryl must not infer effective reasoning from model-list menu defaults.
- Exact backend-derived values already known may remain visible when a runtime is backend-unavailable; otherwise the cell renders `Unknown` without launching or probing a backend.
- The cell opens a model/reasoning popup only when a selected Syndic thread is idle and its runtime is available.
- With no selected thread or no known value, the cell is passive. With an active selected-thread
  turn it remains visibly disabled with `Model changes are unavailable while the turn is active.`
  With an unavailable selected runtime it remains visibly disabled with `Runtime is unavailable.`
- The popup progressively presents models supported by the selected runtime and only the reasoning
  choices supported by the selected model. Results for a runtime or selection that is no longer
  current never replace the visible choices.
- While the initial model query is pending, the popup remains open with bounded loading feedback.
  An exact successful query with no supported models presents an explicit empty result rather than
  failure feedback or inferred choices.
- If the initial model query fails, the popup remains open with bounded failure feedback and a
  `Retry` command instead of presenting the failed result as an empty model list.
- If a later page query fails, the popup preserves the already presented model choices and current
  selection, marks the progressive result incomplete, and exposes `Retry` for that failed page.
- Retry repeats only the failed query for the same popup and selected runtime. While retry is
  pending, its feedback and command remain visible, `Retry` is unavailable, and repeated activation
  cannot create a duplicate request. Closing the popup or changing the selected thread or runtime
  makes the old result ineligible to change later choices.
- A repeated failure updates the same popup feedback and makes `Retry` available again while its
  scope is current. Successful retry resumes the same progressive model-selection workflow. A
  model-query failure does not itself change the displayed current model or reasoning value or
  create a backend-unavailable notice.
- Choosing a model or reasoning effort updates the selected thread's current-draft or next-turn defaults only. It does not mutate global Codex configuration or other threads.
- Existing-thread selections are carried on the next submitted user turn for that thread. The backend-owned thread default is then the source for later status presentation.
- Draft-only-thread selections are carried on the first submitted user turn. A draft without explicit selection follows current effective backend defaults until submission or explicit user choice.

## Context And Rate-Limit Cell

- Context space displays a percentage only when the selected thread has exact token usage with a positive model context window.
- The percentage is computed from exact selected-thread token usage as `((modelContextWindow - last.inputTokens) / modelContextWindow) * 100`, clamped to `0..100`.
- If no exact same-thread usage is known, the model context window is missing, or the selected thread changes to one without known usage, the cell displays `Unknown`.
- Switching threads must not submit input, start backend turns, or mutate backend conversation history to fill this cell.
- When exact account rate-limit status is available, the same cell appends the active-model short-window and weekly remaining percentages independently.
- Beryl selects only the exact bucket matching the active model and never merges unrelated
  model-specific buckets. An ambiguous, malformed, or missing match is unavailable rather than
  inferred from labels or another model.
- Rate-limit segments are omitted independently when the exact window or exact active-model bucket is unavailable.
- Activating the context cell opens the context operations popup only when a backend conversation thread is selected, idle, and backend-available.
- With no selected thread the cell is passive. With an active selected-thread turn it remains
  visibly disabled with `Compaction is unavailable while the turn is active.` With an unavailable
  selected runtime it remains visibly disabled with `Runtime is unavailable.`
- The context operations popup initially contains `Compact`, which starts backend context compaction for the selected thread. Request acceptance is not compaction completion.
- `Compact` is enabled only when the selected thread is idle and backend-available, has no already
  accepted next-turn input, no repair-pending turn, and no other operation in progress. Otherwise
  the row stays disabled with a localized reason; it never displaces queued user work or asks the
  backend to replace an active task.
- Manual compaction does not submit or clear the composer draft, add an ordinary conversation
  message, or start a model response.
- The app-wide context compaction timeout is a whole number of seconds in the inclusive range
  `1..=86400` and defaults to `180` when no value has been saved.
- An admitted operation keeps the timeout value active when it started; later settings changes
  affect only later operations.
- Expiry reports that compaction is still in progress. It does not show success, interrupt or repeat
  the operation, release queued input, or clear `compacting`; a later exact outcome still updates
  the visible state.
- Rejection, failure, interruption, unknown completion, and lost backend authority receive bounded
  operation feedback and never appear as successful compaction merely because the start request
  was accepted or no further progress is visible.

## Turn State, View Count, And Stop Controls

- Last-turn state displays `compacting` while selected-thread context compaction is active;
  `repair pending` for a proven-terminal turn awaiting history repair; `repaired` after successful
  repair; `incomplete` for explicit incomplete convergence; `unknown terminal` while terminal
  outcome cannot be established; `working` only for an ordinary active parent turn with exact live
  authority; `ok` only after an ordinary complete turn; `error` only after an ordinary failed turn;
  `interrupted` after an ordinary interrupted turn; and `Unknown` before any turn state is known.
- `repair pending`, `repaired`, `incomplete`, `unknown terminal`, and `interrupted` never collapse to
  `working`, `ok`, or a missing value merely because some transcript content remains visible.
  `interrupted` is a distinct terminal presentation, not an error, whether or not the backend
  supplied error detail.
- An ordinary turn with a requested soft stop remains `working`, while a context-compaction request
  remains `compacting`, until the system supplies an exact terminal or authority-loss outcome.
  Stop-request progress and failure belong to the operations popup or its notice fallback; the
  status line does not invent a terminal result or add a second `stopping` last-turn state.
- The turn cell appends a secondary `View` segment that reports the backend turn number currently represented at the transcript viewport bottom as `<current>/<total>`, such as `working View 5/5`.
- `View` numbers are one-based chronological backend parent-turn numbers for the selected thread. They are not transcript presentation row numbers, loaded detail counts, or Markdown block counts.
- A synthetic discussion-context item never increments `current` or `total`. When it is the lowest intersecting presentation item, `current` remains the real source turn immediately preceding its branch-boundary position.
- The `View` segment is passive and never loads additional history, starts backend work, or changes
  transcript state merely to fill a value.
- The `total` value appears only when the selected thread's exact total is known. Beryl does not use
  the currently loaded history length when older history may exist.
- The `current` value is the real backend turn represented at the viewport bottom. If the bottom is
  trailing space after the final turn, `current` is that final turn.
- Missing current or total values render as `-` in the count segment. With no known turns, including an exact zero-turn selected thread, the view segment is `-/-`; when total is exact and positive but the viewport turn cannot be identified it is `-/N`.
- The `View` segment must not make the turn cell interactive when stop controls are otherwise unavailable.
- Interrupted turns without actual error payload update status but do not automatically enqueue
  turn-error notices.
- Terminal and recovery states, including `ok`, `error`, `interrupted`, `repair pending`, `repaired`,
  `incomplete`, and `unknown terminal`, have no eligible `Soft stop` and do not open a stop-capable
  turn operations popup.
- When the selected operation enters any such state, an open turn popup closes. Required exact stop
  feedback moves to Notifications; interrupted completion uses the informational stop-completion
  record and does not create a separate `Turn error` notice solely because interruption occurred.
- `Soft stop` availability consumes only the one opaque exact-soft-stop eligibility fact supplied
  by the CAS-live system for the selected operation. The status line never derives eligibility from
  runtime or backend availability, a known turn id, the displayed `working` or `compacting` state,
  or any combination of those observations.
- While that fact is present, activating the turn segment opens the popup with its single `Soft
  stop` row. Without it, a terminal segment is passive; an otherwise active turn or compaction
  segment is visibly disabled with the system-supplied closest eligibility reason unless exact
  feedback-only state for a request already in progress keeps the feedback popup available.
- A compaction operation may be visibly `compacting` before the eligibility fact exists. During that
  interval the segment remains visibly disabled with the supplied eligibility reason; Beryl never
  guesses a target or substitutes thread-wide cleanup.
- `Soft stop` requests backend interruption for the exact selected-thread active turn or compaction
  operation. Request progress and bounded failure feedback remain accessible through the turn
  operations popup while its exact segment anchor survives, or through the established notice when
  the popup can no longer retain that feedback safely.
- Request acceptance is not terminal turn completion. The visible parent state changes only after
  an exact terminal or authority-loss outcome.
- After a durably admitted stop is proven locally nondispatched, controls reopen only when CAS-live
  separately supplies a new exact-soft-stop eligibility fact for that same still-active operation.
  Nondispatch by itself is not eligibility.
- A volatile pre-admission fallback that is proven nondispatched ends with one bounded request-
  failure message. It does not re-enable `Soft stop`, expose retry or join, imply a durable stop
  operation, or claim that the target reached terminal state.
- While the system supplies feedback-only state, the turn segment remains action-menu-capable and
  its popup retains the one disabled `Soft stop` row with the latest bounded request feedback.
  Duplicate activation cannot dispatch another request.
- When the exact popup anchor cannot safely retain required stop progress or outcome feedback, this
  feature contributes the current bounded exact stop feedback to Notifications. It remains visibly
  associated with the originating request and never moves to a later turn or compaction.
- Feedback persists while the request awaits terminal or authority-loss convergence. Durable
  nondispatch, volatile nondispatch, interrupted terminal, and final authority-loss outcomes use
  the resolved presentation defined by Notifications. Interrupted completion remains localized
  exact feedback even without a backend error payload; it does not become a `Turn error` notice
  solely because the interruption completed.
- This feature never mounts a notice independently. Notifications owns priority, preemption,
  persistence, dismissal, and the sole visible notice instance.
- Exact soft stop is the only stop command. No hard-stop row, escalation, child/subagent
  termination, command-process termination, or background-cleanup fallback is exposed.
- User input fragments queued before or during stop remain visible and ordered. If they cannot be delivered to the interrupted turn, they remain queued for the next eligible turn.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers:

- `external-side-effects/v1`
