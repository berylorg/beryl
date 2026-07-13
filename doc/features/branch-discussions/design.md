# Goals

Let users branch from selected assistant text, explore the issue in an ordinary Syndic thread, and hand a resolved outcome back to the exact parent thread without semantic graph or checklist authority.

Keep discussion context, resolution intent, parent delivery, and archive state durable and understandable without presenting selected text as user-authored transcript narrative.

## Non-goals

- Providing a GUI command that resolves, archives, and merges a discussion.
- Creating a context-only transcript turn before the user submits input.
- Showing branch siblings or the complete Syndic DAG.
- Discarding queued user input to prioritize resolution.
- Archiving a discussion before its parent handoff turn succeeds.

# Decisions

## Supplemental Material

- `gui.md` is the normative supplemental GUI composition file for the in-transcript discussion-context record and composer-adjacent discussion status strip.
- Internal provenance, queue, idempotency, and recovery behavior are defined in `doc/systems/branch-discussion-handoff/design.md`.
- CAS projection of selected context is defined in `doc/systems/cas-live-syndic-transcript/design.md`.

## Discuss In New Branch

- A stable rendered assistant-text selection exposes `Discuss in new branch` alongside Quote.
- Activating it creates a new durable Syndic discussion thread and switches the invoking window to that exact thread through ordinary activation.
- The discussion's first current draft has the source turn as immutable parent and owns the exact selected text plus source turn, item, projection revision, and selected-range provenance.
- The discussion thread separately owns the exact parent Syndic thread id used for eventual handoff.
- Creation performs no CAS request and runs no model.
- Selected context longer than the approved 65,536-byte UTF-8 limit is rejected before thread creation and the source selection remains intact.
- The new discussion is an ordinary catalog thread with lineage, title behavior, runtime/root binding, durable draft, navigation history, and exclusive window claim.

## Context Presentation

- The selected context appears as one readonly synthetic context item in the discussion thread's transcript presentation.
- The item is positioned at the branch boundary immediately after the source turn and before the first branch-local submitted turn. It is the first branch-local presentation item, not necessarily the first item in inherited history.
- The item derives from the immutable context/provenance envelope. It is not a Syndic turn, does not change DAG parentage or turn counts, and is never projected to CAS as transcript input.
- The context item scrolls, anchors, virtualizes, and remeasures with the transcript instead of reserving fixed window space. Large selected passages use the transcript's existing bounded chunk presentation.
- The context remains selectable and copyable but cannot be edited, quoted as if it were a transcript message, branched again, or targeted by ordinary turn actions.
- Missing or invalid provenance never causes Beryl to substitute similar transcript text. The stable branch-boundary item shows an explicit unavailable-context state while preserving the discussion thread.

## Discussion Status

- Every selected branch-discussion thread shows one fixed-height discussion status strip immediately above the composer and below any visible activity panel.
- The strip remains mounted for the complete branch-discussion lifecycle so resolution state changes do not move or resize the composer.
- Its states are `Open`, `Resolution pending`, `Handing off`, `Handoff failed`, and `Archived`.
- `Open` means discussion input remains allowed subject to ordinary composer gates. A deferred resolution tool call caused by queued input leaves the strip in `Open`.
- `Resolution pending` begins when resolution intent is admitted and remains while the resolving turn or parent eligibility is pending. `Handing off` means the exact parent handoff turn is active.
- `Handoff failed` represents either a retryable failure with the same admitted job still live or a terminally failed attempt. `Retry handoff` appears only for the live retryable job. `Archived` begins only after successful parent handoff and durable archive publication.
- `Resolution pending`, `Handing off`, and retryable `Handoff failed` keep the composer inert. Terminal `Handoff failed` leaves the discussion unarchived and makes the composer editable again subject to ordinary composer gates; the failed status remains visible until a fresh resolution attempt is admitted.
- `Archived` is readonly. The strip state, retry availability, and composer writable or inert state publish atomically from the same discussion revision.
- The strip never exposes Resolve or Archive. `Handoff failed` may expose `Retry handoff` for the already admitted immutable job.
- Long failure detail uses the established per-window notice rather than expanding the strip.

## First Submission

- The user types and submits through the ordinary composer.
- Submission freezes the same context-bearing draft as the first submitted discussion turn and atomically creates the replacement current draft.
- Beryl supplies selected context to CAS once through the exact lossless selected-context projection owned by the CAS-live Syndic system; it is neither ordinary user input nor developer instructions.
- The selected text remains untrusted source context and cannot gain application-instruction authority merely because Beryl forwards it. Its CAS projection preserves its actual assistant provenance as one assistant-role output-text history item rather than fabricating a user turn.
- Later discussion turns do not copy the immutable context into new Syndic drafts or resend it to an already established CAS lineage.

## Resolution Tool

- Beryl registers one discussion-scoped dynamic tool on the discussion's exclusive CAS projection.
- The user initiates resolution conversationally. The AI calls the tool with the proposed resolution payload; it does not supply authoritative parent, child, thread, job, or archive identities.
- A tool call outside the exact bound discussion and active turn is rejected.
- If accepted future-turn input is queued, the call returns a structured retryable deferred result and changes no state. Beryl does not retry automatically.
- Deferred resolution leaves the composer enabled, leaves the discussion unarchived, and lets queued turns run normally.
- Intervening user input or steering may change or cancel the AI's intention to retry.

## Accepted Resolution

- Successful tool admission durably records the exact resolution intent before returning success to CAS.
- The discussion immediately enters resolution-pending state and accepts no new composer submission, steering, replacement edit, or other mutation that could alter the resolving path.
- The composer remains disabled while the admitted attempt is waiting, running, or retryably failed with its retryable job still live.
- The discussion remains unarchived until the exact parent handoff turn reaches terminal success.
- If the parent is active, unavailable, or temporarily ineligible, the durable handoff waits without blocking other threads.
- A retry of already admitted intent retries only the durable handoff job and cannot change the stored resolution payload or create duplicate parent input.
- A discussion may have only one live admitted attempt. A second resolution cannot be admitted while the current attempt is pending, active, or retryably failed.

## Parent Availability

- A parent open in another main window remains the same handoff destination; successful handoff activity appears in that owning window.
- Parent runtime, root, or CAS failure leaves the handoff pending or retryable and leaves the discussion unarchived.
- Beryl exposes no parent-thread deletion command. If exact parent identity is nevertheless missing or invalid before resolution admission, the tool rejects without accepting intent and the discussion remains editable and unarchived.
- Beryl never silently redirects resolution to an ancestor, sibling, replacement thread, or newly created thread.

## Completion And Navigation

- Archive state is Beryl-owned metadata and does not depend on CAS archive or thread-list state.
- After successful parent handoff and archival, the current window remains on the archived readonly discussion rather than switching automatically.
- The lineage strip remains the explicit route to the parent. If the parent is open elsewhere or unavailable, its breadcrumb remains represented and unavailable according to the conversation-thread contract.
- No successful or failed handoff automatically activates the parent or changes the owning window's selected thread.
- Archived discussions accept no new input. Their transcript and context remain readable.
- Click-to-focus of a parent open in another window remains deferred.

## Failure And Retry

- Retryable handoff failure keeps the admitted resolution, disabled composer, unarchived discussion, and exact parent binding.
- The discussion status strip exposes a `Retry handoff` command that retries the existing job only; it is not a resolve or archive command.
- Terminal handoff failure ends the live attempt, leaves the discussion unarchived, preserves `Handoff failed`, removes `Retry handoff`, and makes the composer editable again subject to ordinary gates.
- The terminally failed attempt and any parent handoff turn already appended for it remain durable. Beryl never creates a second parent turn or starts a fresh resolution attempt automatically.
- The user may continue the discussion and later initiate resolution conversationally again. A later tool admission creates a fresh intent and job from the then-current discussion; it is not a retry or replacement of the terminally failed attempt.
- Multiple attempts are allowed only sequentially after terminal failure. A retryable failure retains the sole live attempt, while successful handoff archives the discussion and permits no later attempt.
- Post-admission missing-parent, store, oversized context/recovery projection, or invariant failure is reported explicitly, transitions the attempt terminally when it cannot be retried safely, and never counts as successful archive.
