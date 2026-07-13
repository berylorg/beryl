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
- Typed references identify their owner kind and id: current draft marker, accepted input, submitted turn item, queued input, retry record, transcript projection, or transient clipboard token.
- Reference mutations participate in the same home-store commit as the owning durable record.

## Physical Byte Storage

- One Beryl home owns one content-addressed image sidecar namespace shared by every thread and reference in that home.
- Original image bytes are ordinary digest-addressed sidecar files under the Beryl home. They are never stored as Fjall values or blob payloads.
- Fjall stores typed asset metadata, durable references, and sidecar admission and availability state; those records point to the authoritative sidecar files.

## Sidecar Admission

- New bytes are hashed and decoded for required metadata away from the GPUI thread under deterministic input limits.
- Admission uses the sidecar write, flush, digest-path rename, directory durability, then metadata/reference commit ordering defined by `doc/systems/beryl-home-storage/design.md`.
- Existing matching content reuses the final sidecar after exact length and digest verification.
- A committed reference never points to a temporary path or unverified runtime staging file.
- Orphaned temporary or final sidecars are inert and retained pending future garbage collection.

## Reference And Label Evidence

- Beryl asset records prove byte identity and reference ownership; Syndic input records prove per-thread historical label use.
- Label caches contain only acceleration facts and a validated Syndic frontier. Cache eviction makes label-affecting operations wait for bounded Syndic validation again.
- Asset deduplication never causes label reuse across threads and never merges two distinct draft marker identities.

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
