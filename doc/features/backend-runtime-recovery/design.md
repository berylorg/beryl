# Goals

Keep Beryl windows and Syndic threads usable when runtimes, roots, or backend connections are unavailable, while preserving exact backend ownership, security, and thread execution bindings.

Let users understand which backend-dependent actions are unavailable, which runtime target is affected, and which recovery actions are available without silently switching context.

## Non-goals

- Bundling, installing, or replacing Codex.
- Exposing operator-managed unauthenticated app-server listeners.
- Silently switching users to a different backend process, runtime, root, or thread after failure.
- Treating backend thread enumeration as the source of Beryl thread identity.

# Decisions

## Supplemental Files

- [`gui.md`](gui.md) is a normative supplemental GUI composition file for the per-window
  backend-unavailable notice and the native-lineage recovery prompt.

## Backend-Unavailable State

- Backend availability is visible per runtime.
- Missing or inaccessible configured Codex executable, managed process spawn failure, exact release
  admission rejection, or connection loss marks only the affected runtime backend-unavailable.
- Backend-unavailable runtimes disable backend-required operations for their bound threads until successful retry or runtime configuration recovery.
- Backend-unavailable state must not erase runtime/root records, change thread bindings, change selected threads, make Syndic history unavailable, or require application exit.
- Backend-unavailable user-facing states identify the affected runtime.
- Backend-supplied error text is shown only as bounded, explicitly truncated detail. Displayed text
  never changes recovery eligibility, the selected binding, or the available recovery actions.

## Shell Behavior During Backend Failure

- Opening the Beryl home and main conversation shells requires no backend launch, release admission, or backend thread enumeration.
- If a selected thread's runtime cannot launch or satisfy exact release admission, Beryl keeps the shell, selected thread, durable draft, thread navigation, runtime/root configuration, and Syndic history available.
- Composer typing, new empty-thread creation, existing-thread activation, thread selection, and
  other operations that require no backend remain available.
- Submission, context compaction, turn controls, title-generation maintenance, and other
  backend-required commands are unavailable for affected threads.
- No thread-rebind command is exposed. An unavailable binding remains exact until its configured runtime and root recover.
- One backend-unavailable configured executable runtime does not disable other usable executable runtimes, including another runtime in the same Host or WSL environment.

## Disabled Paths

- Operations that target incompatible or unavailable backends fail or present localized recovery and must not silently switch runtime, root, backend process, or thread.
- Missing branch backend primitives disable branch actions rather than allowing local transcript-copy emulation.
- Missing edit backend primitives or unprovable rollback scope disable edit actions rather than allowing local transcript mutation emulation.
- Exact soft interruption is Beryl's only stop operation. No hard-stop command, escalation, child
  termination, or background-cleanup fallback is exposed.
- A stopped or lost live stream preserves explicit interrupted, repair-pending, incomplete, failed,
  or unknown-terminal Syndic turn state rather than deleting work or silently mixing history sources.
- If eligible recovery cannot represent the required history within its approved bounds, submission
  remains rejected with the draft intact and the window explains that continuation is unavailable.

## Live-Capture Gap Recovery

- Any proven or suspected capture gap makes only the affected turn visibly repair-pending. Content
  shown for that turn is explicitly provisional until recovery resolves it.
- Successful recovery changes the turn to visibly repaired and identifies the repair provenance.
  Recovery that cannot establish complete canonical content changes the turn to visibly incomplete
  rather than presenting provisional content as complete.
- While a turn is repair-pending, its thread remains readable and its draft remains preserved, but
  successor submission, fork, rollback or replacement edit, and context compaction are unavailable
  with an explanation. Those actions become eligible again only after the turn is repaired or
  resolved as incomplete.
- Repair-pending state gates only the affected thread. Other threads and healthy runtimes remain
  available for independent work.

## Connection Loss Recovery

- If the foreground backend connection or managed process is lost, the GUI keeps Beryl-home state, runtime/root records, selected Syndic thread, active transcript selection, and durable draft intact.
- When the owning backend process or execution session is known to be gone before a repair can be
  completed, the affected turn remains visible with its submitted input, its durably captured
  assistant prefix, and an explicit incomplete outcome. It does not remain indefinitely active.
- Capture disagreement follows the same visible repair-pending, repaired, or incomplete outcomes as
  every other capture gap.
- If CAS may have accepted `turn/start` or one steering fragment before the confirming response was
  lost, Beryl also retains an explicit delivery-unknown fact for that input. This fact does not
  authorize delivery, non-delivery, or automatic replay.
- When execution authority is unprovable, backend-backed composer actions remain unavailable until
  the same exact binding is usable again. Recovery itself starts no replacement model turn and does
  not silently resend the interrupted input.
- If a distinct follow-up was already durably admitted while the interrupted turn was stopping,
  Beryl may start only that admitted follow-up after the predecessor's exact eligible context is
  usable again. The interrupted turn remains visibly
  incomplete, is never resent, and any uncertain assistant or provider state keeps continuation
  unavailable instead of being presented as history.
- After a repair-pending turn reaches a repaired or incomplete resolution, another submission
  remains unavailable until the same exact binding is usable again. Draft typing and persistence
  remain available during that work, which starts no model turn and does not change the resolved
  transcript entry.
- Any later user-requested retry or continuation is a new durable submission. The incomplete turn
  and its prior external effects remain unchanged.
- If a background backend connection for title generation, inventory refresh, or lazy maintenance fails while the managed process remains usable, Beryl reports or logs only that operation's failure and keeps the active conversation usable.
- Backend launch or exact release admission failure before a usable connection exists is reported as backend-unavailable for that runtime target, not as application startup failure.
- After Beryl-home store recovery, affected backend-backed actions remain unavailable until the
  same runtime, root, and thread binding is usable again. A turn affected by that failure must
  become visibly repaired or incomplete before successor work becomes available.
- Runtime launch, required CAS projection, or foreground connection failure never replaces, hides, or closes an affected main conversation window.
- Each main conversation window contributes one persistent, non-dismissible backend-unavailable
  error notice while its selected thread is affected. The notice identifies the affected runtime
  and exposes a `Retry` command.
- The notice is ineligible when the selected thread is unaffected, when only background maintenance
  or capture repair failed without making the selected runtime unavailable, and while the
  native-lineage recovery prompt presents the same blocking condition. A failure associated only
  with another thread, window, or runtime does not make this notice eligible.
- Retry targets the same configured runtime, root, and exact Syndic thread
  binding and source. It must not switch the user to another runtime/root binding, choose a
  different thread, or silently select a different recovery path.
- While Retry is pending, the same notice remains eligible and the command remains visible but
  unavailable. Repeated activation cannot create a duplicate recovery attempt.
- An unsuccessful Retry updates the same notice with bounded failure feedback, leaves selected
  history and draft intact, and makes `Retry` available again only if the same recovery condition
  remains eligible. It never stacks a second notice or silently starts another attempt.
- Successful recovery removes the notice only after the selected thread's required backend
  operations are usable again.
- While the notice is present, only affected backend-required commands are gated. The main shell,
  Syndic history, thread navigation, and healthy local draft state remain visible and usable.

## Native-Lineage Recovery Decision

- When recovery cannot use a thread's current source, Beryl preserves the exact binding and requires
  an explicit choice between `Retry` and `Recover from Syndic history` before backend-backed work
  can continue.
- The choice is shown in place of the selected thread's composer only when submission or an already
  admitted pending turn actually requires the failed projection. Speculative CAS warm-up failure
  does not replace a focused editor or prevent continued drafting.
- Entering the recovery prompt preserves the current durable draft and its compact host-owned caret,
  selection, bounded undo/redo availability, and scroll restoration facts while unmounting the
  editor widget and releasing its resident ranges and work. The prompt permits no draft mutation.
  Closing, switching, or exiting still applies the ordinary draft flush and preservation rules.
- `Retry` keeps the binding and retries the same exact source. `Recover from Syndic history`
  explicitly authorizes the separately eligible recovery path for the selected target. Recovering
  one selected thread never invalidates an unrelated thread or a different parent thread.
- While either command is pending, both commands remain visible but unavailable, duplicate
  activation cannot create another recovery attempt, and the prompt remains in place. Failure
  returns to the same prompt with bounded feedback and both commands' current eligibility without
  clearing the draft, changing the selected thread, or dismissing the underlying condition.
- If no user input was durably admitted, successful recovery mounts an eligible replacement editor,
  restores the exact draft, caret, selection, bounded undo/redo availability, and scroll position
  through fresh bounded range requests, and waits for a new submission command. If a pending turn
  was already durably admitted, successful recovery continues only that exact pending delivery and
  never creates a duplicate admission.
  Input whose delivery may already have occurred is never replayed automatically.
- The recovery command is unavailable when exact history cannot be represented within the approved
  recovery contract. Its disabled explanation names the blocking history or representation
  condition.
- `Recover from Syndic history` is unavailable while the selected path contains a repair-pending
  turn. The choice remains unavailable until that turn becomes repaired or incomplete.
- An active or unknown-terminal CAS turn is not eligible for this recovery choice. Beryl first
  converges that turn through its interrupted, incomplete, failed, or unknown-terminal lifecycle;
  it never abandons and replays potentially activated input from this prompt.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers:

- `external-side-effects/v1`
- `availability-required/v1`

Availability protects unaffected local workflows and recovery of the same durable binding; it does
not promise uninterrupted backend service.
