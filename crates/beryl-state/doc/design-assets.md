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
  facts, supplied either by current-generation admission or the selected-repair proof below for
  bytes already durably admitted. A crash may leave unreferenced metadata or a sealed set, but no
  head selects missing bytes.
- Sealed manifest, entry-page, and label-first reads require the complete opaque
  `SealedAssetReferenceSetProof`, never a caller-supplied set id. Each read rechecks the proof's
  exact sequential marker, frontier, and digest authority; a different proof for the same identity
  is rejected. Unsealed inspection has separate build authority.
- First acceptance admits the optional Asset transfer witness for the fixed
  `FirstAcceptancePromotionSuccessorV1` successor shape. Its private static adapter retains only
  the admitted draft identity, accepted-input identity, and sealed-set proof. After Syndic
  authenticates its correlation, the adapter derives exactly three typed point checks for
  HomeStore to execute: absent original CurrentDraft and AcceptedInput heads, and the exact
  SubmittedTurnItem head over the same sealed set at its initial revision. Wrong, duplicate,
  restored, retained, missing, or conflicting state is `Collision`; stored access failures retain
  typed provenance. Marker-free first acceptance has no mutable Asset reconciliation descriptor
  or synthetic mutation and admits the source-only shape explicitly.
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

- Repair storage has one compact build head, immutable pages, a sealed manifest, and one optional
  visibility selector under the existing target thread/turn natural identity. Pages use a
  package-private build identity and consecutive one-based ordinal; entries retain exact target
  item/resource ordinal, `AssetId`, metadata, admitted-sidecar evidence, authenticated provenance,
  and direct page/entry identity. No new shared repair identity or ordinary AssetId index is created.
- Each stage participant consumes matching current-generation `AdmittedSidecar` evidence and
  contributes one immutable page plus its build-head successor to the same command as the matching
  Syndic witness. Reads and contributions have checked fixed record and encoded-byte bounds; one
  page has at most 256 entries and 65,536 encoded bytes. Head totals include all entries/pages and
  exact bytes, and an ordered commitment binds every entry including its locator. Exact replay
  compares only the bounded page/head closure; gaps, duplicate resource occurrences, changed bytes,
  or frontier disagreement reject. Entries follow strictly increasing snapshot item ordinal then
  resource ordinal, checked against the prior head's last occurrence and the matching Syndic
  witness, so duplicate detection requires no set-wide scan. Distinct occurrences may share one
  asset identity.
- The head records the declared complete media frontier and advances from its exact predecessor;
  a seal requires EOF, matching expected totals and ordered commitment, and the complete committed
  frontier authenticated by the complete-response Syndic witness. An intermediate page frontier
  alone cannot declare completion. The immutable sealed manifest binds that head and its page-chain
  commitment. Empty
  media seals the canonical zero-count commitment with no page. Neither sealing nor publication
  traverses pages or reopens sidecars: durable stage evidence proves completed admission within the
  trusted-home contract. Byte acquisition, hashing, and flushing occur before stage writer admission.
- The sole repair-publication participant validates the exact sealed manifest, current selector
  expectation, target, revision, and matching system-supplied commitment and writes only one compact
  visibility selector in the final cross-domain command. It publishes those immutable pages as
  repair-owned metadata/references without copying entries into ordinary Asset records or owner
  heads. It owns no Syndic resource disposition. Its read/write maxima, receipt, and natural-record
  reconciliation closure are fixed independently of media count and byte length; it performs no
  sidecar I/O. Neither a sealed manifest alone nor a matching AssetId grants ordinary visibility.
- A selected-repair read accepts the exact target, item/resource ordinal, page/entry locator,
  AssetId, and sealed commitment obtained from current durable resource authority. It point-reads
  the selector and manifest and reads only the named bounded immutable page, validates exact entry
  agreement, and returns an opaque Asset-owned read proof plus bounded metadata. Missing selection,
  wrong owner, entry, identity, or commitment rejects without scanning or falling back. Byte/range
  access consumes that proof through the ordinary admitted sidecar boundary. Caller fields alone
  cannot construct a proof; an unselected manifest is insufficient.
- AssetId-only metadata reads remain confined to independently admitted ordinary Asset records;
  they do not search repair sets or write on read. A later ordinary ownership operation may admit
  the one demanded repaired asset using its exact selected-repair proof through a bounded metadata
  mutation, validating an existing ordinary record or creating the absent record. The mutation
  checks proof generation and selection and preserves exact asset/media agreement; it neither moves
  the repair selector nor promotes siblings. Ordinary attachment/reference APIs then use that
  admitted record. Preview and resource reads need no such mutation.
- Recovery acquires fresh handles and classifies the compact build/seal/selection closure. Only
  an already fully staged candidate may be sealed or selected; missing pages are never filled by
  another provider read. Prior process proofs cannot authorize recovery. Unselected pages and
  sidecars remain inert for future collection, and observed read failures stay typed. Final-command
  reconciliation distinguishes exact absent and exact selected state or collision without scanning
  pages or verifying all sidecar bytes.
- Bounded exhaustive validation checks heads, sealed entries, selected metadata, sidecars, counts,
  digests, and frontiers without constructing a reverse reference set or trusting a mutable count.
  Reconciliation sees only declared natural identities and exact sidecar commitments; it cannot
  create a correlation, scan heads, or convert plausible Asset state into command success.
