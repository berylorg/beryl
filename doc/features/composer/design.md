# Goals

Provide one reliable durable per-thread composer for ordinary input, active-turn steering, compaction-time queueing, image input, history browsing, and transcript quote insertion without confusing current drafts with submitted transcript history or GUI-owned metadata.

## Non-goals

- Rendering image thumbnails inline inside the draft.
- Persisting composer history across app restarts.
- Allowing draft autosave to be disabled while durable drafts are part of the thread contract.
- Treating manually typed text such as `[Image A]` as an image attachment.
- Clearing drafts on rejected submission.

# Decisions

## Implementation References

- `gui.md` is a normative supplemental GUI composition file for the composer panel, draft editor, image marker menu, and image preview popup.
- CAS-live Syndic submission admission, capture durability, and incomplete-history behavior are defined in `doc/systems/cas-live-syndic-transcript/design.md`.
- Syndic turn ownership, transcript-view provenance, and durable image-marker evidence are defined in `doc/systems/syndic-conversation-history/design.md`.
- Backend runtime availability and protocol capability boundaries are defined in `doc/systems/backend-runtime/design.md`.
- Beryl image-asset product behavior is defined in `doc/features/image-assets/design.md`; durable byte and runtime-path mechanics are defined in `doc/systems/image-assets/design.md`.

## Composer Panel

- The user input panel is pinned above the global status line and below any branch-discussion status strip inside the conversation column.
- Every selected Syndic thread uses the same composer layout over its current durable draft.
- The panel automatically grows and shrinks with wrapped draft line count up to half the OS window height, further clamped to preserve the transcript region minimum height.
- The panel is not manually resizable and does not expose a persistent `Run Turn` or submit button.
- The input field wraps text at its visible width and does not horizontally scroll. When wrapped content exceeds the panel's maximum height, the field owns vertical scrolling and keeps the caret or active selection endpoint reachable.
- Backend-unavailable or otherwise submission-disabled states render the composer disabled for submission while preserving draft text, image markers, caret, selection, and undo history.
- A draft containing at least one image marker and no non-whitespace text is non-empty for submission.

## Durable Current Draft

- Every Syndic thread owns exactly one current durable draft; the composer edits that draft's mutable payload.
- The current draft is not transcript narrative and is never sent to CAS before submission.
- Composer edits update in-memory state immediately and mark the exact draft revision dirty.
- Dirty-only autosave runs every 30 seconds by default. Settings may tune the required interval from 5 through 300 whole seconds but may not disable autosave.
- Thread switch, ordinary window close, application Exit, and submission create dirty-draft flush barriers instead of waiting for the timer.
- Draft content survives restart, thread switching, CAS failure and retry, ordinary window close/reopen, and application Exit.
- A replacement-edit draft carries a durable typed target separate from its mutable payload. Restart or thread reactivation restores edit mode against that exact target instead of silently treating the text as an ordinary append.
- Cancelling replacement edit clears only the typed edit target and keeps the mutable draft payload and editor state.
- A persistent Beryl-home store failure disables editing because Beryl cannot durably own further draft changes.
- Branch discussions with a live admitted resolution in pending, handing-off, retryably failed, or recovery state disable editing, as do archived discussions. A terminally failed handoff releases its composer gate and leaves the unarchived discussion editable according to `doc/features/branch-discussions/design.md`.

## Draft Loading And Activation

- Every restored window's selected current draft is loaded and validated during the invisible pre-window bootstrap so its first visible composer is immediately editable.
- A later thread activation keeps the prior selected thread and its draft authoritative until the target transcript seed and target current draft are both ready.
- Edits made before activation commits continue to belong to the prior thread and are flushed before the atomic switch.
- Successful activation publishes target title, lineage, transcript seed, current draft, and composer history scope together.
- Failed or cancelled activation leaves the prior draft, caret, selection, undo history, and dirty state intact.
- Beryl never merges early text with an unseen target draft and never replaces a target draft with an empty window-local buffer.

## Shared Text Input Integration

- Baseline text editing, selection, clipboard, undo/redo, IME, and pointer behavior comes from the shared text-input contract in `doc/input-hotkeys.md`.
- The composer reserves focused `Enter` for submission and focused `Shift+Enter` for inserting a real newline.
- When thread-edit mode is active, focused `Enter` attempts edit commit instead of ordinary turn start, active-turn steering, or compaction-time queueing.
- When thread-edit mode is active and no higher-priority popup handles `Escape`, focused `Escape` cancels edit mode without mutating the draft buffer, image markers, caret, selection, or undo history.
- While thread-edit mode is inactive, focused `Alt+Up` and `Alt+Down` browse accepted composer history for the current conversation scope.
- Focused `Ctrl+Up` and `Ctrl+Down` scroll the transcript between turn boundaries without moving the draft caret or changing selection.
- `Ctrl+Up` first moves to the current turn's semantic start before earlier jumps move to previous turns. `Ctrl+Down` scrolls to the transcript bottom when no later turn boundary exists.
- Focused `Ctrl+Up` and `Ctrl+Down` are turn-to-turn navigation commands. They do not step between streamed chunks inside one huge turn; when the target turn is huge, the transcript feature anchors that turn and lazily streams chunks according to its own navigation contract.

## Image Markers And Assets

- Pasting image clipboard content into the composer inserts an inline image marker at the caret or replaces the selected draft range.
- Pasted images are stored in the Beryl-home-wide durable image-asset store for backend submission, composer preview, accepted transcript markers, retries, and preview after restart.
- Image labels are correctness-critical model-visible data, because CAS image input records do not carry Beryl's GUI-owned label and Beryl sends labels as adjacent text.
- Image labels are allocated from the selected Syndic thread's reserved-label state as `A`, `B`, `C`, continuing spreadsheet-style when needed.
- The reserved label floor is seeded from the owning conversation-history boundary plus accepted fragments, queued fragments, and retry state. Active draft image markers reserve their labels only while at least one marker occurrence remains in the draft.
- Labels remain stable while a draft marker, accepted fragment, queued fragment, or retry state exists. Removing the final active marker for a draft-only image releases that label when the image has not been accepted, queued, retried, or observed in the owning conversation-history boundary.
- Removing one active draft label does not rename later active draft labels. A later paste may fill the released gap only when no active marker, accepted fragment, queued fragment, retry state, or reliable owning-history evidence still reserves that label.
- Beryl must never allocate a label that overlaps a same-thread image label it has reliable evidence may have existed. When current history is uncertain, sparse or gapped labels are preferable to reuse; Beryl should only avoid gaps when exact validated owning-history data makes doing so safe.
- Multiple markers may show the same label only when they reference the same pasted image payload.
- The compact visual marker such as `[A]` is presentation. It is not submitted as literal user-authored text.
- On submission, Beryl sends the original image once at the first ordered marker occurrence and inserts generated label text such as `Image A:` immediately before that image record. Later marker occurrences for the same image serialize as generated text references such as `[Image A]`.
- Existing-thread image paste is unavailable until Beryl knows prior image labels well enough to allocate without colliding with older history. GUI-local caches may accelerate this check, but they are not authoritative unless validated against the current owning conversation-history boundary.
- Captured transcript histories are owned by Syndic, while CAS remains the live execution and policy owner. Other CAS clients can append to or mutate a thread, and Beryl must treat thread-label caches keyed only by thread id as stale until an owning-history frontier check proves they are still safe.
- Composer image-label authority for the selected transcript comes only from Syndic-captured history and must not be populated by querying CAS historical transcript APIs. Missing or incomplete Syndic history keeps image paste unavailable or produces an explicit incomplete-history state.
- When a conversation thread is loaded or selected, Beryl should proactively validate or synchronize that thread's image-label cache in the background so image paste is usually ready before the user invokes it.
- Image-label readiness gates only operations that create or move image markers into a label scope. Ordinary text input remains available while label synchronization is pending.
- Beryl does not insert unresolved image-marker placeholders into the composer. An image marker must have its final label before it enters the draft, because unlabeled atoms would make submission, undo, copy/cut, edit mode, and scan-failure behavior correctness-critical.
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
- Pasting copied Beryl marker metadata in the same label scope creates another marker reference to the same image and keeps the same label. Durable Beryl image asset id equality is sufficient to identify the same image even when resident history has released its bytes. If that label has since been released and reused by a different draft-only image payload or different durable asset id, the stale private marker payload must be rejected before inserting an atom.
- Cutting and then pasting a marker is the user-visible way to move that image reference inside the draft.
- Pasting copied marker metadata into another conversation scope allocates fresh labels from the target thread, subject to prior-label readiness.
- Clipboard text that merely looks like `[Image A]` without valid Beryl metadata always pastes as ordinary text.

## Submission And Queuing

- When a non-empty draft is accepted for submission, it clears immediately and appears in the transcript as one distinct user input fragment.
- Rejected submissions, including empty drafts, backend-unavailable targets, storage failures, path-preparation failures, or serialization failures, keep the draft intact and report the rejection.
- Acceptance requires one durable Syndic admission that freezes the exact draft payload into a submitted turn, active-turn input, or queued input and atomically establishes the replacement current draft before the composer clears, transcript projection mutates, delivery state changes, or image-label protected state advances.
- When the current draft carries a replacement-edit target, acceptance uses the replacement-edit mutation defined by the conversation-thread feature instead of ordinary append, steering, or queueing.
- If the current Syndic path has a valid CAS projection binding, an accepted new user turn is delivered through ordinary CAS turn start behavior.
- If the current Syndic path has a stale or unbound CAS projection binding, accepted execution must first obtain a fresh CAS projection through the CAS-live Syndic system boundary.
- Once CAS accepts a live turn, the accepted user input for that turn is no longer editable on the Syndic side. The user may stop it, branch after completion, or create later replacement work according to the conversation-thread contract.
- Stopping an active live turn preserves the submitted turn and captured response with explicit interrupted, incomplete, failed, or terminal state.
- Image path preparation validates the durable GUI-owned image asset at the path readable by the GUI process while separately preparing the runtime-readable backend `localImage.path` used by app-server.
- Each accepted composer send-and-clear event remains a distinct user input fragment, even when multiple fragments belong to one backend turn.
- Submitting the first draft of a Syndic thread requires that thread's exact runtime/root binding and a compatible available CAS runtime; it does not create the Syndic thread at submission time.
- Accepted fragments for an ordinary active parent turn are delivered through app-server active-turn steering when Beryl knows the expected active turn id.
- If input is accepted before Beryl knows the active turn id, it is held in a short pending queue and flushed once the id is known.
- If active-turn steering is rejected because the turn is not steerable or the expected id no longer matches, the fragment remains queued for the next eligible turn.
- During selected-thread context compaction, accepted fragments are rendered immediately and queued for the next backend turn after compaction completes. Beryl must not try to steer a compaction operation.
- Multiple queued fragments preserve accepted order and remain separate visible user blocks.
- Pending queue admission is part of submission acceptance. Over-budget pending fragments must be rejected before composer clear, transcript projection, backend delivery queue state, or image-label protected state mutates.
- User input fragments accepted before or during a stop request remain visible and ordered. If they cannot be delivered to the interrupted turn, they remain queued for the next eligible turn.
- Accepted fragments are delivered through app-server turn primitives or preserved as pending GUI-held input with explicit failure presentation. Beryl must not mutate backend history locally to pretend delivery succeeded.

## Composer History

- Composer history is GUI-local in-memory session state scoped to the selected Syndic thread.
- It is not persisted, is not seeded from backend transcript history, and does not trigger backend reads, submissions, or transcript mutations.
- Only accepted non-empty submissions enter history. Rejected submissions and whitespace-only text submissions do not.
- Consecutive duplicate accepted drafts collapse to one entry.
- History lists are bounded per conversation scope. When the bound is exceeded, the oldest entries in that scope are evicted.
- Browsing history replaces the visible draft with an editable copy of the selected entry, including restorable image atoms, remeasures the input panel, clears selection, and places the caret at the end.
- The draft that existed when browsing began is captured exactly and restored when browsing forward past the newest history entry.
- Editing a recalled entry changes only the current draft and not the stored history entry.
- When thread-edit mode is active or no history exists in the requested direction, `Alt+Up` and `Alt+Down` leave draft, caret, selection, panel size, and transcript viewport unchanged.

## External Draft Insertion

- Beryl remembers the latest draft insertion point so transcript quote actions can insert text even while the transcript has focus.
- Transcript quote actions provide composer-owned insertion requests through the transcript feature's quote payload contract. A quote payload is valid only when the transcript feature derived it from rendered resident transcript records with stable provenance and geometry.
- Transcript quote insertion updates the draft buffer, saved insertion position, and undo history through the same shared editing semantics as ordinary edits.
- Quote insertion must not change the system clipboard and does not force keyboard focus into the composer.
- Quote insertion is unavailable when the transcript feature cannot provide a current quote payload. The composer must not synthesize quote text from backend history, legacy transcript caches, stale projections, or nonresident transcript ranges.

## Developer Instructions On User Turns

- Non-empty global developer-instructions preferences are sent as hidden developer-instructions context when Beryl starts a top-level user-message turn.
- This includes first turns of new persistent user-facing threads, later turns in existing threads, and automatic continuation turns after lifecycle yield requests.
- Blank or whitespace-only developer-instructions settings are disabled. If the app-server mechanism is stateful and no other feature-owned hidden developer-instructions section is active, Beryl may send hidden reset metadata.
- Developer-instructions lookup is late-bound at request assembly, so retries and replacement starts use the latest applied setting.
- Developer instructions must not become transcript-visible user messages, current drafts, queued input fragments, Syndic history, or backend-owned Codex configuration.
- Developer instructions are not sent to subagent requests, active-turn steering, title-generation maintenance, inventory refreshes, lazy metadata reads, context-compaction requests themselves, or other background/status-only work.
- If the app-server mechanism requires an effective model and Beryl cannot determine it from exact backend metadata or GUI-held pending defaults, Beryl omits hidden developer-instructions request data rather than guessing.
