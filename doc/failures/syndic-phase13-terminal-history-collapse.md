# Scope

Phase 13 CAS-live turn terminal admission across `beryl-app` and `syndic-storage`.

# Invalidated Approach

The first replacement schema encoded one `TurnEndStatus` outcome and allowed a typed incomplete
reason only when that outcome was `Incomplete`. Terminal mutation rejected provider `Complete`
while an observed item remained open or carried an unsupported-history disposition.

# Evidence

`crates/beryl-app/doc/design.md` requires unknown, malformed, or unsupported history-relevant items
to preserve the exact provider terminal fact while preventing history-complete publication through
a typed incomplete reason. The initial `TurnEndStatus` and `TurnStateRecord` shapes could express
either provider `Complete` without that reason or local `Incomplete` with the reason, but not both
facts together.

# Why It Failed

Converting provider `Complete` into local `Incomplete` would rewrite execution authority. Accepting
`Complete` without the reason would falsely publish complete captured history. Rejecting the
terminal event would lose an exact provider terminal fact and could retain an unnecessary
same-thread execution block.

# Course Correction

Exact provider or local execution outcome and captured-history completeness are independent durable
facts. Turn-ending status stores the exact outcome plus an optional typed history-incomplete reason.
A locally `Incomplete` outcome requires a reason; provider `Complete` may carry a reason. Turn
lifecycle follows the outcome, while history summary, canonical finalization, and transcript
publication remain behind their independent completeness requirements.

A supported provider-completed `PendingAsset` is not an item-audit failure and needs no terminal
reason. Its resource disposition and finalized-item frontier independently keep history incomplete
until the later asset checkpoint resolves it.

# Authority And Verification

The correction is authoritative in the CAS-live and Syndic-history system docs, the storage and app
package docs, Phase 13 of `doc/plan.md`, and Checkpoint 3 of the Beryl-home rework tracker. Codec,
terminal-admission, reopen-corruption, provider-complete-with-incomplete-history, and app terminal
integration tests must preserve the split.
