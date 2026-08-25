# Goals

Provide shared pure-data identities and values used across Beryl packages without pulling in GUI, storage-engine, process, or protocol implementation.

## Non-goals

- Owning process launch, transport I/O, protocol parsing, or CAS requests.
- Owning GPUI rendering types or window lifecycle logic.
- Owning Fjall keyspaces, encodings, transactions, revision sequencing, or persistence workers.
- Owning Syndic turn, draft, item, projection, or CAS-binding record schemas.
- Owning product workflows or cross-package orchestration policy.

# Decisions

## Purity

- This crate must not depend on `gpui`, Tokio, Fjall, filesystem I/O, transport, or process-management APIs.
- Types are serializable only when a consuming boundary needs a stable value shape; serialization does not make this crate the owner of a stored schema.
- Pure validation may reject malformed identities, paths, bounds, and state combinations without performing availability probes or storage reads.

## Beryl Home And Window Identities

- `BerylHomeId` is an opaque stable identity derived and persisted by the home-store boundary; this crate does not derive it from user-facing path text.
- `WindowId` is an opaque stable identity for one restorable main conversation window.
- Pure session values may represent window geometry, monitor/work-area hints, virtual-desktop identity, selected Syndic thread id, and remembered runtime/root identity without owning restore policy.

## Runtime And Root Values

- `RuntimeId` is an opaque Beryl identity for one configured canonical Codex CLI executable.
- `RuntimeMode` preserves the Host or exact WSL-distribution environment derived from that executable path.
- Pure runtime values may carry canonical host-visible and runtime-native executable paths without probing them or deriving environment identity inside this crate.
- `RootId` is an opaque configured-root identity owned by one runtime.
- `ExecutionBinding` contains exact runtime identity, configured root identity, and canonical runtime-
  native root path used by one Syndic thread. `syndic-storage` owns its immutable persisted record;
  this crate owns only the shared pure value.
- Availability is represented as explicit observed state with bounded reason categories. Pure types never probe a path, WSL distribution, executable, or CAS process.

## Thread And Presentation Values

- Stable Syndic thread, turn, draft, draft-marker, content, item, accepted-input, retry-record, transcript-projection, resource, and execution-snapshot identities may cross package boundaries without making this crate the owner of their stored record schemas.
- `SyndicContentId` is the stable 128-bit lookup identity derived from an exact chunk-chain digest. `SyndicContentDigest` retains the complete 256-bit comparison authority so a truncated-identity collision is rejected rather than aliased.
- Idle submission preserves one exact 128-bit identity payload while changing its typed identity
  from draft to submitted turn. Queueing preserves the accepted-input identity and therefore has
  no separate queued-input identity type. Later promotion retains that accepted identity as
  history but uses caller-supplied fresh turn and canonical-item identities; its terminal Syndic
  witness links the distinct predecessor and successor identities.
- `FirstAcceptancePromotionSuccessorV1` is the fixed-size pure correlation used only when an
  indeterminate first-acceptance command is reconciled after valid promotion. It carries the exact
  accepted-input identity, promoted submitted-turn-item owner identity, and complete compact sealed-
  asset-set proof needed by both domain owners. It carries no storage key, hook, reader, receipt,
  reconciliation handle, or authority; Syndic authenticates it from permanent acceptance and
  promotion records, while Beryl-state only validates its Asset-owned witness.
- Shared values may identify a Syndic thread, its execution binding, generated title, automatic
  branch-discussion archive state, exact usage observation, activity timestamp, parent-thread
  lineage summary, current Beryl window claim, and catalog availability without making this crate
  the persisted owner.
- Thread-summary title-source values form a closed tagged set for generated, history-derived, or
  absent sources; no untagged backend-name or catalog-row variant exists. Bounded catalog values
  carry only their declared pure fields and confer no persistence or precedence authority.
- The [conversation-threads feature](../../../doc/features/conversation-threads/design.md) owns title
  behavior, while the [Syndic conversation-history system](../../../doc/systems/syndic-conversation-history/design.md)
  owns title derivation and persistence.

## Revision And Command Values

- Opaque revision values identify home, domain, thread, draft, chunked-content manifest, accepted-input, per-thread input gate, projection, binding, claim, session, and job revisions without exposing storage-engine compare-and-swap primitives.
- `CasNativeTurnCount` is the zero-capable exact number of actual CAS model turns represented by a
  loaded thread prefix. Its checked increment and ordering are independent of Syndic
  conversation-DAG depth.
- Typed expected-revision sets and conflict reports may be shared across packages.
- Idempotency identities remain distinct from user-facing ids and CAS ids.
- Resolution-intent identity remains distinct from its derived durable handoff-job identity and from the external tool-call identity that admitted it.

## Provenance

- Shared provenance values distinguish user-authored input, Beryl-generated handoff input, CAS live events, dynamic tool calls, and durable recovery actions.
- Dynamic tool-call provenance may store exact app-server thread id, turn id, tool name, and tool-call id as opaque external identities.
- Provenance values must not contain authentication material, capability tokens, hidden developer instructions, or unbounded payload text.
- `CasThreadId`, `CasTurnId`, and `CasItemId` are opaque nonempty valid-UTF-8 identities whose
  exact encoded value is at most 256 bytes. Construction and deserialization reject a larger value;
  they never truncate, normalize, hash, or substitute it. This is only the pure identity contract
  delegated by `syndic-storage`, not ownership of CAS correlation, repair, or stored-record policy.
- Exact CAS item ids, managed-process generations, loaded-thread generations, and distinct discussion-context, selected-path, and recovery-sequence digest domains may cross the backend, Syndic, and orchestration boundaries without owning provider calls or stored proof records.
- The recovery-sequence digest domain exposes one pure incremental accumulator shared by Syndic
  preflight and backend replay. It covers declared item and UTF-8 totals plus each exact one-based
  ordinal, closed user/input-text or assistant/output-text role, declared item length, and streamed
  bytes while retaining only SHA-256 state and compact counters; it owns no recovery policy,
  storage cursor, transport source, or durable proof record.

## Asset Identity

- `AssetId` is a pure, versioned content identity composed of SHA-256 digest bytes and exact nonzero byte length.
- `ImageLabelOrdinal` is the shared nonzero `u64` value for one final per-thread image label. It
  provides canonical bijective-letter presentation without owning label allocation, thread
  frontiers, origin evidence, marker references, or durable encoding.
- `SequentialMarkerSummaryV1` is the separate content-neutral pure value holding the established
  sequential SHA-256-style digest, exact count, and optional maximum label over exact ordered
  marker-id/label pairs.
- `OrderedMarkerAssetSummaryV1` is the separate pure value holding an independently domain-
  separated ordered digest and exact count over marker id, label, and the complete versioned
  `AssetId`. It is shared only as cross-domain evidence between an opaque Syndic draft-marker seal
  proof and an opaque sealed Asset reference-set proof.
- `SealedContentMarkerSummary` binds exact Syndic content identity and full digest to one embedded
  `SequentialMarkerSummaryV1`. It remains the content-bound summary used by sealed `ComposerV1`; it
  is not a draft-tree root commitment, marker/asset association summary, or Asset-set identity.
- `DraftMarkerCommitmentV1` is a separate versioned pure value containing one marker-only tree-root
  digest, exact count, and optional maximum label. It is compact evidence, not persistence or
  publication authority. Its digest commits the exact persisted marker-id/label/asset tree
  structure, so separately shaped trees with the same ordered marker semantics need not have equal
  commitments.
- `AssetReferenceSetId` and `AssetReferenceSetDigest` are shared pure values.
  `SealedAssetReferenceSetProof` is opaque sealed evidence issued by the Asset owner and adds exact
  set identity, entry frontier, Asset-set-local chain digest, and
  `OrderedMarkerAssetSummaryV1` to one `SequentialMarkerSummaryV1`; all three counts must agree. It
  binds no Syndic content identity or full content digest.
  A summary, commitment, set identity, or digest alone authorizes no staging, owner mutation,
  storage read, path access, or byte access.
- Asset identity exposes only its version, digest, and length as pure identity fields; it encodes no
  consumer-specific presentation, persistence, or sidecar policy.
- The type owns no filesystem path, media metadata, reference record, sidecar operation, or garbage-collection policy.

# Engineering Rigor

Profile: `trusted-internal-tool/v1`

Modifiers:

- `untrusted-input/v1`
