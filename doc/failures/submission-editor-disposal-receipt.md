# Submission Omits The Editor Disposal Receipt

The unchanged-opening work assumed accepted send-and-clear could reuse the existing terminal
session path. Source inspection disproved that assumption before changing admission: both idle
submission and accepted-input admission construct a disposed session through
`crates/syndic-storage/src/mutation/admission/shared.rs::load_base`, then
`CommonRecords::contribute` writes only its mutable head. Neither path writes its disposal receipt.

`draft_piece/session.rs::candidate_session_closure_is_exact_in_store` sends every disposed head to
`publication::candidate_session_disposal_is_exact_in_store`. That validator requires the exact
disposal receipt selected by the head and has no acceptance-record alternative. A normal session
read therefore rejects the successfully accepted terminal state as `InvariantFailure`.

The existing [draft storage contract](../../crates/syndic-storage/doc/design-draft-storage.md) and
[schema](../../crates/syndic-storage/doc/design-schema-v7.md) already require exact disposal receipts.
The bounded correction is to publish the existing receipt and disposed head atomically with accepted
submission, preserving canonical source/terminal closure, occupied-identity rejection, and exact
reconciliation. This finding requires neither a new persisted format nor weaker validation.

Phase 308 implemented and independently accepted that correction. Ordinary disposal and both
submission routes share one prepared disposal contribution; reconciliation reserves both bounded
session records. Historical publication and disposal evidence is validated independently of the
obsolete current draft, while admission still checks the exact current draft and selector.

The four `phase308_submission_disposal` tests and eight selected Phase 137/167 regressions passed
through locked nextest with `test-faults`, including normal terminal-session reads after replacement
and reopen, exact receipt replay, both submission routes at atomic fault cuts, and occupied receipt
rejection without partial clear. The existing image fixture was corrected to use its explicit
test-only marker builder. The locked storage library check, formatting, and independent adversarial
persistence review passed. The added receipt has fixed-size family bounds and no marker-count-
dependent traversal or allocation. Phase 304 subsequently accepted unchanged-opening normalization
and storage submission status under the [opening contract](pristine-editor-publication.md); the
app integration remains Phase 307 and is not established by this receipt evidence.
