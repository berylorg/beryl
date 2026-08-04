# Goals

Show bounded live and recent backend activity for the selected conversation without making operational activity part of the durable transcript.

## Non-goals

- Persisting activity history as conversation history.
- Showing command output, patch diffs, raw reasoning, tool resources, or handoff content in activity rows.
- Rendering backend thread ids as user-facing agent labels.

# Decisions

## GUI Supplement

- `gui.md` is the normative supplemental GUI composition file for mounting and configuring the project-local `activity panel` widget. Reusable resize, row-viewport, scrolling, and layout mechanics live in that widget's spec.

## Visibility And Layout

- The activity panel has no main-toolbar mode control. Its visibility behavior is fixed to Auto.
- In Auto behavior, the panel is visible from the moment a parent turn is accepted for the selected thread until that turn ends, and while selected-thread context compaction is active. It is hidden outside active-work periods.
- The panel height persists as window-local Beryl-home state.
- The panel is vertically resizable by dragging its top border, taking space from or returning space to the transcript region while preserving any discussion-status strip, the pinned composer, and the global status line.
- If visible rows exceed the panel height, the panel owns vertical scrolling. Otherwise it does not scroll.
- Row rendering and residency are bounded to revision-matched viewport pages plus small overscan
  while total-count geometry represents the full logical result set.
- The initial viewport defaults to the top of the sorted row list, where running and newest activity appears.

## Activity Projection

- Activity is a transient revision-bound presentation query over already admitted Syndic provider
  lifecycle records and bounded GUI-derived facts. It does not duplicate provider payloads or keep
  a parallel whole-session list.
- The query exposes a logical count and bounded resident result pages. Its current-work-period scope
  survives thread switching, while restart or managed-runtime teardown makes the old process-period
  projection ineligible for display.
- Visible rows are scoped to the selected backend conversation thread and that thread's observed subagent activity.
- A pristine draft-only selected thread has no visible activity until its first turn is accepted.
- Activity state is keyed by backend thread id, turn id, and item id so lifecycle updates remain exact across overlapping threads and subagents.
- Running activity remains logically queryable until terminal state but does not pin a resident row
  page. Completed activity is pruned from the presentation query by deterministic row, byte, and
  selected-thread retention windows.
- Row labels, activity values, tooltips, and accessibility strings are bounded projections over
  source ranges. Truncation is explicit; asking for complete source content never materializes it in
  this panel.
- Background metadata resolution for unresolved subagent names is bounded, cancellable, and lower priority than foreground turn streaming.

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
- `commandExecution` rows scan source ranges incrementally for the first non-empty command line,
  retain only its bounded display prefix, and fall back to `commandExecution` when unavailable. They
  never materialize the full command or command array to derive one row.
- Before display, if the first quoted or unquoted command token case-insensitively matches a drive-rooted Windows PowerShell launcher path shaped as `[drive]:\Windows(\.old)?\System32\WindowsPowerShell\v1.0\powershell.exe`, including doubled-backslash activity-log forms, that token is replaced with `powershell.exe` while preserving the rest of the command line.
- Reasoning rows render `reasoning` and, when backend summary text is exposed, a bounded single-line `reasoning: <summary>` value.
- `fileChange` rows render `Patching <relative/path>, +A -D` only when explicit backend file-change records identify exactly one unique path and the path is relative or can be proven under the selected conversation execution target root.
- Otherwise `fileChange` rows render `Patching N file(s), +A -D`.
- File count and addition/deletion counts derive only from explicit backend file-change records and
  accumulate in fixed-width checked counters while those records stream.
- Rows must not infer file paths from command text, diffs, or non-`fileChange` activity, and must not show absolute or outside-root paths as fallbacks.
- Other activity rows use raw protocol-derived tool names or resource identifiers.
- Subagent handoff rows are GUI-derived completed rows for observed child-thread final-answer
  `agentMessage` completions. They render `handoff: N bytes`, where `N` comes from the exact source
  range length or streaming frontier rather than rereading or retaining the handoff text.
- Row bodies omit output, progress messages, resource contents, file paths other than the allowed single relative `fileChange` path, patch diffs, raw reasoning content, handoff content, previews, and expanded operational detail.

## Backend Interaction

- Activity rendering reads the current selected-thread activity projection and does not synchronously query `codex app-server`.
- Exact subagent model/reasoning metadata comes from normalized activity events. The pinned
  metadata-only read contributes nickname and provider identity but exposes no model or reasoning
  value, so the activity projection must not present the provider as a model.
- Beryl may resolve unresolved subagent nicknames through independent backend maintenance client sessions that issue metadata-only `thread/read` requests outside render paths.
- Beryl must not use `thread/resume` merely to decorate activity rows unless a later design explicitly accepts the load and subscription side effects.
