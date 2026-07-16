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
- If CAS may have accepted `turn/start` or one steering fragment before the confirming response was
  lost, Beryl also retains an explicit delivery-unknown fact for that input. This fact does not
  authorize delivery, non-delivery, or automatic replay.
- After retiring the unprovable projection, Beryl may relaunch CAS and establish a fresh exact
  projection from eligible durable Syndic history. Once that projection is ready, the composer is
  available for new input; recovery itself starts no replacement model turn and does not silently
  resend the interrupted input.
- Any later user-requested retry or continuation is a new durable submission. The incomplete turn
  and its prior external effects remain unchanged.
- If a background backend connection for title generation, inventory refresh, or lazy maintenance fails while the managed process remains usable, Beryl reports or logs only that operation's failure and keeps the active conversation usable.
- Backend launch, probe, or compatibility failure before a usable connection exists is reported as backend-unavailable for that runtime target, not as application startup failure.
- Runtime launch, required CAS projection, or foreground connection failure never replaces, hides, or closes an affected main conversation window.
- Each main conversation window whose selected thread is affected presents its own persistent, non-dismissible backend-unavailable error notice. The notice identifies the affected runtime and exposes a Retry command.
- Retry targets the same configured runtime, root, backend ownership mode, and exact Syndic thread binding. It may relaunch that managed backend, resume exact native CAS lineage, or establish a fresh projection through one-time recovery injection, but it must not switch the user to another runtime/root binding or silently choose a different Syndic thread.
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
- An active or unknown-terminal CAS turn is not eligible for this recovery choice. Beryl first
  converges that turn through its interrupted, incomplete, failed, or unknown-terminal lifecycle;
  it never abandons and replays potentially activated input from this prompt.
