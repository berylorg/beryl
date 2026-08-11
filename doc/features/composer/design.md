# Goals

Provide one reliable durable per-thread composer for ordinary input, active-turn steering,
compaction-time queueing, image input, history browsing, and transcript quote insertion without
confusing current drafts with submitted transcript history or image identity.

## Non-goals

- Rendering image thumbnails inline inside the draft.
- Persisting composer history across app restarts.
- Allowing draft autosave to be disabled while durable drafts are part of the thread contract.
- Treating manually typed text such as `[Image A]` as an image attachment.
- Clearing drafts on rejected submission.

# Decisions

## Implementation References

- [`gui.md`](gui.md) is a normative supplemental GUI composition file for the composer panel, draft editor, image marker menu, and image preview popup.
- Submission and conversation-history architecture are defined in
  `doc/systems/cas-live-syndic-transcript/design.md` and
  `doc/systems/syndic-conversation-history/design.md`, with the persistence boundary defined in
  `crates/syndic-storage/doc/design.md`.
- Durable home mutation and reconciliation are defined in
  `doc/systems/beryl-home-storage/design.md`.
- Large-input resource behavior is defined in
  `doc/systems/bounded-resource-dataflow/design.md`.
- Runtime readiness and backend request boundaries are defined in
  `doc/systems/backend-runtime/design.md` and `crates/beryl-backend/doc/design.md`.
- Image behavior is defined in `doc/features/image-assets/design.md` and
  `doc/systems/image-assets/design.md`, with its package boundary in
  `crates/beryl-state/doc/design.md`.
- Composer workflow coordination is defined by `crates/beryl-app/doc/design.md`.

## Durable Mutation Reconciliation

- An indeterminate composer mutation keeps its initiating surface visibly reconciling, preserves
  the last coherent editor presentation plus the exact locally held request intent and evidence,
  and suppresses duplicate activation and dependent mutations.
- Reconciliation may prove exact success, prove noncommit, or return `Collision`, presented as
  terminal `Unavailable`, because it can prove neither. Only proven success publishes the new
  composer state. Only proven noncommit restores the prior state as authoritative. `Unavailable`
  does neither and retains the prior presentation only as coherent context, not as proof that the
  request did not commit.
- A terminally unavailable request and every composer action that depends on its result remain
  unavailable with a persistent bounded explanation. Only established same-home recovery and
  bounded diagnostic reporting may address or explain it; Beryl exposes no save, paste, submit, or
  replacement retry, resubmission, rollback, or manual repair command for that exact request.
  Unrelated healthy threads and composer work remain available.

## Composer Panel

- Every selected conversation thread exposes one composer over its current durable draft. Its normative
  placement, layout, control composition, and editor geometry are defined in `gui.md` and the
  canonical widget specifications linked there.
- Backend-unavailable or otherwise submission-disabled states render the composer disabled for submission while preserving draft text, image markers, caret, selection, and undo history.
- Ordinary backend warm-up and unavailability keep the editor available for drafting. The
  composer is replaced only by the explicit native-lineage recovery decision defined by the
  backend-runtime-recovery feature when execution needs that decision. Replacement unmounts the
  editor; the durable draft and only its compact host-owned caret, selection, bounded undo/redo
  availability, and scroll restoration facts remain intact.
- A draft containing at least one image marker and no non-whitespace text is non-empty for submission.

## Durable Current Draft

- Every conversation thread owns exactly one current durable draft. The composer supports drafts sized for
  million-token or larger model contexts without imposing a smaller whole-draft product limit.
- Large drafts are intentional product state. Selecting, saving, submitting, or restoring one
  loads only the content needed for the visible editor range and requested operation; it does not
  require the complete draft to become resident first.
- The current draft is not transcript narrative and is never sent for execution before submission.
- An ordinary current draft is only the user's durable unsent composer state. It has no submitted
  parent and remains unchanged when previously accepted input advances the thread.
- A branch-discussion first draft retains its immutable selected context, and a replacement-edit
  draft retains its explicit replacement target. An ordinary draft retains neither.
- Composer edits update the visible range immediately and preserve exact authored content through
  autosave, submission, recovery, history browsing, and replacement editing without requiring a
  second whole-draft payload.
- Dirty-only autosave runs every 30 seconds by default. Settings may tune the required interval from 5 through 300 whole seconds but may not disable autosave.
- Publishing a committed autosave-interval change rearms the next dirty-draft deadline from that publication time using the new interval; it does not preserve a deadline derived from the superseded setting.
- Thread switch, ordinary window close, application Exit, and submission create dirty-draft flush barriers instead of waiting for the timer.
- Autosave publication is all-or-nothing: after a crash, the current draft is wholly the prior saved
  state or wholly the newly saved state, never a partial mixture.
- A flush lets a save already underway reach an exact outcome instead of cancelling it. If the
  durable outcome is ambiguous, the barrier remains unsatisfied while same-home verification or
  recovery classifies it as proven saved, proven not saved, or terminally unavailable. Terminal
  unavailability leaves the barrier unsatisfied and follows Durable Mutation Reconciliation rather
  than being treated as a failed save that can be repeated.
- Draft content survives restart, thread switching, backend failure and retry, ordinary window close/reopen, and application Exit.
- Native-lineage recovery-prompt entry does not clear, submit, rewrite, or discard the current
  draft. Leaving the prompt after recovery mounts an eligible replacement editor that restores the
  exact durable draft plus its captured caret, selection, bounded undo/redo availability, and scroll
  position through fresh bounded range requests. An already admitted pending input remains owned by
  its durable accepted-input identity.
- During draft-save failure or reconciliation, the coherent editor content, caret, selection, and
  available undo history remain intact. No save success, thread switch, close, Exit, or submission
  is published from an ambiguous or terminally unavailable durable outcome. A terminally
  unavailable save also preserves its exact local save intent and evidence without restoring the
  visible draft as proof of durable noncommit.
- A replacement-edit draft durably retains its target separately from editable content. Restart or
  thread reactivation restores edit mode against that exact target instead of silently treating the
  text as an ordinary append.
- Cancelling replacement edit clears only the edit target and keeps the draft content and editor state.
- A persistent draft-storage failure disables editing because further changes cannot be kept durable.
- Branch discussions with a resolution in pending, handing-off, retryably failed, recovery, or
  terminal `Unavailable` state disable editing, as do archived discussions. A terminal
  non-collision handoff failure releases its composer gate and leaves the unarchived discussion
  editable according to `doc/features/branch-discussions/design.md`.

## Draft Loading And Activation

- A restored window's first visible composer is immediately editable with the selected draft,
  caret, and visible markers. Preparing that presentation never requires the complete draft to
  become resident.
- A later thread activation keeps the prior selected thread and its draft authoritative until the
  target transcript and first required editor range are ready.
- Edits made before activation commits continue to belong to the prior thread and are flushed before the atomic switch.
- Successful activation publishes the target title, lineage, transcript, current draft, and
  composer-history scope together.
- Failed or cancelled activation leaves the prior draft, caret, selection, undo history, and dirty state intact.
- Beryl never merges early text with an unseen target draft and never replaces a target draft with an empty window-local buffer.

## Shared Text Input Integration

- Baseline text editing, selection, clipboard, undo/redo, IME, and pointer behavior comes from the shared text-input contract in `doc/input-hotkeys.md`.
- Caret movement, selection, scrolling, wrapping, and editing across a large draft preserve the last
  coherent editor surface while nonresident content loads; they never require the whole draft to
  flatten into one value.
- Pasting a large supported text or rich-atom clipboard representation is one draft edit. While it
  is visibly paste-pending, the composer preserves coherent text, caret, and selection; draft
  mutation and submission are temporarily unavailable so later typing cannot race the captured
  insertion point. `Escape` may cancel before the edit is accepted; an accepted edit remains
  pending through the exact success, proven-noncommit, or terminally unavailable dispositions in
  Durable Mutation Reconciliation.
- Successful paste publishes the complete edit once, places the caret after the inserted range, and
  records one logical undo operation without retaining a second copy of the inserted bytes.
  Ordinary failure, proven noncommit, or pre-admission cancellation restores the prior writable
  state unchanged. A terminally unavailable paste instead keeps the prior presentation, captured
  insertion intent, and local evidence, while duplicate paste and dependent draft mutation remain
  unavailable. An unsupported or non-preflightable oversized clipboard representation is rejected
  before draft mutation; no partial paste becomes visible or durable.
- Selecting an arbitrarily large range remains a compact logical range operation. Copy and cut use
  the platform clipboard only when the selected logical representation fits the explicitly
  configured contiguous clipboard limit. Otherwise Beryl reports that the selection is too large for
  the clipboard and leaves the draft unchanged; cut never deletes content whose copy did not
  complete. A separately defined streaming save or export command may provide an alternative
  without weakening this limit.
- Undo and redo cover a bounded recent operation history. When an older edit has left that history,
  the command is unavailable rather than retaining draft-sized inverse text. Autosave and durable
  draft state remain independent of that GUI history bound.
- The composer reserves focused `Enter` for submission and focused `Shift+Enter` for inserting a real newline.
- When thread-edit mode is active, focused `Enter` attempts edit commit instead of ordinary turn start, active-turn steering, or compaction-time queueing.
- When thread-edit mode is active and no higher-priority popup handles `Escape`, focused `Escape` cancels edit mode without mutating the draft buffer, image markers, caret, selection, or undo history.
- While thread-edit mode is inactive, focused `Alt+Up` and `Alt+Down` browse accepted composer history for the current conversation scope.
- Focused `Ctrl+Up` and `Ctrl+Down` scroll the transcript between turn boundaries without moving the draft caret or changing selection.
- `Ctrl+Up` first moves to the current turn's semantic start before earlier jumps move to previous turns. `Ctrl+Down` scrolls to the transcript bottom when no later turn boundary exists.
- Focused `Ctrl+Up` and `Ctrl+Down` are turn-to-turn navigation commands. They do not step between streamed chunks inside one huge turn; when the target turn is huge, the transcript feature anchors that turn and lazily streams chunks according to its own navigation contract.

## Image Markers And Assets

- Image paste exposes ready, paste-pending, and unavailable outcomes. A ready paste of supported
  image clipboard content inserts one compact marker at the captured caret or replaces the captured
  selection. Paste-pending preserves the prior draft, caret, selection, labels, and exact local
  paste intent through reconciliation. Proven success publishes the complete marker edit; proven
  noncommit leaves the prior draft authoritative. A terminally unavailable paste follows Durable
  Mutation Reconciliation and does not present the unchanged draft as proof of noncommit. An
  ordinary unavailable or failed pre-admission paste reports the reason and leaves the draft
  unchanged.
- Labels follow the visible sequence `A`, `B`, `C`, continuing spreadsheet-style when needed. A
  marker receives its final label before it appears; the composer never inserts an unresolved
  marker placeholder.
- Labels remain stable through draft editing, autosave, submission, accepted or queued input,
  steering, retry, delivery-unknown outcomes, restart, replacement editing, and historical
  presentation.
- Removing the final occurrence of a draft-only image may release its label only when that image
  has never been accepted or retained by queued or historical input. Removing an earlier label
  never renames later markers. A later paste may reuse a released draft-only gap only when that
  reuse is known to be safe; otherwise paste is unavailable without changing the draft.
- A label used by accepted conversation history remains reserved, including a historical gap.
  Multiple markers show the same label only when they refer to the same image.
- The compact marker such as `[A]` is presentation rather than ordinary authored text. Submission
  preserves marker order, stable label meaning, and repeated-reference identity according to the
  image-assets feature contract.
- While image-label readiness is pending, commands that create or move a marker into the
  conversation remain unavailable, while ordinary text input remains available. If the safe next
  label cannot be established, image paste presents an explicit unavailable or incomplete-history
  outcome and does not mutate the draft.
- A marker is one indivisible draft position for caret movement, selection, deletion, cut, paste,
  undo, and redo. Removing one occurrence removes only that draft reference and never removes an
  image already retained by accepted or queued input.
- Activating an editable composer marker offers `View` and `Remove`; it does not directly open
  inspection. `View` opens Beryl image inspection without submitting the draft or opening an
  external viewer. `Remove` uses the same editing outcome as keyboard deletion. Their placement and
  composition are defined in `gui.md`.
- Paste rejects before draft mutation for unsupported clipboard content, an exact product or
  runtime constraint, image-admission failure, or representable-domain exhaustion. Preview
  resource pressure presents the bounded pending or unavailable outcome defined by the image-assets
  feature without discarding the admitted image or marker.
- A draft may contain any representable number of markers without a smaller product count limit.
  Selection, restoration, and preview remain responsive through the bounded-resource behavior
  delegated in Implementation References.

## Image Clipboard Semantics

- Copying or cutting a selection containing image markers writes explanatory fallback text such as `[Image A]` to the system clipboard.
- While Beryl's private clipboard representation remains eligible, pasting it in the same
  conversation creates another reference to the same image with the same label. A stale
  representation that no longer identifies that label and image is rejected before draft mutation.
- Cutting and then pasting a marker is the user-visible way to move that image reference inside the draft.
- Pasting the private marker representation into another conversation allocates that conversation's
  own label, subject to the visible readiness outcomes above.
- Clipboard text that merely looks like `[Image A]` without valid Beryl metadata always pastes as ordinary text.

## Submission And Queuing

- Immediately before Beryl accepts work that will start a new turn, it must be able to confirm
  sufficient storage capacity. A below-reserve, unavailable, or indeterminate result rejects the
  attempt before acceptance or backend delivery. Direct submission preserves the exact current
  draft, editor state, and markers; a queued fragment remains visibly queued in its existing order.
- Active-turn steering does not start a new turn and is unaffected by this admission guard. A later
  storage failure is reported as that later operation's failure and does not
  retroactively become a below-reserve rejection.
- While the selected thread has a repair-pending turn, successor submission is unavailable with the
  current draft intact. Input already accepted for a later turn remains visible and queued until
  the affected turn is repaired or resolves as incomplete.
- When a non-empty draft is accepted for submission, it clears immediately and appears in the transcript as one distinct user input fragment.
- Rejected submissions, including empty drafts, backend-unavailable targets, pre-admission resource
  rejection, or image/submission preparation failure before acceptance, keep the draft intact and
  report the rejection.
- An ordinary submission with an `Indeterminate` durable outcome keeps the last coherent draft and
  editor state visibly reconciling, retains the exact captured submission intent and local evidence,
  and suppresses duplicate submission while the exact request is classified. Reconciled `ExactNew`
  publishes that submission's accepted send-and-clear result exactly once. Reconciled `ExactOld`
  restores the exact unchanged pre-attempt draft, markers, caret, selection, and undo history and
  reports that the submission was not accepted.
- Reconciled `Collision` makes that submission terminally `Unavailable`. It publishes neither
  acceptance nor noncommit, keeps the coherent pre-attempt editor presentation without treating it
  as proof that the draft was not accepted, and preserves the exact local submission intent and
  evidence. Duplicate submission and every action that depends on that admission remain unavailable
  under Durable Mutation Reconciliation; the draft is never sent again as recovery.
- Acceptance publishes one coherent outcome across composer clearing, transcript presentation,
  delivery state, and accepted-label stability; none appears accepted independently.
- Ordinary idle submission targets the currently selected thread state. A concurrent thread,
  draft, or eligibility change rejects the whole attempt and leaves the composer content intact
  for reconciliation or retry.
- Image-bearing acceptance requires every marker, final label, and image identity to agree.
  Missing, stale, duplicated, or disagreeing input rejects before acceptance and leaves the draft
  intact.
- When the current draft is in replacement-edit mode, submission follows the replacement behavior
  defined by `doc/features/conversation-threads/design.md` instead of ordinary append, steering, or
  queueing.
- If backend execution eligibility must be re-established after acceptance, the accepted turn
  remains visibly pending and is not accepted a second time.
- Once the backend accepts a live turn, its submitted user input is no longer editable. The user
  may stop it, branch after completion, or create later replacement work according to
  `doc/features/conversation-threads/design.md`.
- Soft-stopping an active live turn preserves the submitted turn and captured response with an
  explicit interrupted, incomplete, failed, or terminal outcome.
- Image-asset access or runtime preparation failure after acceptance is reported against that same
  accepted input. Its markers and labels remain visible under their accepted identity; the input is
  not restored to the draft, discarded, or accepted again.
- Each accepted composer send-and-clear event remains a distinct user input fragment, even when multiple fragments belong to one backend turn.
- Submitting the first draft of a thread requires its selected runtime and root to remain available;
  submission does not create the thread.
- While active-turn steering eligibility is unresolved, the accepted fragment remains visibly
  pending and is not duplicated or accepted again.
- Temporary local delivery congestion does not convert input accepted for a still-steerable active
  turn into a later ordinary turn. It remains pending unless exact target evidence later makes
  steering ineligible.
- If the exact active turn becomes terminally uncertain while Beryl can still receive late evidence
  about that turn, fragments accepted during that interval are rendered immediately and remain
  ordered for the next backend turn. Later evidence that the uncertain turn was still active does
  not retroactively convert those fragments into steering.
- If active-turn steering is rejected because the turn is no longer eligible, the same fragment
  remains in accepted order for the next eligible turn. It is not cleared and accepted again,
  merged with another fragment, or presented as delivered.
- During selected-thread terminal-history finalization or context compaction, accepted fragments are
  rendered immediately and queued, rather than steered, for the next backend turn after the owning
  operation completes.
- Multiple queued fragments preserve accepted order and remain separate visible user blocks.
- When the thread later becomes eligible, only the earliest effective queued fragment starts the
  next ordinary turn. That transition leaves the current draft and editor state untouched, while
  the accepted fragment remains permanent history linked to the turn it started.
- A proven pre-admission storage failure, representable-domain exhaustion, or an exact operation constraint rejects
  before composer clear, transcript presentation, backend delivery state, or visible marker and
  label state changes. Logical backlog size does not impose a smaller product limit or require the
  whole queue to become resident.
- User input fragments accepted before or during a stop request remain visible and ordered. Work
  proven not dispatched remains queued for the next eligible turn. A possibly dispatched fragment
  whose response was lost becomes delivery-unknown durable history and is not presented as
  next-turn queued work or replayed.
- Accepted fragments either reach the backend or remain explicitly presented as pending,
  terminally failed, or delivery-unknown. They are never presented as delivered without confirmed
  delivery.

## Composer History

- Composer history is bounded GUI-local session state containing exact accepted inputs. It is not
  persisted, does not originate from transcript history, and never changes submitted transcript
  content.
- Only accepted non-empty submissions enter history. Rejected submissions and whitespace-only text submissions do not.
- Consecutive duplicate accepted drafts collapse to one entry.
- When composer history reaches its configured bound, it evicts the oldest eligible entries without
  changing their submitted transcript inputs.
- Browsing history presents the exact selected accepted input in the current draft, including image
  markers, without requiring the complete input to become resident; it clears selection and places
  the caret at the logical end.
- The exact draft that existed when browsing began is restored when browsing forward past the
  newest history entry.
- Editing a recalled entry changes only the current draft and not the stored history entry.
- When thread-edit mode is active or no history exists in the requested direction, `Alt+Up` and
  `Alt+Down` leave the draft, caret, selection, composer presentation, and transcript viewport
  unchanged.

## External Draft Insertion

- Beryl remembers the latest draft insertion point so transcript quote actions can insert text even while the transcript has focus.
- A quote action inserts only text that the transcript currently presents as eligible for quoting.
- Transcript quote insertion updates the draft buffer, saved insertion position, and undo history through the same shared editing semantics as ordinary edits.
- Quote insertion must not change the system clipboard and does not force keyboard focus into the composer.
- Quote insertion is unavailable when the quoted transcript content is no longer current or
  available. The composer never substitutes stale or unseen transcript content.

## Developer Instructions On User Turns

- Non-empty global developer-instructions preferences are sent as hidden developer-instructions context when Beryl starts a top-level user-message turn.
- This includes first turns of new persistent user-facing threads, later turns in existing threads, and automatic continuation turns after lifecycle yield requests.
- Blank or whitespace-only developer-instructions settings are disabled.
- Retries and replacement starts use the latest applied developer-instructions setting.
- Developer instructions must not become transcript-visible user messages, current drafts, queued
  input fragments, accepted history, or backend-owned configuration.
- Developer instructions are not sent to subagent requests, active-turn steering, title-generation maintenance, inventory refreshes, lazy metadata reads, context-compaction requests themselves, or other background/status-only work.
