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

- `doc/systems/bounded-resource-dataflow/design.md` owns the shared bounded-resource invariant and
  enforcement model. This system owns the exact image decoder, thumbnail/tile surface,
  pending-upload, GPU-texture, and window-pin configuration. Durable asset identity and sidecar
  ownership remain here.
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
- Existing matching content reuses the final sidecar only after exact length and digest
  verification plus a bounded page-by-page byte-for-byte comparison against the staged source.
  Any differing byte is the hard collision invariant failure and publishes no metadata or
  reference.
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
- Label caches contain only the thread identity, exact frontier and revision, plus bounded resident
  pages or query results from durable range-indexed current-draft marker metadata. Cache eviction
  makes label-affecting operations repeat bounded point or range reads; no synchronization path
  loads the whole current-draft marker collection, walks complete history, or rebuilds a whole
  used-label set.
- Asset deduplication never causes label reuse across threads and never merges two distinct draft marker identities.

## Pinned-Release Generated Output Admission

- The admitted producer contract is the standalone `image_gen.imagegen` extension in exact CAS
  0.146.0. Native hosted Responses image generation is not an admitted source, and parser tolerance
  for another item shape cannot create a generated-output source.
- Generated output crosses Beryl's ingress boundary through `savedPath`, not through the protocol
  item's base64 `result`. The bounded incoming JSON decoder structurally consumes and discards
  `result` before constructing a retained JSON value, normalized backend item, app event, Syndic
  frame, diagnostic payload, or log record. Beryl never decodes, persists, retries from, or falls
  back to that field.
- Generated-output release admission requires exact runtime admission for CAS 0.146.0 plus retained
  release-scoped proof of the target item's discriminant-first serialization. The containment
  boundary streams JSON directly from bounded WebSocket payload chunks. Reordered or ambiguous
  target input fails closed; it does not authorize whole-field buffering, external spooling, base64
  decoding, or a second media handoff path.
- A CAS runtime path, hosted URL, protocol item, or CAS historical record is never durable generated
  media authority. Syndic publishes the completed provider-item lifecycle independently, while the
  generated-media resource starts in `pending`. Canonical resource finalization and history
  completeness remain behind until the exact produced bytes are admitted into the Beryl-home
  sidecar store.
- The normalized standalone item and its Syndic provider frame retain exact item identity,
  lifecycle timestamps, status, optional revised prompt, `savedPath`, and CAS runtime, process,
  thread, turn, item, and loaded-session provenance. They deliberately contain no `result` field.
  A successful output with no nonempty `savedPath` has no supported byte handoff and terminalizes
  the resource as `unavailable`; it never re-admits the discarded base64 payload.
- Admission retains the exact CAS thread, turn, item, runtime, process, and loaded-session provenance
  while it reads and validates the output away from GPUI. Host output is read from its exact path;
  WSL output is streamed from the exact selected distribution into host-side admission without
  treating a WSL cache or project root as durable state.
- Sidecar bytes are prepared and made durable first. One typed home-store command then publishes the
  asset metadata/reference, changes that same generated-media resource from `pending` to `admitted`,
  and advances only canonical resource-finalization and history-complete frontiers. It never
  rewrites the completed provider item lifecycle or terminal status.
- The resource remains `pending` only while its exact authenticated read, durable admission, or
  authorized recovery is still in progress. Missing, changed, unsupported, unreadable,
  unauthenticated, representationally exhausted, or otherwise unrecoverable output terminalizes the
  same resource as `unavailable` and never publishes a path-only transcript asset.
- `beryl-app` owns one generated-media finalization coordinator for each published Beryl service.
  It is keyed by durable generated-media resource identity, admits at most one flight for an
  identity, and uses the configured `generated_media_queue_items`,
  `generated_media_worker_slots`, and `generated_media_page_bytes` capacities. A window,
  transcript host, connection, broker, CAS item handler, or request waiter never owns the
  finalization lifetime. Joining an existing flight reserves no second queue item, worker, or page.
- Admitting a coordinator task reserves one queue item. Scheduling it transfers that reservation
  into one worker slot; each source read then reserves at most one bounded page before transferring
  the page to sidecar admission or releasing it after consumption. `admitted`, `unavailable`,
  unrecoverable failure, supersession, coordinator cancellation, and service disposal release every
  queue, worker, and page reservation owned by that flight. Saturation creates no process-local
  task or reservation; the durable pending identity remains fairly eligible for a later slot while
  the service is current.
- Caller or window cancellation releases only that caller's interest. Service retirement cancels
  and joins every coordinator flight and releases all of its queue, worker, page, and byte
  capacity without erasing the durable `pending` resource or its durable admission evidence. A
  replacement service constructs a fresh coordinator and reconstructs work only from that durable
  state; no old-generation path handle, task, or authenticated session crosses the boundary.
- Restart or replacement resumes a `pending` resource only when the exact recorded runtime,
  process, loaded-session, thread, turn, and item provenance can still authenticate the source, or
  when durable sidecar-admission evidence proves that the exact bytes already crossed into Beryl
  authority. Otherwise it terminalizes the same resource as `unavailable`; a recreated session,
  equal-looking path, similar file, or process-local observation is not recovery evidence.
- Every admitted coordinator flight and every reconstructed `pending` resource converges to
  `admitted` or `unavailable` under its bounded lifecycle. Disposal, restart, saturation, repeated
  cancellation, or lost producer authority cannot leave a resource indefinitely `pending`.
- A terminal-turn repair snapshot uses the same base64 exclusion and `savedPath`-only handoff.
  Beryl begins the authenticated runtime read promptly after validating the complete snapshot so a
  temporary producer path is not treated as durable availability.
- Repair admission binds the read to the exact snapshot, CAS release, runtime/process, thread,
  turn, and item provenance. Missing, empty, changed, unreadable, unsupported, oversized, or
  unauthenticated path or bytes invalidates the whole repair candidate. All repair-derived asset
  metadata, references, and canonical resource dispositions remain unpublished; a usable sibling
  item never becomes a partial repaired asset.
- Preparing repair media first publishes only an inert Beryl-state staging record through a bounded
  cross-domain stage command with the matching noncanonical Syndic media witness. That record
  consumes the current-generation `AdmittedSidecar`, retains the target turn/item natural identity,
  asset digest/length, media facts, and authenticated repair provenance, and is unreachable from
  every ordinary asset, resource, transcript, history, and projection read. It is not canonical
  asset metadata, a durable reference, or a resource disposition.
- The CAS-live Syndic transcript system owns whole-turn repair convergence and `beryl-app` owns its
  process coordinator. One final `HomeCommand` combines the Beryl-state promotion participant with
  Syndic seal-and-selection. The Beryl participant revalidates every inert staging record and final
  sidecar before publishing exact asset metadata, references, and resource dispositions; the Syndic
  participant validates the identical media commitment, selects the whole snapshot, and enters
  `FinalizingHistory`. Neither participant can commit alone.
- A typed media failure or explicit incomplete convergence publishes no repair-derived asset or
  snapshot. Prepared sidecars and staging records remain inert and unreachable for future home-wide
  garbage collection. A fresh service may finish only an already complete durable staged candidate;
  it never rereads CAS or trusts a recreated runtime path to fill a missing stage.
- Inline base64, a hosted URL, a similar filesystem entry, prior transient bytes, and CAS history
  outside the exact repair snapshot are never fallback media authority.

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

## Media Working-Set Configuration

- Process startup constructs one immutable validated media-resource configuration with exact
  positive values for `generated_media_queue_items`, `generated_media_worker_slots`,
  `generated_media_page_bytes`, `paste_admission_queue_items`, `path_preparation_slots`,
  `sidecar_verification_slots`, `sidecar_io_page_bytes`, `decoder_slots`, `decoder_queue_items`, `surface_max_bytes`,
  `cpu_surface_entries`, `cpu_surface_bytes`, `upload_staging_buffers`, `upload_staging_bytes`,
  `gpu_texture_entries`, `gpu_texture_bytes`, `window_pin_entries`, and `window_pin_gpu_bytes`.
  Startup fails rather than running with an absent, zero, inconsistent, or arithmetic-overflowing
  capacity.
- A waiting decode reserves one of `decoder_queue_items`. When scheduled, it releases that queue
  reservation and reserves one of exactly `decoder_slots`, one CPU surface entry, and the complete
  bounded output against `surface_max_bytes` and `cpu_surface_bytes` before decode begins. Visible
  demand takes priority over preload.
- The shared CPU thumbnail/tile cache admits only when both `cpu_surface_entries` and
  `cpu_surface_bytes` remain satisfied. Each reserved decoder output counts as one entry, and its
  allocation capacity counts as bytes; promotion to a resident decoded surface transfers rather
  than duplicates that accounting.
- Decode cancellation or failure releases the queue item or decoder slot and the complete reserved
  surface accounting. Successful decode releases the decoder slot and transfers the existing
  surface reservation into the CPU cache; it does not add a second entry or byte charge.
- Upload staging admits only after reserving one of `upload_staging_buffers` and the exact staged
  byte count within `upload_staging_bytes`. Completion, cancellation, texture-admission failure, or
  device loss releases both reservations immediately.
- Beryl-owned image textures use one process-wide cache that admits only while both
  `gpu_texture_entries` and `gpu_texture_bytes` remain satisfied. Driver-private copies,
  swap-chain resources, paths, and glyph atlases are outside that estimate and never justify
  exceeding either Beryl-owned cap.
- CPU surface and upload-staging byte accounting uses owned allocation capacity. GPU texture byte
  accounting uses the checked sum of each Beryl texture's allocated extents, layers, mip levels,
  and format bytes per texel; an in-flight reserved texture counts before upload begins.
- Successful upload transfers its in-flight texture reservation into the GPU cache. Upload
  cancellation, failure, supersession, or device-generation mismatch releases that reservation
  and its staging reservation without publishing a texture entry.
- Each editor or transcript window may pin at most `window_pin_entries` shared image renditions and
  `window_pin_gpu_bytes` of their GPU texture estimates. Pins are references to process-wide cache
  entries rather than per-window byte copies. A pin that cannot satisfy the per-window and process
  caps yields the owning local unavailable state; it does not evict another window's pinned visible
  resource or expand a cap.
- CPU and GPU caches evict least-recently-used unpinned entries until both their item and byte caps
  admit new work. Leaving the realized editor/transcript range, replacing its revision, switching
  threads, closing a window, cancelling a request, or removing a preview releases the
  corresponding pin. An unpinned entry may remain only within its cache caps.
- GPU device loss increments a device generation, cancels old-generation uploads, releases all
  upload staging, drops every old-generation texture, and preserves only admitted CPU renditions
  within the CPU cache. After device recovery, currently visible pins request lazy re-upload under
  the same staging and GPU caps; no durable asset or transcript state changes and no full-history
  or full-asset preload runs.
- Media diagnostics and repetition proofs are content-free. They may expose configuration,
  generation, aggregate counts and byte estimates, high-water marks, waits, denials, cancellations,
  releases, and evictions, but never asset ids, digests, labels, paths, prompts, thread/turn/item
  identities, source bytes, decoded pixels, or excerpts. Repetition verification retains only
  aggregate counter evidence that load, range change, window close, cancellation, and device-loss
  cycles return resources to baseline or a bounded cache plateau.

## Bounds And Recovery

- Paste admission reserves one `paste_admission_queue_items` entry before retaining source demand.
  Path preparation and sidecar verification reserve one `path_preparation_slots` or
  `sidecar_verification_slots` entry before opening their source, and each owns at most one
  `sidecar_io_page_bytes` page at a time. Promotion transfers the existing queue or slot ownership
  to the next stage rather than duplicating it. Success, typed unavailability, rejection,
  cancellation, failure, supersession, service disposal, and source exhaustion release every
  entry, slot, page, handle, and staged buffer owned by the operation. Saturation applies bounded
  backpressure or the owning typed unavailable result and retains no uncharged work.
- Logical draft references and asset bytes remain durable and paged; their total count or length
  does not enlarge a queue or impose a Beryl memory-safety cap.
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

# Engineering Rigor

Profile: `production-application/v1`

Modifiers: none

Asset bytes, media metadata, and generated paths are untrusted at admission. Arbitrary same-user
mutation of final sidecars after admission remains outside the correctness contract.
