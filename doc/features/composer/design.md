# Goals

Provide one reliable composer for new threads, existing threads, active-turn steering, compaction-time queueing, image input, history browsing, and transcript quote insertion without confusing user-authored draft text with GUI-owned metadata.

## Non-goals

- Rendering image thumbnails inline inside the draft.
- Persisting composer history across app restarts.
- Treating manually typed text such as `[Image A]` as an image attachment.
- Clearing drafts on rejected submission.

# Decisions

## Composer Surface

- The user input panel is pinned above the status line inside the conversation column.
- The same composer layout is used for selected conversation threads and pending new-thread drafts.
- The panel automatically grows and shrinks with wrapped draft line count up to half the OS window height, further clamped to preserve the transcript region minimum height.
- The panel is not manually resizable and does not expose a persistent `Run Turn` or submit button.
- The input field wraps text at its visible width and does not horizontally scroll. When wrapped content exceeds the panel's maximum height, the field owns vertical scrolling and keeps the caret or active selection endpoint reachable.
- Backend-unavailable or otherwise submission-disabled states render the composer disabled for submission while preserving draft text, image markers, caret, selection, and undo history.
- A draft containing at least one image marker and no non-whitespace text is non-empty for submission.

## Shared Text Input Integration

- Baseline text editing, selection, clipboard, undo/redo, IME, and pointer behavior comes from the shared text-input contract in `doc/input-hotkeys.md`.
- The composer reserves focused `Enter` for submission and focused `Shift+Enter` for inserting a real newline.
- When thread-edit mode is active, focused `Enter` attempts edit commit instead of ordinary turn start, active-turn steering, or compaction-time queueing.
- When thread-edit mode is active and no higher-priority popup handles `Escape`, focused `Escape` cancels edit mode without mutating the draft buffer, image markers, caret, selection, or undo history.
- While thread-edit mode is inactive, focused `Alt+Up` and `Alt+Down` browse accepted composer history for the current conversation scope.
- Focused `Ctrl+Up` and `Ctrl+Down` scroll the transcript between turn boundaries without moving the draft caret or changing selection.
- `Ctrl+Up` first moves to the top of a tall current turn before earlier jumps move to previous turns. `Ctrl+Down` scrolls to the transcript bottom when no later turn boundary exists.

## Image Markers And Assets

- Pasting image clipboard content into the composer inserts an inline image marker at the caret or replaces the selected draft range.
- Pasted images are stored as durable Beryl image assets under workspace-local state for backend submission, composer preview, accepted transcript markers, retries, and preview after restart.
- Image labels are allocated from selected-thread or pending-new-thread monotonic label state as `A`, `B`, `C`, continuing spreadsheet-style when needed.
- Labels remain stable while the draft, accepted fragment, queued fragment, or retry state exists. Removing label `B` does not rename later labels or allow reuse in the same scope while surrounding text may refer to it.
- Multiple markers may show the same label only when they reference the same pasted image payload.
- The compact visual marker such as `[A]` is presentation. It is not submitted as literal user-authored text.
- On submission, Beryl sends the original image once at the first ordered marker occurrence and inserts generated label text such as `Image A:` immediately before that image record. Later marker occurrences for the same image serialize as generated text references such as `[Image A]`.
- Existing-thread image paste is unavailable until Beryl knows prior image labels well enough to allocate without colliding with older history.
- Composer image marker atoms are indivisible draft positions for caret movement, selection, deletion, cut, paste, undo, and redo.
- Removing one marker occurrence removes only that reference. The payload is dropped from the mutable draft after the final active occurrence is removed, unless it already belongs to an accepted or queued fragment.
- Primary activation of an image marker opens a context menu with `View` and `Remove`.
- `View` opens a fitted Beryl image preview popup over original image bytes without submitting the draft or opening an external viewer.
- `Remove` deletes the selected marker occurrence through the same editing path as keyboard deletion.
- Pasted images that exceed deterministic count or retained-byte budgets are rejected before the draft mutates.

## Image Clipboard Semantics

- Copying or cutting a selection containing image markers writes explanatory fallback text such as `[Image A]` to the system clipboard.
- Beryl may also write private clipboard metadata with an opaque token for restoring marker atoms while the transient payload is still live.
- Original image bytes must not be serialized into clipboard metadata.
- Pasting copied Beryl marker metadata in the same label scope creates another marker reference to the same image and keeps the same label.
- Cutting and then pasting a marker is the user-visible way to move that image reference inside the draft.
- Pasting copied marker metadata into another conversation or pending-new-thread scope allocates fresh labels from the target scope, subject to prior-label readiness.
- Clipboard text that merely looks like `[Image A]` without valid Beryl metadata always pastes as ordinary text.

## Submission And Queuing

- When a non-empty draft is accepted for submission, it clears immediately and appears in the transcript as one distinct user input fragment.
- Rejected submissions, including empty drafts, backend-unavailable targets, storage failures, path-preparation failures, or serialization failures, keep the draft intact and report the rejection.
- Image path preparation validates the durable GUI-owned image asset at the path readable by the GUI process while separately preparing the runtime-readable backend `localImage.path` used by app-server.
- Each accepted composer send-and-clear event remains a distinct user input fragment, even when multiple fragments belong to one backend turn.
- If no backend thread is active and the user submits from the workspace screen, Beryl creates a new persistent Codex thread through the current primary member and activates it.
- Starting a new thread requires a default runtime, a resolved primary member, and a backend-available runtime target.
- Accepted fragments for an ordinary active parent turn are delivered through app-server active-turn steering when Beryl knows the expected active turn id.
- If input is accepted before Beryl knows the active turn id, it is held in a short pending queue and flushed once the id is known.
- If active-turn steering is rejected because the turn is not steerable or the expected id no longer matches, the fragment remains queued for the next eligible turn.
- During selected-thread context compaction, accepted fragments are rendered immediately and queued for the next backend turn after compaction completes. Beryl must not try to steer a compaction operation.
- Multiple queued fragments preserve accepted order and remain separate visible user blocks.
- User input fragments accepted before or during a stop request remain visible and ordered. If they cannot be delivered to the interrupted turn, they remain queued for the next eligible turn.
- Accepted fragments are delivered through app-server turn primitives or preserved as pending GUI-held input with explicit failure presentation. Beryl must not mutate backend history locally to pretend delivery succeeded.

## Composer History

- Composer history is GUI-local in-memory session state scoped to the selected backend thread or pending-new-thread draft.
- It is not persisted, is not seeded from backend transcript history, and does not trigger backend reads, submissions, or transcript mutations.
- When a pending-new-thread draft creates a backend thread, that draft's history scope follows the newly created thread.
- Only accepted non-empty submissions enter history. Rejected submissions and whitespace-only text submissions do not.
- Consecutive duplicate accepted drafts collapse to one entry.
- History lists are bounded per conversation scope. When the bound is exceeded, the oldest entries in that scope are evicted.
- Browsing history replaces the visible draft with an editable copy of the selected entry, including restorable image atoms, remeasures the input panel, clears selection, and places the caret at the end.
- The draft that existed when browsing began is captured exactly and restored when browsing forward past the newest history entry.
- Editing a recalled entry changes only the current draft and not the stored history entry.
- When thread-edit mode is active or no history exists in the requested direction, `Alt+Up` and `Alt+Down` leave draft, caret, selection, panel size, and transcript viewport unchanged.

## External Draft Insertion

- Beryl remembers the latest draft insertion point so transcript quote actions can insert text even while the transcript has focus.
- Transcript quote insertion updates the draft buffer, saved insertion position, and undo history through the same shared editing semantics as ordinary edits.
- Quote insertion must not change the system clipboard and does not force keyboard focus into the composer.

## Developer Instructions On User Turns

- Non-empty global developer-instructions preferences are sent as hidden developer-instructions context when Beryl starts a top-level user-message turn.
- This includes first turns of new persistent user-facing threads, later turns in existing threads, and automatic continuation turns after lifecycle yield requests.
- Blank or whitespace-only developer-instructions settings are disabled. If the app-server mechanism is stateful and no other feature-owned hidden developer-instructions section is active, Beryl may send hidden reset metadata.
- Developer-instructions lookup is late-bound at request assembly, so retries and replacement starts use the latest applied setting.
- Developer instructions must not become transcript-visible user messages, queued input fragments, semantic graph state, or backend-owned Codex configuration.
- Developer instructions are not sent to subagent requests, active-turn steering, title-generation maintenance, inventory refreshes, lazy metadata reads, context-compaction requests themselves, or other background/status-only work.
- If the app-server mechanism requires an effective model and Beryl cannot determine it from exact backend metadata or GUI-held pending defaults, Beryl omits hidden developer-instructions request data rather than guessing.
