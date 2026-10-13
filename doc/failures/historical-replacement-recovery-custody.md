# Historical Replacement Recovery Custody

Phase 774 assumed the existing selected-composer tracked drain and failed-resident recovery
boundary could carry a historical import without an additional retirement contribution. Source
inspection and independent semantic review on 2026-10-13 invalidated that assumption.

## Decisive Evidence

`MainWindowComposerRecoverySnapshot` in
`crates/beryl-app/src/main_window/conversation_composer_owner/recovery.rs` retains close, flush,
restoration and protection custody. `fence_clean_recovery` requires completed window-close flush
readiness and agreement with the widget's current binding and history frontier.

An admitted historical import can instead retain its original begin/advance/start command scope
before flush can settle, or select a durable successor before deferred widget rebind and coherent
realization. The current snapshot cannot represent either stage. The selected service's
`retire_clean_window_close` and `rebind_candidate_for_recovery` consume clean-close custody;
`MainWindowComposerSlot::begin_disposal` starts the ordinary release flush. None transfers the
historical ticket and its command/proof/outcome scopes into recovery ownership.

The new host observation API classifies original start/cancel/termination receipts without writes,
adoption or release. The mounted same-generation resume method advances the original ticket only.
Neither has a production recovery caller. These partial capabilities do not accept recovery.
The qualification attempt also demonstrated that pending original reconciliation blocks
`HomeRecoveryCandidate::publish`. An observer accepting only a published `HomeStore` therefore
cannot supply the candidate's prerequisite terminal classification. The corrected host outcome
test qualifies healthy original-scope observation only and does not claim graph recovery.
An open import, including a proven noncommitted start, still requires exact nonselecting termination.
Replaying its old prepared capability against a replacement generation is forbidden.

## Required Correction

Establish the bounded historical-operation retirement
contribution in the owning
[adapter](../../crates/beryl-app/doc/design-catalog-and-composer.md#historical-replacement-adapter)
and [operation-lifetime](../../crates/beryl-app/doc/design-shell-lifecycle.md#historical-replacement-operation-lifetime)
authority. Specify exact capture and transfer into the existing selected-resident recovery owner,
qualified original-scope classification, eligible termination of an open import without old
capability replay, and typed terminal release before replacement graph publication. Qualify each
admitted stage and observer-loss boundary; do not disguise historical custody as a flush or ordinary
mutation.

The correction extends `ComposerHostFailedResident` and the existing selected-resident candidate
preparation, rather than the clean-close route described above. The earlier stop overstated the
need for an Operator decision: these are bounded technical prerequisites within the accepted
product architecture. A committed but unpresented import requires genuine prior-publication
protection and a separately authenticated changed-source successor; ordinary preserved-position
adoption remains strict. Original candidate receipt classification precedes fresh graph preparation
and publication. The Home pending gate blocks candidate publication and coherence election;
ordinary candidate reads do not themselves prove that every original scope has been classified.
The bounded all-resident observation pass performs no writes and preserves those original scopes
before any recovery cleanup command.

Complete mounted evidence and independent review accepted the correction, and production Edit is
enabled. Canonical `ImageMarker` support and bounded inline origins passed actual native menu,
import, inspection/focus and cancellation qualification; host image import alone was insufficient.
See the [accepted workflow](../audits/transcript-mounting-readiness.md#mounted-historical-replacement-editing-acceptance).

Native recovery also exposed the distinction between a protected publication and the replaced
durable root. Re-reading the predecessor root after committed import can legitimately fail while
its genuine protected widget view remains intact. Changed-source preparation validates its
authenticated imported positions against the fresh target; preserved-source preparation keeps its
storage validation. The marker-only target then exposed a range-source defect: `Validate(0)` tried
to find a text leaf in an authenticated zero-byte marker tree. Zero is its sole valid UTF-8 boundary;
the corrected text reader returns the empty bounded result after authenticating that root, while
object-gap validation remains separate.
