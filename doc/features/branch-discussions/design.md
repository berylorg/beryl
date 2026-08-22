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

- [`gui.md`](gui.md) is the normative supplemental GUI composition file for the in-transcript discussion-context record and composer-adjacent discussion status strip.
- Internal provenance, queue, idempotency, and recovery behavior are defined in `doc/systems/branch-discussion-handoff/design.md`.
- CAS projection of selected context is defined in `doc/systems/cas-live-syndic-transcript/design.md`.

## Discuss In New Branch

- A stable rendered assistant-text selection from a finalized reply exposes `Discuss in new branch` alongside Quote.
- Activating it captures one creation request containing the exact selected UTF-8 bytes and their
  exact source provenance. That request preserves the captured bytes and provenance through
  pending and reconciliation instead of recapturing a later selection. Creation performs no model
  request.
- While creation is pending, the command remains visibly busy and cannot be activated again. The
  invoking window stays on the source thread with the exact selection intact while creation is
  classified as proven success, proven noncommit, or terminally unavailable.
- An indeterminate durable outcome changes the command to a visible reconciling state. Duplicate
  activation remains suppressed while Beryl classifies that exact request.
- A proven creation success first creates exactly one durable ordinary discussion thread that is
  discoverable through the catalog, then requests ordinary activation of that exact thread. The
  discussion preserves the source thread as its handoff destination and the exact selected passage
  as immutable historical context with its assistant provenance.
- Successful activation switches the invoking window to that discussion exactly once. If activation
  has a proven ordinary failure after durable creation, the source thread and its exact selection
  remain coherent, the created discussion remains discoverable, and the failure exposes `Retry`.
  Retry activates that same discussion through the ordinary activation path; it never repeats
  creation or creates a substitute discussion. A terminally unavailable activation follows the
  conversation-thread mutation contract and exposes no activation retry.
- A proven noncommit or ordinary creation failure leaves the source thread current, preserves the
  exact selection, reports the failure, and creates no discussion. Reconciliation never creates a
  second branch for the same request or switches to a substitute thread.
- A creation `Collision` instead makes that exact request terminally `Unavailable`. It neither
  reports a created discussion nor claims that none was created, and it keeps the coherent source
  thread, exact selected bytes and provenance, and local outcome evidence intact. The command stays
  unavailable and the exact creation request can never be repeated. Its persistent bounded
  explanation points only to established same-home recovery and bounded diagnostic reporting; it
  exposes no creation retry, resubmission, rollback, or manual repair command. Unrelated source-
  thread and discussion work remains available when otherwise healthy.
- Selected context longer than the approved 65,536-byte UTF-8 limit is rejected before creation;
  the source thread and selection remain intact.
- The new discussion follows ordinary catalog, lineage, title, runtime/root, navigation, composer,
  and exclusive-window behavior.

## Context Presentation

- The selected context appears as one readonly synthetic context item in the discussion thread's transcript presentation.
- The item is positioned at the branch boundary immediately after the source turn and before the first branch-local submitted turn. It is the first branch-local presentation item, not necessarily the first item in inherited history.
- The item is not a user-authored or assistant-authored transcript turn and does not change visible turn counts.
- The context item scrolls and anchors with the transcript instead of reserving fixed window space. Large selected passages remain usable through the transcript's bounded presentation behavior.
- The context remains selectable and copyable but cannot be edited, quoted as if it were a transcript message, branched again, or targeted by ordinary turn actions.
- Missing or invalid provenance never causes Beryl to substitute similar transcript text. The stable branch-boundary item shows an explicit unavailable-context state while preserving the discussion thread.
- Later replacement or divergence in the parent thread does not remove or invalidate an already admitted discussion context. The recorded parent thread remains the handoff destination while the quoted source turn remains immutable historical context.
- Replacing the discussion's first submitted input likewise does not remove or relocate the synthetic context item, even when the original first turn is no longer on the discussion's selected path.

## Discussion Status

- Every selected branch-discussion thread shows one discussion status surface for the complete
  branch-discussion lifecycle.
- Its states are `Open`, `Resolution pending`, `Handing off`, `Handoff failed`, `Unavailable`, and
  `Archived`.
- `Open` means discussion input remains allowed subject to ordinary composer gates. A deferred resolution tool call caused by queued input leaves the strip in `Open`.
- `Resolution pending` begins when resolution intent is admitted and remains while the resolving turn or parent eligibility is pending. `Handing off` means the exact parent handoff turn is active.
- `Handoff failed` distinguishes a retryable failure from a terminally failed attempt through the presence or absence of `Retry handoff`. `Archived` begins only after successful parent handoff and durable archive publication.
- `Unavailable` is the terminal result when reconciliation of exact resolution admission, parent
  handoff, or archive publication proves neither success nor noncommit. It preserves the last
  coherent strip presentation, exact resolution intent and parent binding, any already visible
  parent turn, and exact locally held evidence without claiming archive or restoring the earlier
  state as a proven noncommit.
- `Resolution pending`, `Handing off`, retryable `Handoff failed`, and `Unavailable` keep the
  composer inert. Terminal non-collision `Handoff failed` leaves the discussion unarchived and makes
  the composer editable again subject to ordinary composer gates; the failed status remains visible
  until a fresh resolution attempt is admitted.
- `Archived` is readonly. The strip state, retry availability, and composer writable or inert state change together without an inconsistent intermediate presentation.
- The strip never exposes Resolve or Archive. `Handoff failed` may expose `Retry handoff` for the
  already accepted resolution attempt. `Unavailable` exposes no retry, resubmission, rollback, or
  manual repair command and suppresses every duplicate or repeated mutation for that exact attempt.
- Long failure or unavailable detail is reported persistently and within bounded detail through the
  established per-window notice. It points only to established same-home recovery and bounded
  diagnostic reporting; unrelated healthy threads remain available.

## First Submission

- The user types and submits through the ordinary composer.
- The first submission uses the exact immutable selected passage at the branch point as prior assistant context. It is neither ordinary user input nor developer instructions.
- The selected text remains untrusted source context and cannot gain application-instruction authority merely because Beryl supplies it to the discussion. Its actual assistant provenance is preserved rather than presented as a fabricated user turn.
- The exact selected context is supplied once for the discussion lineage. Later turns neither duplicate it nor present it as newly authored content.

## Resolution Tool

- The user initiates resolution conversationally. The AI may propose one nonempty resolution of at most 65,536 Unicode scalar values only for the open discussion in which that conversation is occurring.
- Invalid, oversized, out-of-context, stale, or otherwise ineligible resolution requests are rejected without accepting resolution intent or changing the discussion.
- If accepted future-turn input is queued, resolution is deferred as retryable without changing discussion state. Beryl does not retry automatically.
- Deferred resolution leaves the composer enabled, leaves the discussion unarchived, and lets queued turns run normally.
- Intervening user input or steering may change or cancel the AI's intention to retry.
- Once an otherwise eligible resolution admission is durably attempted, an indeterminate result
  keeps the last coherent `Open` presentation, exact proposed resolution, parent binding, and local
  evidence while the tool reply remains unresolved and duplicate admission is suppressed. Proven
  success enters `Resolution pending`; proven noncommit leaves the discussion `Open` without
  reporting admission. `Collision` enters terminal `Unavailable` and neither tells the AI that
  admission succeeded nor reopens the discussion as if noncommit were proved.

## Accepted Resolution

- Successful admission preserves the exact resolution intent before the AI is told it succeeded.
- The discussion immediately enters resolution-pending state and accepts no new composer submission, steering, replacement edit, or other mutation that could alter the resolving path.
- The composer remains disabled while the admitted attempt is waiting, running, or available for retry after a retryable failure.
- The discussion remains unarchived until the exact parent handoff turn reaches terminal success.
- If the parent is active, unavailable, or temporarily ineligible, the handoff waits without blocking other threads.
- A retry of already admitted intent retries only that same handoff and cannot change the accepted resolution or create duplicate parent input.
- A discussion may have only one live admitted attempt. A second resolution cannot be admitted while the current attempt is pending, active, or retryably failed.
- An unavailable attempt likewise admits no retry or fresh resolution for that exact unresolved
  scope. Unrelated threads and operations remain available, but no action dependent on that result
  is admitted.

## Parent Availability

- A parent open in another main window remains the same handoff destination; successful handoff activity appears in that owning window.
- Parent runtime, root, or CAS unavailability leaves the same handoff pending or retryable with its
  exact parent binding intact and leaves the discussion unarchived. Temporary unavailability alone
  never terminalizes or redirects the handoff or admits the resolution again.
- Beryl exposes no parent-thread deletion command. If exact parent identity is nevertheless missing or invalid before resolution admission, the tool rejects without accepting intent and the discussion remains editable and unarchived.
- Beryl never silently redirects resolution to an ancestor, sibling, replacement thread, or newly created thread.

## Completion And Navigation

- Archive state is shown consistently on every discussion surface and does not depend on whether the parent is currently visible.
- After successful parent handoff and archival, the current window remains on the archived readonly discussion rather than switching automatically.
- The lineage strip remains the explicit route to the parent. If the parent is open elsewhere or unavailable, its breadcrumb remains represented and unavailable according to the conversation-thread contract.
- No successful or failed handoff automatically activates the parent or changes the owning window's selected thread.
- Archived discussions accept no new input. Their transcript and context remain readable.
- Activating a parent already open in another window does not move focus to that other window; the unavailable breadcrumb explains that the parent is open elsewhere.

## Failure And Retry

- Retryable handoff failure keeps the admitted resolution, disabled composer, unarchived discussion, and exact parent binding.
- The discussion status strip exposes a `Retry handoff` command that retries the same accepted handoff only; it is not a resolve or archive command.
- Terminal handoff failure ends the live attempt, leaves the discussion unarchived, preserves `Handoff failed`, removes `Retry handoff`, and makes the composer editable again subject to ordinary gates.
- If external parent-handoff completion is unknown, Beryl does not expose `Retry handoff` or resend
  while doing so could duplicate delivery. A non-collision attempt that cannot complete safely
  becomes a visible terminal failure and retains any incomplete parent turn.
- A durable-mutation `Collision` is not converted into terminal `Handoff failed`: it remains
  `Unavailable`, retains the exact admitted resolution, parent binding, incomplete parent turn and
  local evidence, and neither archives nor releases the discussion for another handoff or
  resolution attempt.
- The terminally failed attempt and any parent handoff turn already shown for it remain available as history. Beryl never creates a second parent turn or starts a fresh resolution attempt automatically.
- After a terminal non-collision handoff failure, the user may continue the discussion and later
  initiate resolution conversationally again. A later accepted resolution is a fresh attempt from
  the then-current discussion; it is not a retry or replacement of the terminally failed attempt.
- Multiple attempts are allowed only sequentially after terminal non-collision failure. A retryable
  failure retains the sole live attempt, `Unavailable` permanently suppresses another attempt for
  its exact terminal scope, and successful handoff archives the discussion and permits no later
  attempt.
- Any non-collision unrecoverable post-admission failure is reported explicitly, ends the attempt
  without archive, and never counts as a successful handoff.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers:

- `external-side-effects/v1`
