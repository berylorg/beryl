# Candidate Worker Custody

A preparation worker retaining its own slot until completion does not alone preserve abandoned
recovery custody. During phase 749 review, dropping the consumer left the detached task as the
last slot owner. It returned the candidate and keyed result, then dropped both after notification.
`HomeRecoveryCandidate::drop` deliberately forgets its home lifecycle custodian until process exit
(`crates/beryl-home-store/src/recovery.rs`); dropping this value is not explicit abort or cleanup.
A weak-pointer lifetime test incorrectly treated disappearance after completion as success.

The corrected worker returns a separate recovery-owned custody receiver. The notification consumer
may disappear while that receiver retains the returned candidate, source and keyed completion.
The abandonment regression must retrieve the exact completion and explicitly abort/close the
candidate, not merely observe that the task ended. The enclosing recovery owner must retain this
receiver through settlement; connecting that owner remains in phase 744. No change to the app
catalog/composer recovery contract is required.
