# Syndic Accepted-Promotion Immediate Reconciliation Is Not Stable

## Scope

`syndic-storage` accepted-input promotion reconciliation assigned to root-plan Phase 61, including
the active transcript-build admission contradiction corrected first by Phase 60.

## Invalidated Approach

Classify an ambiguously surfaced promotion as `Exact` only while every mutable thread, input-gate,
draft-index, transcript, activity, binding, and route record still has the immediate post-promotion
shape.

## Decisive Evidence

Promotion commits a pending ordinary turn and moves the input gate to `PendingTurn`. Before the
caller reconciles that commit, another accepted-input admission can validly read the new gate and
append next-turn work. Accepted-input admission neither takes nor should take the long-lived
same-thread execution flight, because user input must remain admissible while an ordinary turn is
live.

The later admission advances several mutable records inspected by
`PromotionObservation::is_exact`, even though the immutable promotion witness and promoted
successor still prove that the original promotion committed. Current reconciliation therefore
misclassifies this legal descendant as `Collision`.

A focused promotion, transcript-build start, then queued-admission regression exposed a second
invalid assumption: queued admission advanced the broad thread revision without touching the
active build. The command committed, but exhaustive Syndic validation and reopen rejected
`active transcript build authority disagrees`. Merely relaxing promotion reconciliation would
therefore classify a structurally invalid durable state instead of correcting its publisher.

## Why It Fails

Immediate shared projection shape is not durable operation identity. A process-local mutex cannot
cover every legitimate publisher without coupling admission and execution, and the same-thread
flight cannot cover accepted admission without disabling live user input. Re-reading or retrying
the mutable shape only moves the race window.

## Course Correction

Promotion reconciliation must authenticate the immutable promotion witness and accept compatible
monotonic descendants of the promoted state. It must still reject a changed promotion identity,
successor, disposition, or incompatible lineage as `Collision`. The correction belongs in storage
authority and must be proven across concurrent post-promotion admission, reopen, and ambiguous
commit cuts before ordered dispatch is mounted. Current-draft saves are also compatible when only
the draft reverse revision and matching summary activity time advance coherently.

Before that reconciliation correction, queued admission must atomically supersede an observed
active transcript build and select one fresh stale generation while advancing the thread. It must
leave a completed current transcript untouched when no active build exists. This preserves the
strict active-build revision invariant instead of weakening it or allowing an old build to publish
a regressed history-summary revision.

## Affected Authority

The Operator approved the active-build publisher correction as root-plan Phase 60 and the
descendant-tolerant reconciliation as Phase 61. Their contracts belong in
`doc/systems/cas-live-syndic-transcript/design.md`,
`doc/systems/syndic-conversation-history/design.md`, and `crates/syndic-storage/doc/design.md`.
Ordered scheduling and dispatch follow as Phase 62 only after the corrected reconciliation passes
independent review.
