# Goals

Define Beryl-home-wide content-addressed image storage, typed references, durable sidecar ordering, collision resistance, and Host/WSL runtime projection.

Allow many drafts and turns to share exact bytes without making a thread directory, clipboard payload, runtime staging path, or CAS record authoritative.

## Non-goals

- Defining composer or transcript layout.
- Owning per-thread label-allocation policy.
- Garbage-collecting unreferenced assets.
- Treating runtime staging copies as durable Beryl state.

# Decisions

## Asset Identity And Records

- Asset identity is a versioned SHA-256 content digest plus byte length; metadata also stores media type, dimensions when validated, creation revision, and sidecar state.
- A digest collision with different bytes or length is a hard invariant failure. Beryl never overwrites one final sidecar with different content.
- Asset ids are opaque at feature boundaries even when their storage representation contains a digest.
- Every durable marker has one stable marker id and one final per-thread label ordinal before it enters a draft. Syndic retains that marker identity and label in exact input order.
- Typed references identify one exact owner: current draft plus marker, accepted input plus marker,
  submitted turn item plus marker, provider-generated turn item, retry record plus marker when a
  distinct retry snapshot owns bytes, transcript projection, or transient clipboard token.
- Queueing, steering, retryable delivery, and terminal delivery-unknown states of one accepted input do not create new asset owners. Its accepted-input marker reference survives those disposition and lifecycle changes unchanged.
- One accepted input or submitted turn item may own many image references because marker identity is part of the reference key. Repeating one marker occurrence retains one marker identity and one asset identity rather than relying on owner-kind cardinality.
- Reference mutations participate in the same home-store commit as the owning durable record.

## Physical Byte Storage

- One Beryl home owns one content-addressed image sidecar namespace shared by every thread and reference in that home.
- Original and provider-generated image bytes are ordinary digest-addressed sidecar files under the
  Beryl home. They are never stored as Fjall values or blob payloads.
- Fjall stores typed asset metadata, durable references, and sidecar admission and availability state; those records point to the authoritative sidecar files.

## Sidecar Admission

- New bytes are hashed and decoded for required metadata away from the GPUI thread under deterministic input limits.
- Admission uses the sidecar write, flush, digest-path rename, directory durability, then metadata/reference commit ordering defined by `doc/systems/beryl-home-storage/design.md`.
- Existing matching content reuses the final sidecar after exact length and digest verification.
- A committed reference never points to a temporary path or unverified runtime staging file.
- Orphaned temporary or final sidecars are inert and retained pending future garbage collection.

## Reference And Label Evidence

- Beryl asset records prove byte identity and reference ownership; Syndic input records prove per-thread historical label use.
- Draft-to-turn and draft-to-accepted-input admission resolves every marker to its exact asset id and moves all affected per-marker references in the same home-store command that consumes the draft. The admitted Syndic payload and asset-reference moves must agree exactly in marker identity, label, asset identity, count, and order before admission.
- Starting replacement edit verifies every target marker against its existing submitted-turn-item reference, retains that historical reference, and adds a separate current-draft-marker reference in the same home-store command that copies the immutable Syndic payload. The historical owner is never moved merely because its turn is being replaced.
- Label caches contain only acceleration facts and a validated Syndic frontier. Cache eviction makes label-affecting operations wait for bounded Syndic validation again.
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
- The containment boundary pins official CAS 0.144.1's proven discriminant-first serialization and
  streams JSON directly from bounded WebSocket payload chunks. Reordered or ambiguous target input
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
  provider item lifecycle or terminal status. A partial, missing, changed, oversized, unsupported,
  or unreadable output keeps the resource pending or unavailable and never publishes a path-only
  transcript asset.
- The CAS 0.144.1 admitted producer is the standalone `image_gen.imagegen` extension. Native hosted
  Responses image generation is outside that pinned producer contract; parser tolerance alone does
  not create a generated-output admission source.

## Host Runtime Projection

- Host execution uses the canonical host sidecar path after verifying that it still resolves to the admitted length and digest.
- The backend request receives a runtime path only; it does not receive the Beryl asset id or storage-internal sidecar metadata.

## WSL Runtime Projection

- Beryl first asks the selected WSL runtime to resolve and read a mapped path for the host sidecar, then verifies exact length and digest inside that distribution.
- If the host path is not directly readable, Beryl copies the bytes to a bounded per-runtime staging cache under the neutral WSL Beryl directory using the digest-derived filename, then verifies the staged length and digest before use.
- Staging writes use temporary-file plus atomic-rename ordering inside WSL.
- Staging cache entries may be recreated and evicted because the Beryl-home sidecar remains authoritative.
- WSL projection never changes the thread's configured root and never falls back to a project directory.

## Bounds And Recovery

- Paste admission, retained draft references, transient clipboard payloads, path-preparation workers, staging caches, and verification queues have deterministic count and byte limits.
- Sidecar files may be OS memory-mapped only within those bounded decode and upload-preparation workers.
- Memory mapping does not bypass image decoding: compressed image formats are decoded into uploadable pixel data before GPU texture upload.
- Restart reconstructs durable references from the home store and treats all staging paths as absent until reverified.
- Missing or corrupt authoritative sidecars produce typed asset-unavailable failures and keep their durable references for diagnosis or later recovery.
- Reference removal updates metadata only. No durable image-sidecar bytes are deleted before a future reachability-aware Collect Garbage system is accepted.
