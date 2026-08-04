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

- `gui.md` is a normative supplemental GUI composition file for the per-window
  backend-unavailable notice and the native-lineage recovery prompt.

## Implementation References

- Backend launch, listener security, capability probing, process lifecycle, connection recovery, and protocol ownership are defined in `doc/systems/backend-runtime/design.md`.
- CAS-live transcript capture and selected-history authority are defined in `doc/systems/cas-live-syndic-transcript/design.md`.

## Backend-Unavailable State

- Backend availability is visible per runtime.
- Missing or inaccessible configured Codex executable, managed process spawn failure, probe failure, incompatible required capability, or connection loss marks only the affected runtime backend-unavailable.
- Backend-unavailable runtimes disable backend-required operations for their bound threads until successful retry or runtime configuration recovery.
- Backend-unavailable state must not erase runtime/root records, change thread bindings, change selected threads, make Syndic history unavailable, or require application exit.
- Backend-unavailable user-facing states identify the affected runtime.
- Backend-supplied error text is shown only through the bounded explicitly truncated diagnostic
  projection defined by the backend and notification contracts. Error code and closed typed
  recovery verdicts remain distinct; displayed text never authorizes retry, source retirement, or
  a lineage decision.

## Shell Behavior During Backend Failure

- Opening the Beryl home and main conversation shells requires no backend launch, compatibility probe, or backend thread enumeration.
- If a selected thread's runtime cannot launch or probe, Beryl keeps the shell, selected thread, durable draft, thread navigation, runtime/root configuration, and Syndic history available.
- Composer typing, new empty-thread creation, existing-thread activation, thread selection, and other Fjall-backed operations that require no CAS remain available.
- Submission, context compaction, turn controls, title-generation maintenance, and other CAS-backed commands are unavailable for affected threads.
- No thread-rebind command is exposed. An unavailable binding remains exact until its configured runtime and root recover.
- One backend-unavailable configured executable runtime does not disable other usable executable runtimes, including another runtime in the same Host or WSL environment.

## Disabled Paths

- Operations that target incompatible or unavailable backends fail or present localized recovery and must not silently switch runtime, root, backend process, or thread.
- Missing branch backend primitives disable branch actions rather than allowing local transcript-copy emulation.
- Missing edit backend primitives or unprovable rollback scope disable edit actions rather than allowing local transcript mutation emulation.
- Missing hard-stop support disables only affected hard-stop escalation controls and must not disable soft interruption.
- CAS historical turn reads are not a user-visible recovery path for selected transcript rendering after CAS-live Syndic capture cutover.
- A stopped or lost live stream preserves explicit interrupted, incomplete, failed, or unknown-terminal Syndic turn state rather than deleting work or silently recovering from CAS historical reads.
- If a stale or lost exclusive CAS projection cannot inject its complete required Syndic prefix once into a fresh CAS thread within the approved and proven budget, submission remains rejected with the draft intact and the window reports that fresh continuation is unavailable because the history is too large or cannot be represented exactly.

## Connection Loss Recovery

- If the foreground backend connection or managed process is lost, the GUI keeps Beryl-home state, runtime/root records, selected Syndic thread, active transcript selection, and durable draft intact.
- A turn whose owning CAS process or exact loaded execution session is proven gone remains visible
  with its submitted user input, every durably captured assistant prefix, and an explicit incomplete
  outcome. Beryl does not keep that thread indefinitely locked as though late events could still
  arrive from the dead session.
- If an item completion disagrees with the assistant or plan text already received live, the
  affected turn remains visible with its live text and an explicit incomplete-history outcome.
  Beryl does not replace that text, label the conversation corrupt, switch threads, or create a new
  backend thread automatically.
- If CAS may have accepted `turn/start` or one steering fragment before the confirming response was
  lost, Beryl also retains an explicit delivery-unknown fact for that input. This fact does not
  authorize delivery, non-delivery, or automatic replay.
- After retiring the unprovable projection, Beryl may relaunch CAS and establish a fresh exact
  projection from eligible durable Syndic history. Once that projection is ready, the composer is
  available for new input; recovery itself starts no replacement model turn and does not silently
  resend the interrupted input.
- If a distinct follow-up was already durably admitted while the interrupted turn was stopping,
  Beryl may establish that fresh projection from the predecessor's exact eligible authority-lost
  context and then start only the admitted follow-up. The interrupted turn remains visibly
  incomplete, is never resent, and any uncertain assistant or provider state keeps continuation
  unavailable instead of being presented as history.
- After a narrative mismatch reaches terminal state, Beryl automatically reacquires the same
  persistent backend thread through exact resume on a fresh proven session before allowing another
  submission. This includes a thread whose original history was established by one successful
  recovery injection; its injected prefix is not sent again. Draft typing and persistence remain
  available during that work. Reacquisition starts no model turn and leaves the incomplete
  transcript entry unchanged.
- Any later user-requested retry or continuation is a new durable submission. The incomplete turn
  and its prior external effects remain unchanged.
- If a background backend connection for title generation, inventory refresh, or lazy maintenance fails while the managed process remains usable, Beryl reports or logs only that operation's failure and keeps the active conversation usable.
- Backend launch, probe, or compatibility failure before a usable connection exists is reported as backend-unavailable for that runtime target, not as application startup failure.
- Runtime launch, required CAS projection, or foreground connection failure never replaces, hides, or closes an affected main conversation window.
- Each main conversation window whose selected thread is affected presents its own persistent, non-dismissible backend-unavailable error notice. The notice identifies the affected runtime and exposes a Retry command.
- Retry targets the same configured runtime, root, backend ownership mode, and exact Syndic thread
  binding. It may relaunch that managed backend, resume exact native or successfully published
  recovered CAS lineage, or establish a fresh projection through one-time recovery injection when
  that separate path is eligible, but it must not switch the user to another runtime/root binding
  or silently choose a different Syndic thread.
- Source-preserving or unclassified native resume/fork failures first receive bounded automatic
  retries against the same exact binding. Fresh recovery injection is selected automatically only
  when Beryl has authoritative proof that the native source is unusable.
- An unsuccessful Retry leaves the notice, selected history, and draft intact. Successful recovery removes the notice only after the selected thread's required CAS-backed operations are usable again.
- While the notice is present, only affected CAS-backed commands are gated. The main shell, Syndic history, thread navigation, and healthy Fjall-backed draft state remain visible and usable.

## Native-Lineage Recovery Decision

- When bounded automatic native resume/fork retries are exhausted without authoritative source-loss
  proof, Beryl preserves the exact binding and requires an explicit choice between `Retry` and
  `Recover from Syndic history` before that projection can execute.
- The choice is shown in place of the selected thread's composer only when submission or an already
  admitted pending turn actually requires the failed projection. Speculative CAS warm-up failure
  does not replace a focused editor or prevent continued drafting.
- Entering the recovery prompt preserves the current durable draft and any coherent in-memory
  editor state. The prompt permits no draft mutation. Closing, switching, or exiting still applies
  the ordinary draft flush and preservation rules.
- `Retry` keeps the binding and repeats bounded acquisition against the same exact source.
  `Recover from Syndic history` explicitly authorizes Beryl to stop relying on that source for the
  selected target, create a fresh CAS thread, inject the complete eligible Syndic prefix once, and
  bind the result. It retires a selected thread's own unusable binding but never invalidates a
  different parent thread merely because the selected child could not fork from it.
- While either command is running, duplicate activation is disabled and the prompt remains in
  place. Failure returns to the same prompt without clearing the draft, changing the selected
  thread, or dismissing the underlying condition.
- If no user input was durably admitted, successful recovery restores the exact draft editor and
  waits for a new submission command. If a pending turn was already durably admitted, successful
  recovery continues only that exact pending delivery and never creates a duplicate admission.
- The recovery command is unavailable when exact history cannot be represented within the approved
  recovery contract. Its disabled explanation names the blocking history or capability condition.
- In particular, `Recover from Syndic history` is unavailable when the selected path crosses a
  completion/live narrative mismatch. While Beryl still holds the quarantined same-process
  subscription anchor, `Retry` attempts the exact fresh-connection handoff to that same in-memory
  backend thread. Failure or closure of only the fresh replacement keeps the old anchor and leaves
  `Retry` available through another fresh connection. If the anchor or process is lost, exact
  continuation is unavailable. Beryl never cold-resumes recovered lineage or injects a shorter
  prefix and presents it as exact continuation.
- An active or unknown-terminal CAS turn is not eligible for this recovery choice. Beryl first
  converges that turn through its interrupted, incomplete, failed, or unknown-terminal lifecycle;
  it never abandons and replays potentially activated input from this prompt.
