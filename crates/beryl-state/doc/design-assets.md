# Asset state

This supplement is governed by [the `beryl-state` design](design.md) and normatively owns Asset V3
metadata, staged references and owner heads, sidecar verification, proof-gated reads, witnesses,
and repair-media participants.

## Asset V3 and reference ownership

- Asset V3 is the only accepted asset record shape. Earlier forms are rejected; no decoder, dual
  write, importer, or adapter accepts them. An asset record stores AssetId, media facts,
  committed-sidecar state, creation domain revision, and metadata revision. `AssetId` is SHA-256-v1
  digest plus exact nonzero `u64` length; media type is ASCII graphic and at most 127 bytes;
  dimensions are optional and nonzero. Representable-length exhaustion is explicit, not a memory
  ceiling.
- Immutable paged reference sets have a sealed manifest with exact sequential-marker and ordered
  marker/asset summaries, entry frontier, and local chain digest. Ordered entries map marker ids and
  labels to exact asset ids; label-first build keys derive first-occurrence disposition without a
  resident seen-label set. Compact owner heads bind CurrentDraft, AcceptedInput, SubmittedTurnItem,
  RetryRecord when separately snapshotted, or TranscriptProjection to a sealed set and owner
  revision. Queued, steering, delivering, retryable, and delivery-unknown reuse AcceptedInput;
  transient clipboard tokens are never durable references.
- Typed unpublished construction begins with only set identity and canonical empty accumulators;
  bounded pages advance all summaries and a final seal proves final summaries, frontier, and chain.
  The original staging capability binds the set id to a caller-held 32-byte secret while storage
  keeps only its commitment. Completion evidence is maintained atomically and returns building
  state or sealed proof only after lifecycle and commitment revalidation; a bare id, newly minted
  same-id capability, capability alone, or mismatched expected summaries cannot select sealed
  authority.
- A final cross-domain command swaps compact source/destination heads with matching Syndic
  admission, never one record per marker. Changed nonempty draft heads require the sealed proof;
  changed empty removes the exact head after Syndic validates complete empty summaries; unchanged
  nonempty asserts its existing head and proof; marker-free asserts absence. Marker-free work uses
  one duplicate-free validation-only participant: it checks exact optional heads and revision
  fence, makes no write, advances no asset revision, and is absent from affected receipt domains.
  An all-assertion batch is `NoEffect`.
- Next-turn promotion atomically removes the exact AcceptedInput head and publishes the exact fresh
  SubmittedTurnItem head over the same sealed set; marker-free promotion asserts both absent and
  never touches CurrentDraft. Removing any head does not delete metadata or sidecar bytes, prove
  final reachability, or perform eager deletion.

## Sidecars, proofs, and witnesses

- `AssetState` constructs the private `images` content address and verifies exact sidecar length and
  digest by bounded pages, returning file-backed authority without mapping the whole asset. First
  metadata publication requires matching admitted sidecar namespace, digest, length, and media
  facts. A crash may leave unreferenced metadata or a sealed set, but no head selects missing bytes.
- Sealed manifest, entry-page, and label-first reads require the complete opaque
  `SealedAssetReferenceSetProof`, never a caller-supplied set id. Each read rechecks the proof's
  exact sequential marker, frontier, and digest authority; a different proof for the same identity
  is rejected. Unsealed inspection has separate build authority.
- First acceptance reserves the `FirstAcceptancePromotionSuccessorV1` witness role. After Syndic
  authenticates its correlation, it proves the intended CurrentDraft-to-AcceptedInput transfer,
  absent original heads, and the same sealed set on the derived SubmittedTurnItem head; wrong,
  duplicate, restored, retained, missing, or conflicting state is `Collision`. Marker-free first
  acceptance has no mutable reconciliation descriptor or synthetic mutation.
- The private draft-marker readiness witness accepts only a complete bounded accepted-evidence page,
  revalidates selected sealed proofs, manifest, completion evidence, label-first entry, and asset
  metadata on the snapshot, and returns no Asset facts to callers. It neither reads Syndic records
  nor mutates Asset state. It uses protocol `0x53444d5244595631`, operation
  `0x5244595041474531`, and SHA-256 correlation domain
  `syndic/draft-marker-label-readiness-page/v1` over page ordinal, EOF, ordered proofs, labels, and
  AssetIds. The accepted entry is exactly 194 raw bytes: tag `1`; 16-byte sealed-set id; 32-byte
  sequential digest; little-endian `u64` sequential count and maximum label, with zero meaning no
  maximum; 32-byte ordered-asset digest; little-endian `u64` ordered count and entry frontier;
  32-byte asset-chain digest; little-endian `u64` source label; then AssetId version byte, 32-byte
  digest, and nonzero little-endian `u64` length. The cross-domain digest preimage is domain bytes,
  page ordinal as little-endian `u64`, EOF as exactly one byte `0` or `1`, entry count as
  little-endian `u64`, then entries without framing; evidence counts entry bytes and is at most
  65,536 bytes. Repeated identical durable lookups may share reads but every occurrence stays
  separately ordered and hashed.

## Repair participants and validation

- Repair staging records are package-private and keyed by existing Syndic target thread, turn,
  item, resource ordinal, and asset identity. A bounded stage participant consumes the
  current-generation `AdmittedSidecar` and records exact digest/length, media facts, authenticated
  repair provenance, and matching commitment. It publishes no ordinary metadata, head, transcript
  reference, or canonical resource disposition, and ordinary asset APIs cannot select it.
- The sole repair-publication participant runs only in the CAS-live/Syndic final cross-domain home
  command. It verifies every expected stage and final sidecar by bounded pages, then promotes exact
  metadata, references, and resource dispositions. Missing, disagreeing, or unstaged media rejects
  the participant; it cannot publish alone. Failed or incomplete repair leaves staging and prepared
  sidecars inert and unreachable for future home-wide collection.
- Bounded exhaustive validation checks heads, sealed entries, selected metadata, sidecars, counts,
  digests, and frontiers without constructing a reverse reference set or trusting a mutable count.
  Reconciliation sees only declared natural identities and exact sidecar commitments; it cannot
  create a correlation, scan heads, or convert plausible Asset state into command success.
