# Goals

Define Beryl-home-wide content-addressed image storage, typed references, durable sidecar ordering, collision resistance, and Host/WSL runtime projection.

Allow many drafts and turns to share exact bytes without making a thread directory, clipboard payload, runtime path, or CAS record authoritative.

## Non-goals

- Defining composer or transcript layout.
- Owning per-thread label-allocation policy.
- Garbage-collecting unreferenced assets.
- Creating runtime staging copies for submitted input assets.

# Decisions

## Runtime Resource Boundary

- `doc/systems/bounded-resource-dataflow/design.md` owns practical stream-page, file-range, decode,
  thumbnail, tile, pending-upload, CPU-cache, and GPU-cache limits. Durable asset identity and
  sidecar ownership remain here.
- Asset byte length is not permission to decode or upload an arbitrary full-resolution raster.
  Consumers use bounded reads where practical and explicit whole-operation, pixel, frame, decode,
  upload, and cache limits where incremental processing is unavailable.

## Asset Identity And Records

- Asset identity is a versioned SHA-256 content digest plus byte length; metadata also stores media type, dimensions when validated, creation revision, and sidecar state.
- A digest collision with different bytes or length is a hard invariant failure. Beryl never overwrites one final sidecar with different content.
- Asset ids are opaque at feature boundaries even when their storage representation contains a digest.
- Every durable marker has one stable marker id and one final per-thread label ordinal before it enters a draft. Syndic retains that marker identity and label in exact input order.
- Durable ownership uses one compact owner head selecting an immutable paged marker-reference set
  for a current draft, accepted input, submitted turn item, retry record when distinct, or
  transcript projection. Provider-generated items may select an equivalent resource-owned set.
  Transient clipboard tokens remain non-durable.
- Queueing, steering, retryable delivery, and terminal delivery-unknown states of one accepted
  input do not create new asset owners. Its accepted-input marker reference survives those
  disposition and lifecycle changes unchanged. Promotion into a fresh pending ordinary turn is
  different: the atomic cross-domain command moves that same sealed reference-set head from the
  accepted-input owner to the exact fresh submitted-item owner.
- One accepted input or submitted turn item may own any representable number of image references in
  bounded set pages. Repeating one marker occurrence retains one marker identity and one asset
  identity rather than relying on owner-kind cardinality.
- Reference-set pages are staged and sealed before publication. The same final home-store commit as
  the owning durable record swaps only compact owner heads and validates matching set identity,
  count, frontier, and digest.
- Every sealed manifest, entry-page, and label-first read is authorized by the complete opaque
  sealed-set proof and rechecks exact manifest agreement. A bare set identity, or a different proof
  that merely names the same set, cannot select durable reference data.

## Physical Byte Storage

- One Beryl home owns one content-addressed image sidecar namespace shared by every thread and reference in that home.
- Original and provider-generated image bytes are ordinary digest-addressed sidecar files under the
  Beryl home. They are never stored as Fjall values or blob payloads.
- Fjall stores typed asset metadata, durable references, and sidecar admission and availability state; those records point to the authoritative sidecar files.
- Beryl-owned processes never replace or delete a referenced final sidecar. Arbitrary same-user
  filesystem tampering is outside Beryl's correctness contract. Runtime verification retains an
  open handle only while checking bounded length/digest pages and deriving the runtime path; it does
  not retain one handle per logical image through an external consumer's later path access.

## Sidecar Admission

- New bytes stream through bounded pages into sidecar construction and incremental hashing away from
  the GPUI thread. Required format, dimensions, frame count, and other admission metadata come from
  bounded header parsing; admission never decodes the complete raster merely to discover metadata.
- Admission uses the sidecar write, flush, digest-path rename, directory durability, then metadata/reference commit ordering defined by `doc/systems/beryl-home-storage/design.md`.
- Existing matching content reuses the final sidecar after exact length and digest verification.
- A committed reference never points to a temporary path or unverified runtime staging file.
- Orphaned temporary or final sidecars are inert and retained pending future garbage collection.

## Reference And Label Evidence

- Beryl asset records prove byte identity and reference ownership; Syndic input records prove per-thread historical label use.
- Draft-to-turn and draft-to-accepted-input admission streams every marker into one sealed
  owner-neutral reference set. The final command rebinds that set from draft to admitted owner and
  publishes the Syndic input only when content identity/digest, marker-only digest/count, maximum
  label, marker/label/asset order, first-occurrence disposition, frontier, and asset-chain digest
  agree exactly. Build-local durable
  label-first keys avoid an in-memory set even when marker count is arbitrarily large.
- Accepted-input-to-submitted-item promotion performs the corresponding compact owner-head swap
  without rebuilding or scanning the paged set. Marker-free promotion validates both heads absent.
  The current-draft owner head is outside this transition and remains unchanged.
- Starting replacement edit retains the submitted item's immutable reference set and constructs the
  current draft's copy-on-write owner head over that same set. Later edits build replacement sets;
  historical ownership is never moved merely because its turn is being replaced. One Asset-domain
  mutation participant asserts the unchanged historical head and publishes the absent draft head;
  it never composes separate validation and mutation participants for the same domain.
- A Syndic thread owns a compact durable label frontier and immutable origin spans for admissions
  that advanced it. Each span names the admitted owner and sealed set proof; point lookup resolves
  the label through that set's durable label-first index. A child discussion inherits only its
  parent's exact frontier at branch creation and follows immutable thread-lineage authority with
  constant resident state instead of copying the parent label index.
- Label caches contain only the thread identity, exact frontier and revision, plus bounded
  current-draft marker facts. Cache eviction makes label-affecting operations repeat point reads;
  no synchronization path walks complete history or rebuilds a whole used-label set.
- Asset deduplication never causes label reuse across threads and never merges two distinct draft marker identities.

## Generated Output Admission

- Standalone CAS image generation crosses Beryl's ingress boundary through `savedPath`, not through
  the protocol item's base64 `result`. The bounded incoming JSON decoder discards `result` before
  constructing a retained JSON value, normalized backend item, app event, Syndic frame, diagnostic
  payload, or log record. Beryl never decodes, persists, retries from, or falls back to that base64
  field.
- CAS currently still transmits that base64 on the wire alongside `savedPath`; ingress exclusion is
  a containment boundary for the pinned protocol, not a desired media-transfer design. If a future
  compatible CAS contract can emit the filesystem path without the base64 field, Beryl should use
  that path-only contract and remove the now-unnecessary exclusion without changing Syndic or asset
  authority.
- Compatibility admission for exact CAS 0.146.0 requires retained proof of discriminant-first
  serialization; the containment boundary then streams JSON directly from bounded WebSocket payload
  chunks. Reordered or ambiguous target input
  fails closed; it does not authorize whole-field buffering, external spooling, base64 decoding, or
  a second media handoff path.
- A CAS runtime path, hosted URL, protocol item, or CAS historical record is never durable generated
  media authority. Syndic preserves the exact completed provider item lifecycle immediately, while
  its generated-media resource carries a separate pending-asset disposition. Canonical resource
  finalization and history completeness remain behind until Beryl has read the exact produced bytes
  through the owning runtime boundary and admitted them into the Beryl-home sidecar store.
- The normalized standalone item and its Syndic provider frame retain exact item identity,
  lifecycle timestamps, status, optional revised prompt, `savedPath`, and CAS runtime, process,
  thread, turn, item, and loaded-session provenance. They deliberately contain no `result` field.
  A successful output with no nonempty `savedPath` has no supported byte handoff and becomes a typed
  missing generated-media resource; it never re-admits the discarded base64 payload.
- Admission retains the exact CAS thread, turn, item, runtime, process, and loaded-session provenance
  while it reads and validates the output away from GPUI. Host output is read from its exact path;
  WSL output is streamed from the exact selected distribution into host-side admission without
  treating a WSL cache or project root as durable state.
- Sidecar bytes are prepared and made durable first. One typed home-store command then publishes the
  asset metadata/reference, resolves the generated-media resource disposition, and advances only
  canonical resource-finalization and history-complete frontiers. It never rewrites the completed
  provider item lifecycle or terminal status. A partial, missing, changed, unsupported, unreadable,
  or representationally exhausted output keeps the resource pending or unavailable and never
  publishes a path-only transcript asset.
- The admitted CAS image producer is the standalone `image_gen.imagegen` extension. Native hosted
  Responses image generation remains outside the exact 0.146.0 producer contract unless retained
  release-scoped evidence proves that the client can declare and invoke it; parser tolerance alone
  does not create a generated-output admission source.

## Host Runtime Projection

- Host execution uses the canonical host sidecar path after verifying that it still resolves to the admitted length and digest.
- The backend request receives a runtime path only; it does not receive the Beryl asset id or storage-internal sidecar metadata.

## WSL Runtime Projection

- Beryl verifies the canonical Windows sidecar path through the Windows process, then translates an
  ordinary absolute drive path such as `C:\Users\operator\.beryl\...` directly to the selected WSL
  runtime path `/mnt/c/Users/operator/.beryl/...` for `localImage.path`.
- Windows validation paths and WSL backend paths are separate representations of the same
  Beryl-home sidecar. Beryl does not validate the sidecar through a `\\wsl.localhost\...` UNC path.
- UNC, relative, non-drive, or otherwise unmappable Host paths reject projection. Beryl does not
  copy an input asset into WSL, create a per-runtime staging cache, or fall back to a project root.
- WSL projection never changes the thread's configured root and never falls back to a project directory.

## Bounds And Recovery

- Resident paste, path-preparation, and verification queues have deterministic page and slot
  capacity. Logical draft references and asset bytes remain durable and paged; their total count or
  length does not enlarge a queue or impose a Beryl memory-safety cap.
- Sidecar files use bounded range reads or bounded mapping windows only inside admitted decode and
  upload-preparation workers. Mapping the complete original is not a substitute for a range-backed
  source or a resource reservation.
- Compressed formats are decoded only into a preflighted admitted thumbnail surface or visible tile
  set before GPU upload. Unsupported decoders that require a full-original output surface produce a
  local unavailable result instead of allocating that surface and downscaling afterward.
- Restart reconstructs durable references from the home store and re-verifies each canonical sidecar
  before deriving a new Host or WSL request path.
- Missing or corrupt authoritative sidecars produce typed asset-unavailable failures and keep their durable references for diagnosis or later recovery.
- Reference removal updates metadata only. No durable image-sidecar bytes are deleted before a future reachability-aware Collect Garbage system is accepted.
