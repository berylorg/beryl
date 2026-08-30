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
- `SequentialMarkerSummaryV1` remains the content-neutral sequential digest, exact count, and
  optional maximum label over every exact ordered marker-id/label pair. Asset reference sets bind
  that value. The sealed `ComposerV1` summary separately binds exact content identity and full
  digest while embedding the same sequential summary. A draft root instead carries a compact
  structural marker-tree commitment; none substitutes for another or independently authorizes
  publication.
- `OrderedMarkerAssetSummaryV1` is the separate ordered digest and exact count over each marker id,
  label, and complete `AssetId`. Syndic's opaque draft-marker seal proof and Beryl-state's opaque
  sealed reference-set proof both carry it. It is neither content identity nor an Asset-set-local
  chain digest, and sealed `ComposerV1` does not embed it because ComposerV1 contains no asset ids.
- Syndic owns an immutable persistent marker-order commitment tree alongside each draft's composite
  sequence tree and marker-identity index. Every marker in all three structures carries and commits
  its exact marker id, label, and `AssetId`, independent of text positions. Text-only edits reuse
  them unchanged; marker insertion, removal, movement, or replacement path-copies only bounded-
  height paths. Move and same-id replacement preserve byte-equal label and asset identity; changing
  the referenced asset requires a new marker identity. The combined draft root authenticates the
  exact commitment-tree root and compact commitment. This is structural root authority, not a
  promise that independently shaped trees with equal semantics have equal digests.
- A marker-changing draft save captures one exact immutable Syndic root and streams its complete
  marker-order tree through a durable bounded seal cursor. Exact EOF, frontier, count, maximum label,
  commitment, and root/build agreement yields one opaque Syndic seal proof binding that root and
  commitment to both `SequentialMarkerSummaryV1` and `OrderedMarkerAssetSummaryV1`, never to a
  content identity or content-bound summary.
  Failure, cancellation, restart, replay, collision,
  corruption, and supersession remain bounded and cannot publish partial current-draft ownership.
- The app feeds each authenticated same-root marker-id/label/asset page into Beryl-state's
  unpublished reference-set build in the same atomic `HomeCommand` that advances the Syndic seal
  cursor, while retaining only bounded current page, cursor, and custody state. Beryl-state begins
  from the set identity and canonical empty accumulators, incrementally derives both summaries, and
  accepts the authenticated final summaries only at seal. It returns its opaque sealed-set proof;
  it neither authenticates Syndic roots nor depends on Syndic storage. The app composes these opaque
  proofs and never constructs an authority mapping from a draft commitment to a sequential summary.
- Exact reference-set completion is reopenable without weakening proof-gated reads. The original
  typed staging capability contains the set identity plus a caller-retained 32-byte secret; Asset
  construction persists only its commitment and independent compact manifest-state evidence. Given
  that capability plus both expected final marker summaries, Beryl-state point-reads the manifest
  and evidence and reports either the exact still-building manifest or the exact sealed proof after
  revalidating lifecycle, summaries, frontier, local chain, and final-proof commitments. A bare set
  identity, a newly minted capability for the same set, a capability without both expected
  summaries, or disagreeing completion facts cannot recover sealed authority. A fresh app service
  receives the original capability with the exact request-owned authority when Syndic already
  proves the matching marker seal complete, so a final Asset-seal commit never requires another
  marker traversal.
- Final marker-changing current-draft publication uses one mutating Syndic participant plus one
  Asset participant in a single `HomeCommand`. For a changed nonempty commitment, the Syndic
  participant validates its seal proof against the captured root and commitment and requires the
  new sealed Asset proof's sequential and ordered marker/asset summaries to match; the Asset
  participant swaps the exact `CurrentDraft(draft id)` head. For changed-to-empty, Syndic instead
  requires that seal to prove both canonical empty summaries and the Asset participant removes the
  exact prior head; no Asset proof or synthetic empty set exists. Both participants publish or
  neither does, and no second validation-only Syndic participant is present.
- Syndic compares the prior and captured exact draft-marker commitments before publication. Equal
  commitments reuse and validation-assert the existing nonempty CurrentDraft Asset head and proof,
  or validate an absent head for marker-free state, without sealing or scanning markers. A changed
  commitment, including undo or redo to a different marker set, requires bounded sealing and owner-
  head replacement or removal. Structurally different but semantically equal commitments may
  conservatively reseal and rebuild.
- Draft-to-turn and draft-to-accepted-input admission streams every marker into one sealed
  owner-neutral reference set. The final command independently validates the root-bound sealed
  `ComposerV1` content identity/full digest and opaque draft-marker seal proof through Syndic,
  requires its embedded `SequentialMarkerSummaryV1` and the seal proof's ordered association summary
  to equal the Asset proof's respective summaries, then rebinds that set from draft to admitted
  owner only when marker/label/asset order, first-occurrence disposition, frontier, and asset-chain
  digest also agree exactly. Build-local durable
  label-first keys avoid an in-memory set even when marker count is arbitrarily large.
- Accepted-input-to-submitted-item promotion performs the corresponding compact owner-head swap
  without rebuilding or scanning the paged set. Marker-free promotion validates both heads absent.
  The current-draft owner head is outside this transition and remains unchanged.
- Starting replacement edit retains the submitted item's immutable reference set and constructs the
  current draft's copy-on-write owner head over that same set. Later nonempty marker changes build
  replacement sets, while last-marker removal uses the sealed empty-removal branch above;
  historical ownership is never moved merely because its turn is being replaced. One Asset-domain
  mutation participant asserts the unchanged historical head and publishes the absent draft head;
  it never composes separate validation and mutation participants for the same domain.
- A Syndic thread owns one independently revisioned compact image-label-authority head, separate
  from the broad thread revision. The head contains the immutable inherited frontier and current
  permanent accepted frontier. Immutable origin spans bind each local accepted-frontier advance to
  its admitted owner and sealed set proof; point lookup resolves the label through that set's
  durable label-first index. A child discussion copies only its parent's exact permanent frontier
  into its new head at branch creation and follows immutable thread-lineage authority with constant
  resident state instead of copying the parent label index or origin spans.
- Syndic owns one opaque operation-qualified draft-marker label-readiness lifecycle. Its public
  final proof is non-cloneable, move-only, and bound to the current home generation, destination
  thread and exact label-authority and draft-label-protection heads, destination draft, editor-
  candidate session and generation, exact predecessor candidate root, operation, closed same-
  conversation reuse or cross-conversation allocation disposition, frozen occurrence commitment,
  sealed assigned target-id root, and any exact contiguous allocation range. Syndic derives the
  fixed-size durable readiness binding only while moving that proof into mutation custody; no
  public input can construct, inject, replace, or use the binding as independent authority.
- Every readiness page has exactly one homogeneous proof shape. A candidate/cut-only page contains
  only candidate or cut associations and uses Syndic's source-only proof: same-conversation reuse
  preserves the existing label only when exact destination authority proves the same `AssetId`; a
  live candidate occurrence is proved through its root-bound marker lookup, and a retained private
  marker whose occurrence was removed by its captured cut is proved through the exact source and
  successor candidate provenance. An accepted-only page contains only local or inherited accepted
  associations and uses one typed Syndic source contribution to validate the immutable origin span
  plus one typed Beryl-state witness contribution to validate the sealed-set proof, exact sealed
  manifest/completion evidence, label-first entry, and complete `AssetId`. `beryl-home-store` runs
  the two accepted-only roles on one coherent read snapshot independent of writer serialization and
  accepts only their equal fixed-size complete-page correlation. Candidate/cut-only pages have no
  Asset witness. One operation may ingest both page shapes in arbitrary order but never mixes them
  within one page. Neither domain reads the other's records, and the app never receives or compares
  the private facts. Missing provenance, stale candidate binding, an absent origin or label-first
  entry, or correlation disagreement makes readiness unavailable and cannot authorize insertion.
- Readiness evidence may arrive in arbitrary caller or source order. Each move-only operation page
  attempt owns one bounded canonical page and the complete sealed HomeStore proof plan with its
  independently retained receipt consumer. The app may transport only the opaque attempt state,
  executable command, and receipt; only the executable enters `HomeStore::compose_proof`, and the
  attempt retains the consumer. After the applicable source-only or Syndic/Asset source-and-witness
  evidence agrees, Syndic inserts every occurrence into operation-owned authenticated source-order
  and target-id indexes and rejects duplicate target marker ids. No caller cumulative digest,
  resident label map, or page prefix is readiness authority.
- Exact evidence EOF freezes the authenticated source-order root and count as the occurrence
  commitment, then starts bounded durable post-EOF assignment. For cross-conversation allocation,
  Syndic reserves a contiguous range derived from the authenticated occurrence count and strictly
  above the draft-label-protection head and every live destination reservation. Assignment consumes
  source-order leaves, reuses one label only for agreeing repeated source-label/`AssetId` pairs, and
  writes each final label into the matching target-id leaf. Same-conversation reuse writes the
  already authenticated label. Checked exhaustion or disagreement terminalizes without candidate
  mutation. Only a canonical-empty source root and zero unassigned targets permit final proof
  issuance.
- Durable readiness heads and indexes own operation progress. Syndic's home-generation runtime
  attachment owns only the configured bounded live reservations, compact destination reservation
  frontiers, and active attempt identities; it owns no page, cumulative stream state, replay
  history, durable index, label registry, marker collection, or unbounded waiter queue. The declared
  head, association, encoded-byte, and runtime-slot ceilings refuse new custody before exceeding
  their bounds without limiting a draft's marker population across completed edits.
- Replay is selected from durable authority rather than process-object continuity. The exact head-
  selected receipt, relevant target-leaf point reads, and retained source/target closure must be
  canonically byte-equal to the requested old or new closure. Digest equality alone is only a
  commitment and rejection aid; a differing occupied closure collides, an older receipt is
  obsolete, and re-presenting an equal-looking attempt grants no authority.
- `MutationBegin` moves the final proof into Syndic custody, derives the durable binding inside the
  package, and carries the sealed assigned target-root authority into the operation. Builder
  progress point-consumes the exact target-id entry for each marker-producing effect in actual
  effect order; no app-supplied label, association root, successor commitment, or structural digest
  can substitute for that entry. Final marker-changing adoption requires canonical-empty target
  authority, zero remaining entries, the exact proof, binding, reservation, proposal/effect closure,
  and storage-derived successor roots. One Syndic-domain command atomically publishes candidate,
  history, session, settlement, terminal readiness disposition, and any monotonic protection-head
  advance. It repeats no Asset participant because the immutable Asset evidence was already
  validated on the still-current HomeStore generation. Only later first acceptance advances the
  permanent accepted frontier and creates the immutable local origin span.
- Before durable begin, cancellation or terminal rejection releases the reservation. Once durable
  admission exists, cancellation first reaches a durable or exactly reconciled terminal staging or
  build settlement; after final writer admission the operation owner drains the classified outcome.
  Exact-old retains move-only custody for retry or cancellation, indeterminate transfers it to the
  exact reconciliation wrapper, exact-new and proven terminal noncommit release it, and collision
  keeps the scope closed until exact durable reconciliation. Attachment retirement invalidates
  process attempts and proofs but neither deletes nor reclassifies durable admission custody;
  bounded durable cleanup removes terminal operation records and retained replay closure. A
  committed result releases proof and reservation custody even with a later surfaced failure,
  because the settlement is already durable; publication still requires the exact receipt and
  current-generation health.
- Label caches contain only the thread identity, exact label-head revision/frontiers, exact
  candidate-root binding, and bounded resident pages or query results. Cache eviction makes label-
  affecting operations repeat bounded point or range reads; no synchronization path loads the whole
  current-draft marker collection, walks complete history, or rebuilds a whole used-label set.
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
