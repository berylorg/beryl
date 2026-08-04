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
- Risk-based working-set limits, range-backed editing, streaming clipboard/export, and bounded
  dependency behavior are defined in `doc/systems/bounded-resource-dataflow/design.md`.
- Backend runtime availability and protocol capability boundaries are defined in `doc/systems/backend-runtime/design.md`.
- Beryl image-asset product behavior is defined in `doc/features/image-assets/design.md`; durable byte and runtime-path mechanics are defined in `doc/systems/image-assets/design.md`.

## Composer Panel

- The user input panel is pinned above the global status line and below any branch-discussion status strip inside the conversation column.
- Every selected Syndic thread uses the same composer layout over its current durable draft.
- The panel automatically grows and shrinks with wrapped draft line count up to half the OS window height, further clamped to preserve the transcript region minimum height.
- The panel is not manually resizable and does not expose a persistent `Run Turn` or submit button.
- The input field wraps text at its visible width and does not horizontally scroll. When wrapped content exceeds the panel's maximum height, the field owns vertical scrolling and keeps the caret or active selection endpoint reachable.
- Backend-unavailable or otherwise submission-disabled states render the composer disabled for submission while preserving draft text, image markers, caret, selection, and undo history.
- Ordinary CAS warm-up and backend unavailability keep the editor available for drafting. The
  composer is replaced only by the explicit native-lineage recovery decision defined by the
  backend-runtime-recovery feature when execution needs that decision; the hidden editor state and
  durable draft remain intact.
- A draft containing at least one image marker and no non-whitespace text is non-empty for submission.

## Durable Current Draft

- Every Syndic thread owns exactly one current durable draft; the composer edits it through a
  range-backed projection rather than owning another whole-draft payload.
- Draft metadata references one exact sealed content manifest whose ordered content is stored in bounded chunks. Internal record and chunk ceilings do not impose a whole-draft size limit; pasted text sized for million-token or larger model contexts remains durable.
- The current draft is not transcript narrative and is never sent to CAS before submission.
- An ordinary current draft is only the user's durable unsent composer state. It does not own a
  selected-path parent and is not rebased, rotated, cleared, or revision-bumped when previously
  accepted input advances the thread. The thread owns the committed tail.
- A branch-discussion first draft owns its immutable selected-context envelope, and a
  replacement-edit draft owns an explicit replacement target. Those typed facts are not an
  ordinary draft-parent field.
- Composer edits update the visible range and bounded edit frontier immediately, stage the exact
  range operation against the durable content identity, and mark the exact draft revision dirty.
  Visible text, bounded overscan, active IME composition, compact selection/caret positions,
  marker identities, and the bounded undo frontier are the only content-dependent resident editor
  state. Draft size never requires a whole `String`, one descriptor per chunk, or a second payload
  for autosave, submission, recovery, history browsing, or replacement editing.
- Dirty-only autosave runs every 30 seconds by default. Settings may tune the required interval from 5 through 300 whole seconds but may not disable autosave.
- Publishing a committed autosave-interval change rearms the next dirty-draft deadline from that publication time using the new interval; it does not preserve a deadline derived from the superseded setting.
- A save or same-home reconciliation that started under an older timer generation cannot replace the interval or deadline anchor of a newer committed setting publication.
- Thread switch, ordinary window close, application Exit, and submission create dirty-draft flush barriers instead of waiting for the timer.
- Autosave constructs changed content through bounded staged chunk commits and publishes the new sealed manifest reference with the draft revision in one final atomic command. A crash may leave unreachable partial content, but the current draft remains wholly old or wholly new.
- A flush drains an already-admitted save instead of cancelling it. If that save has an ambiguous post-admission storage outcome, the barrier remains unsatisfied until same-home verification or recovery rereads and reconciles the exact draft identity, revision, and payload.
- Draft content survives restart, thread switching, CAS failure and retry, ordinary window close/reopen, and application Exit.
- Native-lineage recovery-prompt entry does not clear, submit, rewrite, or discard the current
  draft. Leaving the prompt after recovery restores the exact unadmitted editor state, while an
  already admitted pending input remains owned by its durable Syndic identity.
- During draft-save failure or reconciliation, the coherent resident editor ranges, caret,
  selection, and bounded undo frontier remain intact. Nonresident content stays durable and
  range-addressable. No save success, thread switch, close, Exit, or submission is published from
  an ambiguous durable outcome.
- A replacement-edit draft carries a durable typed target separate from its mutable payload. Restart or thread reactivation restores edit mode against that exact target instead of silently treating the text as an ordinary append.
- Cancelling replacement edit clears only the typed edit target and keeps the mutable draft payload and editor state.
- A persistent Beryl-home store failure disables editing because Beryl cannot durably own further draft changes.
- Branch discussions with a live admitted resolution in pending, handing-off, retryably failed, or recovery state disable editing, as do archived discussions. A terminally failed handoff releases its composer gate and leaves the unarchived discussion editable according to `doc/features/branch-discussions/design.md`.

## Draft Loading And Activation

- Every restored window's selected current draft identity, visible caret range, markers intersecting
  that range, and bounded editor frontier are loaded and validated during the invisible pre-window
  bootstrap so its first visible composer is immediately editable. Bootstrap never loads the
  complete draft merely to make the editor ready.
- A later thread activation keeps the prior selected thread and its draft authoritative until the
  target transcript seed, target draft identity, and first required editor range are ready.
- Edits made before activation commits continue to belong to the prior thread and are flushed before the atomic switch.
- Successful activation publishes target title, lineage, transcript seed, current draft, and composer history scope together.
- Failed or cancelled activation leaves the prior draft, caret, selection, undo history, and dirty state intact.
- Beryl never merges early text with an unseen target draft and never replaces a target draft with an empty window-local buffer.

## Shared Text Input Integration

- Baseline text editing, selection, clipboard, undo/redo, IME, and pointer behavior comes from the shared text-input contract in `doc/input-hotkeys.md`.
- The composer requires the range-backed multiline variant of the shared text-input. Caret movement,
  selection, scrolling, wrapping, and editing that reach a nonresident range request bounded pages
  and preserve the last coherent editor surface while they load; they never flatten the draft.
- Pasting a large supported text or rich-atom clipboard representation streams into one staged
  draft edit. While that operation is visibly paste-pending, the composer preserves its coherent
  text, caret, and selection; draft mutation and submission are temporarily unavailable so later
  typing cannot race the captured insertion range. `Escape` may cancel before final edit admission;
  an admitted edit drains to an exact outcome.
- Successful paste publishes the complete edit once, places the caret after the inserted range, and
  records one logical undo operation without retaining a second copy of the inserted bytes. Failure
  or pre-admission cancellation restores the prior writable state unchanged. An unsupported or
  non-preflightable oversized clipboard representation is rejected before draft mutation; no
  partial paste becomes visible or durable.
- Selecting an arbitrarily large range remains a compact logical range operation. Copy and cut use
  the platform clipboard only when the selected logical representation fits the explicitly
  configured contiguous clipboard limit. Otherwise Beryl reports that the selection is too large for
  the clipboard and leaves the draft unchanged; cut never deletes content whose copy did not
  complete. A separately defined streaming save or export command may provide an alternative
  without weakening this limit.
- Undo and redo retain a bounded recent operation frontier. When an older edit is no longer resident
  in that frontier, the command is unavailable rather than keeping draft-sized inverse text in
  memory. Autosave and durable content revisions remain independent of that GUI history bound.
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
- The reserved label floor is the owning Syndic thread's exact durable image-label frontier. First
  acceptance of a marker advances that frontier and publishes at most one immutable origin span
  naming the sealed asset-set proof in the same home command; steering, queue, retry,
  delivery-unknown, and submitted states do not maintain separate label sets.
- Active draft image markers above that frontier reserve their labels only while at least one marker
  occurrence remains in the draft. Their bounded marker index, not a scan of draft text, supplies
  allocation conflicts.
- Labels remain stable while a draft marker, accepted fragment, queued fragment, or retry state exists. Removing the final active marker for a draft-only image releases that label when the image has not been accepted, queued, retried, or observed in the owning conversation-history boundary.
- Removing one active draft label does not rename later active draft labels. A later paste may fill a
  released draft-only gap strictly above the durable frontier when the bounded current-draft marker
  index proves it free.
- Every label at or below the durable frontier remains permanently reserved even when the owning
  origin span's asset set has no label-first entry for a historical gap. Beryl never reconstructs a
  complete used-label set merely to fill such a gap.
- Multiple markers may show the same label only when they reference the same pasted image payload.
- The compact visual marker such as `[A]` is presentation. It is not submitted as literal user-authored text.
- On submission, Beryl sends the original image once at the first ordered marker occurrence and inserts generated label text such as `Image A:` immediately before that image record. Later marker occurrences for the same image serialize as generated text references such as `[Image A]`.
- Existing-thread image paste is unavailable until Beryl has point-read the exact current label
  frontier and bounded current-draft marker index. A fixed-capacity GUI cache may retain only that
  compact frontier/revision proof; eviction repeats the point reads rather than scanning history.
- Captured transcript histories are owned by Syndic, while CAS remains the live execution and policy owner. Other CAS clients can append to or mutate a thread, and Beryl must treat thread-label caches keyed only by thread id as stale until an owning-history frontier check proves they are still safe.
- Composer image-label authority for the selected transcript comes only from Syndic-captured history and must not be populated by querying CAS historical transcript APIs. Missing or incomplete Syndic history keeps image paste unavailable or produces an explicit incomplete-history state.
- When a conversation thread is loaded or selected, Beryl should proactively point-read and validate
  its compact image-label frontier so image paste is usually ready before the user invokes it.
- Image-label readiness gates only operations that create or move image markers into a label scope. Ordinary text input remains available while label synchronization is pending.
- Beryl does not insert unresolved image-marker placeholders into the composer. An image marker must have its final label before it enters the draft, because unlabeled atoms would make submission, undo, copy/cut, edit mode, and scan-failure behavior correctness-critical.
- Composer image marker atoms are indivisible draft positions for caret movement, selection, deletion, cut, paste, undo, and redo.
- Removing one marker occurrence removes only that reference. The payload is dropped from the mutable draft after the final active occurrence is removed, unless it already belongs to an accepted or queued fragment.
- Primary activation of an image marker opens a context menu with `View` and `Remove`.
- `View` opens a fitted Beryl image preview popup over original image bytes without submitting the draft or opening an external viewer.
- `Remove` deletes the selected marker occurrence through the same editing path as keyboard deletion.
- Image-marker count does not impose a whole-draft memory-safety limit. Paste rejects before draft
  mutation only for a typed clipboard/format failure, exact product/provider constraint, durable
  asset-admission failure, or representable-domain exhaustion; preview residency pressure uses the
  bounded unavailable/fallback state instead of discarding admitted bytes.

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
- Rejected submissions, including empty drafts, backend-unavailable targets, pre-admission storage rejection, path-preparation failure, or serialization failure, keep the draft intact and report the rejection. A post-admission storage failure keeps the visible editor intact but is reconciled against recovered durable state before Beryl classifies the submission as rejected or accepted.
- Acceptance requires one durable Syndic admission that freezes the exact draft payload into a submitted turn, active-turn input, or queued input and atomically establishes the replacement current draft before the composer clears, transcript projection mutates, delivery state changes, or image-label protected state advances.
- Ordinary idle admission selects the then-current committed thread tail as the new turn's parent
  inside that same atomic command. It never relies on historical parent metadata in the draft.
  Concurrent tail, gate, thread, or draft change rejects the whole admission and leaves the
  composer content intact for exact reconciliation or retry.
- Admission reuses the sealed draft content reference and compact shared sealed asset-set proof; the
  paged set is the separate marker-resolution authority, so no arbitrarily large text or marker
  vector is copied into submitted-turn, accepted-input, or canonical-item metadata.
- For image-bearing input, bounded staging resolves every stable marker and final label to its exact
  durable asset id and seals one paged asset-reference set. The final admission atomically rebinds
  only compact owner heads to the submitted item or accepted-input owner. Missing, stale,
  duplicated, or disagreeing entries reject before admission and leave the draft intact.
- When the current draft carries a replacement-edit target, acceptance uses the replacement-edit mutation defined by the conversation-thread feature instead of ordinary append, steering, or queueing.
- If the current Syndic path has a valid CAS projection binding, an accepted new user turn is delivered through ordinary CAS turn start behavior.
- If the current Syndic path has a stale or unbound CAS projection binding, accepted execution must first obtain a fresh CAS projection through the CAS-live Syndic system boundary.
- Once CAS accepts a live turn, the accepted user input for that turn is no longer editable on the Syndic side. The user may stop it, branch after completion, or create later replacement work according to the conversation-thread contract.
- Stopping an active live turn preserves the submitted turn and captured response with explicit interrupted, incomplete, failed, or terminal state.
- Image path preparation validates the durable GUI-owned image asset at the path readable by the GUI process while separately preparing the runtime-readable backend `localImage.path` used by app-server.
- Each accepted composer send-and-clear event remains a distinct user input fragment, even when multiple fragments belong to one backend turn.
- Submitting the first draft of a Syndic thread requires that thread's exact runtime/root binding and a compatible available CAS runtime; it does not create the Syndic thread at submission time.
- Accepted fragments for an ordinary active parent turn are delivered through app-server active-turn
  steering only when Beryl knows the exact expected active turn id.
- If input is accepted before Beryl knows that exact id, the same accepted fragment is held on its
  short pending route and flushed once the id is known. It does not acquire a second queue or
  submission identity.
- Temporary local delivery congestion does not convert a fragment accepted for a still-steerable
  active turn into a later ordinary turn. The fragment remains pending for steering unless exact
  target evidence later makes steering ineligible.
- If the exact active turn becomes terminally uncertain while Beryl can still receive late evidence
  about that turn, fragments accepted during that interval are rendered immediately and remain
  ordered for the next backend turn. Later evidence that the uncertain turn was still active does
  not retroactively convert those fragments into steering.
- If active-turn steering is rejected because the turn is not steerable or the expected id no
  longer matches, the same fragment remains in accepted order for the next eligible turn. It is
  not cleared and accepted again, merged with another fragment, or presented as delivered.
- During selected-thread terminal-history finalization or context compaction, accepted fragments are
  rendered immediately and queued for the next backend turn after the owning operation completes.
  Beryl must not try to steer either operation.
- Multiple queued fragments preserve accepted order and remain separate visible user blocks.
- When the gate later becomes eligible, Syndic promotes only the earliest effective queued
  fragment into one fresh pending ordinary turn and canonical user-input item. The promotion
  retains the accepted fragment as permanent history with an exact successor link, transfers its
  compact asset owner to that submitted item, and leaves the current draft, editor state, and
  draft asset owner untouched.
- Pending-route admission is part of submission acceptance. Durable-store failure, checked-domain
  exhaustion, or an exact operation constraint must reject before composer clear, transcript
  projection, backend delivery state, or image-label protected state mutates. Logical backlog size
  is not a process-memory budget: delivery reads durable accepted inputs through bounded pages and
  fixed worker slots.
- User input fragments accepted before or during a stop request remain visible and ordered. Work
  proven not dispatched remains queued for the next eligible turn. A possibly dispatched fragment
  whose response was lost becomes delivery-unknown durable history and is not presented as
  next-turn queued work or replayed.
- Accepted fragments are delivered through app-server turn primitives or preserved in durable
  pending, terminal failure, or delivery-unknown state with explicit presentation. Beryl must not
  mutate backend history locally to pretend delivery succeeded.

## Composer History

- Composer history is GUI-local session state represented by compact references to exact sealed
  Syndic input manifests. It never stores another draft byte buffer, image payload, or atom list.
- It is not persisted, is not seeded from backend transcript history, and does not trigger backend reads, submissions, or transcript mutations.
- Only accepted non-empty submissions enter history. Rejected submissions and whitespace-only text submissions do not.
- Consecutive duplicate accepted drafts collapse to one entry.
- One process-global fixed-capacity history-reference pool bounds both retained thread scopes and
  entries. Per-thread fairness prevents one scope from consuming the pool; overflow evicts the
  oldest eligible references without touching their durable submitted inputs.
- Browsing history replaces the current draft manifest with a copy-on-write range-backed view of
  the selected sealed input, including referenced image atoms, remeasures only resident editor
  ranges, clears selection, and places the caret at the logical end.
- The draft that existed when browsing began is captured exactly by its manifest identity and
  revision and restored by reference when browsing forward past the newest history entry.
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
