# Goals

Show bounded live and recent backend activity for the selected conversation without making operational activity part of the durable transcript.

## Non-goals

- Persisting activity history as conversation history.
- Showing command output, patch diffs, raw reasoning, tool resources, or handoff content in activity rows.
- Rendering backend thread ids as user-facing agent labels.

# Decisions

## GUI Supplement

- [`gui.md`](gui.md) is the normative supplemental GUI composition file for mounting and configuring the project-local `activity panel` widget. Reusable resize, row-viewport, scrolling, and layout mechanics live in that widget's spec.

## Visibility And Layout

- The activity panel has no main-toolbar mode control. Its visibility behavior is fixed to Auto.
- In Auto behavior, the panel is visible from the moment a parent turn is accepted for the selected
  thread until that turn ends, and while selected-thread context compaction is active. It is hidden
  when the selected thread has neither an active parent turn nor active context compaction.
- The panel height persists as window-local Beryl-home state.
- The panel is vertically resizable by dragging its top border, taking space from or returning space to the transcript region while preserving any discussion-status strip, the pinned composer, and the global status line.
- If visible rows exceed the panel height, the panel owns vertical scrolling. Otherwise it does not scroll.
- Row rendering remains bounded to the visible viewport plus small overscan while scrollbar
  geometry represents the complete eligible activity collection.
- The initial viewport defaults to the top of the sorted row list, where running and newest activity appears.

## Activity Collection

- The panel presents already admitted lifecycle activity and bounded GUI-derived facts. It does not
  show unadmitted backend events or duplicate payload content.
- A `runtime activity period` is the visible activity lifetime of one continuously usable managed
  runtime. Thread switching, turn completion, and starting later turns do not end the period.
  Process restart, managed-runtime teardown or replacement, and same-home recovery that replaces
  runtime services end it exactly.
- Eligible completed rows survive later turns within the same runtime activity period until the
  deterministic recent-activity bounds remove them. When Auto next shows the panel, those retained
  rows may appear beneath current running and newer completed work; a later turn does not clear the
  collection merely because it started.
- Ending a runtime activity period immediately makes every row from that period ineligible for
  current display. If its last coherent panel remains on screen during replacement or recovery, it
  is visibly reconciling and inert; it cannot be focused, scrolled, activated, or treated as current.
- A late activity or metadata result belonging to an ended runtime activity period is discarded. It
  cannot change rows, counts, focus, tooltips, scroll position, visibility, or the next period's
  collection.
- A replacement runtime activity period becomes interactive only after one coherent fresh
  collection for that period is available. Beryl never combines rows from the ended and replacement
  periods; if the selected thread has no eligible activity in the replacement period, ordinary Auto
  behavior leaves the panel hidden.
- Visible rows are scoped to the selected backend conversation thread and that thread's observed subagent activity.
- A pristine draft-only selected thread has no visible activity until its first turn is accepted.
- Lifecycle updates preserve exact row identity across overlapping threads and subagents.
- Running activity remains available until terminal state. Completed activity is retained only
  within deterministic recent-activity bounds; leaving the viewport never changes logical state.
- Row labels, activity values, tooltips, and accessibility strings are bounded projections over
  source ranges. Truncation is explicit; asking for complete source content never materializes it in
  this panel.

## Collection Failure And Retry

- If the initial query for the current activity collection fails before coherent rows are
  available, the Auto-eligible panel remains visible with bounded failure feedback and a `Retry`
  command. The failure is not presented as an empty collection and does not hide the panel.
- If a later page query fails, the panel preserves its coherent rows, count, and scroll position and
  presents bounded failure feedback with `Retry` for that failed page. It does not clear the
  collection, fabricate rows, or present the collection as complete past the failed boundary.
- Retry repeats only the failed initial query or page request for the same selected thread and
  current runtime activity period. While it is pending, the feedback and command remain visible,
  `Retry` is unavailable, and repeated activation cannot create a duplicate request.
- A repeated failure updates the same feedback and makes `Retry` available again while its scope is
  still current. Success removes that feedback only after coherent current-period results are
  available; a thread or runtime-activity-period change makes the old result ineligible.

## Row Presentation

- Each activity item renders as one fixed-height single-line row shaped as `Agent <agent label> Activity <activity display value>`.
- Rows sort running records before finished records, then newest start time first within each group.
- Running, finished-ok, and finished-error rows show themed status marker discs.
- `Agent` and `Activity` labels use muted status-label styling. Values use status-value styling.
- Row text does not wrap. Long agent labels and activity display values truncate within available width.
- Parent-thread activity may use `Main` without model or reasoning metadata.
- Observed subagent child-thread rows use backend-provided subagent nicknames after resolution.
- A nickname outside the backend's 1,024-byte bounded display-label domain remains unresolved
  rather than being truncated into a different agent identity; visual row truncation applies only
  after one exact bounded nickname has been accepted.
- If exact child-thread model metadata is known, a resolved subagent label may append `nickname (model)`. If exact reasoning effort is also known, it may append `nickname (model/reasoning)`.
- If exact model metadata is unavailable, a resolved subagent remains nickname-only.
- Known non-subagent thread display labels may be shown only when they are real user-facing labels rather than generated from backend ids, and they do not receive subagent model/reasoning suffixes.
- Backend thread ids are correlation keys and must not render as fallback agent labels. Unresolved subagent rows keep the agent value empty until nickname resolution succeeds.
- Missing model or reasoning metadata is not inferred from defaults, model-list metadata, thread ids, or nicknames.

## Activity Display Values

- V1 rows render protocol-derived display values without broad human-friendly mapping, except for GUI-derived subagent handoff byte counts.
- `commandExecution` rows show the bounded first non-empty command-line projection and fall back to
  `commandExecution` when unavailable.
- Before display, if the first quoted or unquoted command token case-insensitively matches a drive-rooted Windows PowerShell launcher path shaped as `[drive]:\Windows(\.old)?\System32\WindowsPowerShell\v1.0\powershell.exe`, including doubled-backslash activity-log forms, that token is replaced with `powershell.exe` while preserving the rest of the command line.
- Reasoning rows render `reasoning` and, when backend summary text is exposed, a bounded single-line `reasoning: <summary>` value.
- `fileChange` rows render `Patching <relative/path>, +A -D` only when explicit backend file-change records identify exactly one unique path and the path is relative or can be proven under the selected conversation execution target root.
- Otherwise `fileChange` rows render `Patching N file(s), +A -D`.
- File count and addition/deletion counts derive only from explicit backend file-change records.
- Rows must not infer file paths from command text, diffs, or non-`fileChange` activity, and must not show absolute or outside-root paths as fallbacks.
- Other activity rows use raw protocol-derived tool names or resource identifiers.
- Subagent handoff rows are GUI-derived completed rows for observed child-thread final-answer
  `agentMessage` completions. They render `handoff: N bytes` from the exact completed handoff byte
  length without retaining or displaying the handoff text.
- Row bodies omit output, progress messages, resource contents, file paths other than the allowed single relative `fileChange` path, patch diffs, raw reasoning content, handoff content, previews, and expanded operational detail.

## Metadata Availability

- Rendering the panel never pauses to synchronously contact the backend. A row with unresolved
  optional metadata remains usable and updates in place when exact metadata later becomes
  available, preserving row identity and viewport position.
- Model and reasoning suffixes appear only from exact normalized activity metadata. A provider
  identity is not presented as a model, and missing values are never inferred.
- Metadata decoration never resumes a thread or changes its backend lifecycle merely to improve an
  activity label.
