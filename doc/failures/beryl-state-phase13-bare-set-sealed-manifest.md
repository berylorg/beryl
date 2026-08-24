# Bare Set Identity Cannot Authorize A Sealed Manifest Read

## Context

Phase 13 replaced per-marker asset ownership with immutable paged reference sets and required the
complete opaque `SealedAssetReferenceSetProof` for reads of sealed set authority.

## Invalidated Approach

The first cut proof-gated entry pages, marker lookups, and label-first lookups, but retained a public
`reference_set_manifest` read selected only by `AssetReferenceSetId`. Once a build was sealed, that
bare-id read returned the sealed manifest and let a caller recover the very proof that sealed reads
were meant to require.

Tests used the bare-id API to discover sealed proofs, so their success did not establish that proof
authority was unforgeable at the public read boundary.

## Why It Failed

A set identity names storage; it does not prove the exact sealed content, marker summary, label
frontier, entry frontier, or chain digests. Allowing a bare identity to return a sealed manifest
collapses selection and authorization into the same read and bypasses the proof-bound contract.

## Accepted Correction

Ordinary sealed-manifest reads require the complete opaque proof and recheck exact manifest
agreement before returning data. Unsealed construction inspection uses a separately typed staging/
build capability. Type opacity alone is insufficient: if a public command can remint that value
from the set identity, it remains a bare-id proof-discovery path. The accepted capability therefore
binds the set identity to a caller-retained 32-byte secret while storage persists only its
commitment.

One distinct exact-completion read may use that original capability only when it also receives both
expected final marker summaries and revalidates a separate compact completion-evidence record that
commits lifecycle, summaries, frontier, local chain, and final proof. It returns the opaque proof
only for that exact sealed completion. Capability alone still cannot select sealed state, and there
is no bare-id compatibility overload or same-set remint path.

The correction must cover the same-set-id/different-full-proof case for the manifest itself, reject
newly minted same-set capabilities, staging-only or wrong-summary completion reads, and corrupted
manifest/evidence disagreement, remove tests that recover sealed authority through a bare id, and
keep construction and completion reconciliation available only through the original typed build
capability.
