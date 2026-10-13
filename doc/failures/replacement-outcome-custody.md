# Replacement Outcome Custody

Historical replacement start spans Syndic and Asset publication. Its outcome must retain the
original complete reconciliation scope, including when classification or exact receipt reads
become unavailable. Consuming an indeterminate outcome and then returning an access error lost
the caller's typed resume path even though the storage registry still held the operation.

The corrected [draft storage contract](../../crates/syndic-storage/doc/design-draft-storage.md#sealed-input-import-and-replacement-intent)
retains bounded evidence per exact prepared command. Clones share that command's observation;
distinct preparations do not. Original indeterminate custody is installed before any fallible
target check, and recovery resumes only that retained scope. Known commit and noncommit facts
also survive failed receipt reads. A foreign target is rejected without discarding original
evidence; no command replay, registry enumeration or replacement scope is permitted.

Review also found that a success-only receipt inspection with no retained observation could
accept a Syndic receipt before an outstanding cross-domain scope was classified. Unobserved
lookup must instead remain unavailable. Receipt presence cannot bypass unresolved Asset
publication, and terminal collision cannot later be promoted to success by receipt lookup.

Acceptance requires unavailable classification followed by same-home recovery and exact resume,
foreign-target rejection followed by original-home resume, and closed collision behavior.
Exercise both marker-free and image-bearing start, plus cancellation and nonselecting import
termination; retain the original outcome through later eligible edits.
