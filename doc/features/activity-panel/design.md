# Goals

Show bounded live and recent backend activity for the selected conversation without making operational activity part of the durable transcript.

## Non-goals

- Persisting activity history as conversation history.
- Showing command output, patch diffs, raw reasoning, tool resources, or handoff content in activity rows.
- Rendering backend thread ids as user-facing agent labels.

# Decisions

## Visibility And Layout

- The main toolbar owns an `Activity` mode control with labels `Activity Auto`, `Activity On`, and `Activity Off`.
- New workspace UI state defaults to `Activity Auto`.
- In `Activity Auto`, the panel is visible from the moment a parent turn is accepted on the conversation surface until that turn ends, and while selected-thread context compaction is active. It is hidden outside active-work periods.
- In `Activity On`, the panel remains visible between transcript and user input even when it has no rows.
- In `Activity Off`, the panel is hidden and consumes no conversation-column height.
- The mode and panel height persist as workspace-scoped GUI-local state.
- The panel is vertically resizable by dragging its top border, taking space from or returning space to the transcript region while preserving the pinned composer and status line.
- If visible rows exceed the panel height, the panel owns vertical scrolling. Otherwise it does not scroll.
- Row rendering is bounded to the viewport-visible range plus small overscan while preserving scroll geometry for the full visible row set.
- The initial viewport defaults to the top of the sorted row list, where running and newest activity appears.

## Activity Projection

- Activity is transient presentation state derived from normalized backend stream events and bounded GUI-derived records.
- Activity records are in-memory session history. They survive thread switching within the loaded workspace and are discarded on app restart or workspace/backend-session teardown.
- When the separate operator-enabled Activity diagnostic file capture is active, it may receive content-free diagnostic copies of lifecycle and presentation observations under `doc/features/diagnostics/design.md`. That evidence journal is not the Activity projection, conversation history, transcript state, or workspace state; it must not restore, retain, reorder, or otherwise affect Activity rows.
- Visible rows are scoped to the selected backend conversation thread and that thread's observed subagent activity.
- When the workspace is on a pending new-thread draft, visible activity is empty rather than stale rows from the previous selection.
- Activity state is keyed by backend thread id, turn id, and item id so lifecycle updates remain exact across overlapping threads and subagents.
- A completed multi-agent v2 `subAgentActivity` record keeps the parent event's backend thread, turn, and item identity for correlation, retention, and selected-parent visibility, while its agent label is attributed to the exact child `agentThreadId` carried by that event.
- A v2 lifecycle record with a valid child attribution uses its exact non-empty `agentPath` as the child display label immediately. The path is presentation metadata only; the exact child `agentThreadId` remains the correlation and ownership key.
- Completed-only v2 lifecycle records report the completed collaboration operation; they do not synthesize a running child lifecycle or imply that the child turn has completed.
- Running activity is retained until terminal state. Completed activity may be pruned by deterministic row, byte, and selected-thread retention windows.
- Activity presentation does not issue background metadata requests to resolve backend-generated subagent nicknames.

## Row Presentation

- Each activity item renders as one fixed-height single-line row shaped as `Agent <agent label> Activity <activity display value>`.
- Running parent-thread records sort first, with the active `Main` row at the top. Running child-thread records follow in stable first-active order: each child keeps its relative active position until its running activity ends, rows compact when an earlier child finishes, and a newly active child appends to the active child group.
- Finished records follow all running records and sort by newest completion first.
- Running, finished-ok, and finished-error rows show themed status marker discs.
- `Agent` and `Activity` labels use muted status-label styling. Values use status-value styling.
- Row text does not wrap. Long agent labels and activity display values truncate within available width.
- Parent-thread activity may use `Main` without model or reasoning metadata.
- Observed multi-agent v2 child-thread rows use the exact non-empty protocol `agentPath`, including its `/root/...` hierarchy.
- A malformed v2 child record whose required `agentPath` is missing or blank keeps an empty agent label; it does not fall back to a legacy label, nickname, or thread id.
- Pathless legacy child-thread rows use the fixed label `Subagent`; backend-generated nicknames are not displayed.
- If exact child-thread model metadata is known, a child label may append `agentPath (model)` or `Subagent (model)`. If exact reasoning effort is also known, it may append `agentPath (model/reasoning)` or `Subagent (model/reasoning)`.
- If exact model metadata is unavailable, the child label remains path-only or `Subagent`.
- Conditional model/reasoning formatting remains supported intentionally so a future CAS protocol version that supplies exact child runtime metadata through activity events or a genuinely read-only metadata response can decorate existing and later child rows without changing the presentation contract.
- Known non-subagent thread display labels may be shown only when they are real user-facing labels rather than generated from backend ids, and they do not receive subagent model/reasoning suffixes.
- Backend thread ids are correlation keys and must not render as fallback agent labels. Agent paths do not replace thread ids for attribution, ownership, or lifecycle correlation.
- Missing model or reasoning metadata is not inferred from defaults, model-list metadata, thread ids, agent paths, or nicknames.

## Activity Display Values

- V1 rows render protocol-derived display values without broad human-friendly mapping, except for GUI-derived subagent handoff byte counts.
- `commandExecution` rows use the first non-empty command line and fall back to `commandExecution` when unavailable.
- Before display, if the first quoted or unquoted command token case-insensitively matches a drive-rooted Windows PowerShell launcher path shaped as `[drive]:\Windows(\.old)?\System32\WindowsPowerShell\v1.0\powershell.exe`, including doubled-backslash activity-log forms, that token is replaced with `powershell.exe` while preserving the rest of the command line.
- Reasoning rows render `reasoning` and, when backend summary text is exposed, a bounded single-line `reasoning: <summary>` value.
- `fileChange` rows render `Patching <relative/path>, +A -D` only when explicit backend file-change records identify exactly one unique path and the path is relative or can be proven under the selected conversation execution target root.
- Otherwise `fileChange` rows render `Patching N file(s), +A -D`.
- File count and addition/deletion counts derive only from explicit backend file-change records.
- Rows must not infer file paths from command text, diffs, or non-`fileChange` activity, and must not show absolute or outside-root paths as fallbacks.
- Other activity rows use raw protocol-derived tool names or resource identifiers.
- Subagent handoff rows are GUI-derived completed rows for observed child-thread final-answer `agentMessage` completions. They render `handoff: N bytes`, where `N` is the UTF-8 byte length of the handoff text.
- Row bodies omit output, progress messages, resource contents, file paths other than the allowed single relative `fileChange` path, patch diffs, raw reasoning content, handoff content, previews, and expanded operational detail.

## Backend Interaction

- Activity rendering reads the current selected-thread activity projection and does not synchronously query `codex app-server`.
- Exact subagent model/reasoning metadata may come from normalized activity events or later read-only metadata responses.
- Beryl does not issue `thread/read` requests merely to decorate activity rows with backend-generated subagent nicknames.
- Beryl does not issue `thread/resume` or another activation or subscription operation solely to discover child model/reasoning metadata or decorate Activity rows. When no exact event or genuinely read-only metadata source provides those values, the suffix remains absent.
