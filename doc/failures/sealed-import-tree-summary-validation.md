# Sealed Import Tree Summary Validation

Sealed-input import must prove agreement between the historical source and the completed draft
trees, in addition to mutual agreement among sequence, identity-index and marker-order trees.

During 2026-10-13 implementation, cross-validation retained the source-EOF marker folds while
checking each tree occurrence. Three coherently substituted trees with matching counts and maximum
label could therefore satisfy those checks without an independently recomputed tree-to-source
commitment. Mutual coherence is insufficient evidence of original source identity.

The corrected [canonical extension](../../crates/syndic-storage/doc/design-schema-v7.md#sealed-import-canonical-extension)
reuses the new import frontier's fold slots by phase. Streaming folds count consumed source
markers. Its EOF transition authenticates the complete folds against the basis summaries and
resets them to canonical seeds. Cross-validation then folds each checked tree marker using the
checked-marker count; completion requires both recomputed tree summaries to equal the basis
content/Asset summaries. Source cursors and counts remain complete. No second source decode,
additional field or ordinary-format change is needed.

Acceptance must reject coherent substituted marker trees and validate canonical phase-transition
fold resets, continued tree folding and complete source agreement.
