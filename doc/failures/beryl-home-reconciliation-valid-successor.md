# Beryl-Home Reconciliation Of A Valid Successor

Scope: cross-domain HomeStore reconciliation when a later authorized mutation supersedes one
participant of an earlier indeterminate command.

Phase 174 first acceptance records one exact reconciliation descriptor spanning Syndic admission
and the draft-to-accepted-input asset-owner transfer. A later valid promotion can durably move that
accepted-input asset ownership to the submitted turn item before first-acceptance reconciliation.
The natural state then contains the permanent Syndic acceptance/promotion proof and exact promoted
asset owner, but it is neither the descriptor's exact old side nor its immediate exact new side.

HomeStore reconciliation intentionally accepts only unanimous exact-old or exact-new domain
classifications. The valid successor therefore becomes command-level `Collision` and the registry
scope becomes `Closed`. Current authority and APIs provide no typed semantic-successor
acknowledgement that can validate the permanent proof, vacate that closed scope, and release its
slot and byte charge; closed scopes are released only during orderly store close. Syndic-only
reconciliation cannot repair the separately superseded asset participant.

The rejected workaround was to bypass HomeStore reconciliation with app-level natural-record
reads. That would erase or strand HomeStore custody and violate the rule that an indeterminate
descriptor must be reconciled before any reread, retry, publication, or release.

The accepted correction is successor-aware reconciliation before collision sealing. The original
command declares one bounded typed protocol with exactly one source and zero or more witnesses; the
source authenticates a fixed-size correlation from descriptor-bound records, witnesses validate it
through predeclared quota-enforced typed point reads, and passive participants must remain exact
new. Complete agreement returns `ExactSuccessor` and vacates the scope in the same registry
transition. Any missing role, mismatch, unresolved observation, old-side participant, or invalid
derived record remains collision.

A post-collision acknowledgement was rejected. Closed custody has already discarded the descriptor,
typed hooks, exact receipt, and current snapshot and has cached terminal collision; making it safe
would recreate another verification state with essentially the same authority and greater churn.
The accepted boundary exposes no caller proof, acknowledgement, reset, release, or app reread path.

Affected work: Phase 174 of `doc/plan.md` implements the generic HomeStore protocol. Phase 175 then
binds first acceptance and accepted-input promotion to the shared correlation and completes exact-
root submission without retaining live custody.
