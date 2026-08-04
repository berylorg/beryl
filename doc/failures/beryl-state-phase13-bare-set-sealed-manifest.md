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

Sealed-manifest reads require the complete opaque proof and recheck exact manifest agreement before
returning data. Unsealed construction inspection uses a separately typed staging/build authority
whose capability cannot select sealed state. There is no bare-id compatibility overload.

The correction must cover the same-set-id/different-full-proof case for the manifest itself, remove
tests that recover sealed authority through a bare id, and keep staging reconciliation available
only through the typed build boundary.
