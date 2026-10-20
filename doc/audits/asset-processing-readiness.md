# Asset Processing And Deferred Cleanup Readiness

Read-only readiness assessment at Beryl commit `468366548b23268fe1a9bb8c053f5ae12e5d6010`.
Accepted after independent readiness review. This is source and evidence reconciliation, not a fresh
asset runtime qualification or acceptance of the missing media consumers.

## Boundary And Authority

Checkpoint 7 follows the accepted complete discussion flow. Its remaining outcomes are image
admission and runtime projection, generated-output settlement, bounded presentation resources,
original-image Copy/Save, background title maintenance and deferred cleanup. Successful branch
archive presentation is already qualified by the [discussion flow audit](discussion-complete-flow-qualification.md).

The [image feature](../features/image-assets/design.md) owns visible state, identity, focus and
Copy/Save outcomes. The [image system](../systems/image-assets/design.md) owns exact bytes,
sidecars, runtime projection, generated provenance and media capacities. The
[State asset contract](../../crates/beryl-state/doc/design-assets.md) owns admitted-sidecar and
reference proof semantics. App composition follows its
[catalog/composer](../../crates/beryl-app/doc/design-catalog-and-composer.md) and
[feature adapters](../../crates/beryl-app/doc/design-feature-adapters.md).

The effective contracts are production-application/v2 with the image feature's external-side-
effects/v2 and irreversible-operation/v2 modifiers and State's persistent-state-integrity/v2
modifier. The image system requires untrusted header/path admission and explicit bounded media
capacities. Ordinary unavailable outcomes are permitted where specified; missing implementation
cannot be relabeled as image readiness or successful output.

The rework's explicit-incomplete recovery cutover excludes historical repair adapters, source
snapshot/media staging dispatch and retained-service adoption. It does not exclude live generated
output admission. No new garbage collector, runtime cache, alternate sidecar store or compatibility
image pipeline is needed or authorized.

## Existing Production Foundations

- `composer_host/paste_image.rs::admit_clipboard_image` checks cancellation and a positive byte
  limit, parses bounded PNG/JPEG headers, streams the exact bytes through Home sidecar admission
  off GPUI and publishes typed Asset metadata. The selected clipboard operation reserves its
  existing queue before acquisition, captures one exact selection/range, and retains its permit
  through preparation and mutation settlement. Bootstrap currently supplies one queue item and
  4,096-byte sidecar pages. The complete image process configuration must incorporate this owner,
  not create a second paste queue.
- `AssetState::publish_metadata` consumes opaque `AdmittedSidecar` evidence matching the images
  namespace, digest, length and creation revision. Its contribution requires the same sidecar in
  the Home command. `HomeStore::admit_sidecar_stream` hashes and verifies exact-length source pages,
  verifies reuse or makes a new sidecar durable before issuing generation-qualified evidence.
  This is an existing durable byte boundary, not an absent storage engine.
- State's reference-set append/seal and owner-head operations are bounded and proof-gated. Removing
  a reference head removes only that metadata head; it neither deletes sidecars nor proves final
  reachability. Existing paged markers, stable labels, promotion and historical-reference paths
  remain reusable within their qualified scopes.
- `input_replay/marker/source.rs` authenticates committed Asset metadata and exact admitted sidecar
  bytes before deriving an outbound input path. `marker/path.rs` implements Host Unicode paths and
  WSL absolute drive mapping, refusing unsupported paths without copies, root changes or caches.
  This is outbound Beryl-asset projection, not authority to read a provider's generated `savedPath`.
- Composer and transcript markers already preserve exact zero-width identity, label, geometry,
  currentness, menu origin and fallback focus. Those consumers can host a real image adapter
  without replacing marker or selected-thread authority.
- Generated title acceptance and resolved generated/history/fallback display are Syndic-owned.
  Existing selected-title/catalog readers already display accepted attributes coherently. The
  successful discussion flow already proves archive/navigation presentation.

## Missing Generated-Output Consumer

The bounded backend admits the pinned standalone image-generation item through `savedPath` while
discarding base64 `result`. Syndic's live item path creates `PendingAsset`; App convergence consumes
already admitted assets or preserves incomplete pending/unavailable media. Neither is a producer-
owned file reader or generated finalizer. No production coordinator or live transition to admitted
Asset/terminal Unavailable was found.

The next generated architecture slice must determine the compact resource-owned Asset reference,
exact durable producer and sidecar-admission evidence, typed cross-domain finalization participant,
and the published service's original source reader and pending scan. It must preserve completed
provider lifecycle bytes while atomically changing only resource disposition and canonical
finalization/history frontiers. Completed authenticated output is required; a Started frame cannot
be rewritten into an admitted image prematurely.

The implementation boundary then connects actual savedPath ingress to exact Host/WSL producer
reading, sidecar durability, atomic State/Syndic settlement and bounded replacement/disposal
convergence. Queue/worker/page reservations transfer rather than duplicate. Missing or stale
producer authority, changed/unsupported/oversized output, saturation, cancellation, shutdown and
uncertain publication must preserve original outcome custody and converge as the system specifies.
Equal-looking paths or recreated sessions cannot supply recovery evidence.

## Missing Rendition And Original-Command Consumers

The current mounted composer image state has only Pending/LocallyUnavailable; View starts Pending
without a completion route to Ready. The transcript popup has real origin/fallback focus handling,
but renders unavailable text and static disabled Copy/Save commands. Production transcript resource
demand rejects non-Code/Table resources and non-text backing. Generic resident media command tests
provide synthetic byte-range targeting, not canonical original-image handles or an OS executor.

The published App graph has no process-owned decode queue, CPU surfaces, upload staging, shared GPU
cache, window pins or device-generation owner. Its existing two-field paste configuration does not
provide the image system's complete positive immutable capacity set.

The [pinned GPUI investigation](../memory/github.com/berylorg/zed-fork/commit/3210d6ad35f90a3d13d705b836e84ae206b3d656/windows-image-resource-ownership.md)
establishes a concrete dependency boundary: Windows shares a DirectX device, but each renderer
constructs a separate image-resource store and each Window its own source-backed store. File
preparation reads the whole original and raster preparation decodes a full frame before fitting.
The internal upload/resource/paint types are private. Connecting `img(path)` would violate the
required source/surface boundary and would not establish shared accounting.

Before implementation, the owning image system and GPUI fork contract must specify shared renderer
resource-domain ownership, a narrow already-admitted pixel upload/paint seam, allocation estimates,
device generation and cross-window resource/pin lifetime. The App contract must specify its
one process media owner, bounded original-source/decoder policy, configuration, cancellation and
ownership transfers through CPU/staging/GPU admission. Unsupported full-original decoders must
produce local unavailable rather than allocating an unadmitted raster and downsizing it.

The observable rendition boundary is a real asset-backed Ready preview under those capacities,
including shared windows, range changes, retirement and device loss. Original-image Copy/Save is
a separate effect boundary: exact admitted original bytes, checked clipboard representation,
native destination/overwrite authority, atomic file publication and exact focus return. Its
availability remains independent of rendition readiness, as the feature requires.

## Missing Title-Maintenance Wiring

The App contract already requires bounded background generation from exact Syndic eligibility,
one maintenance backend session and one-way attribute acceptance. Production searches find only
the standalone title task/backend/prompt helpers and title display readers, with no constructor or
polling consumer wiring the maintenance task into the published graph. Tests of accepted generated
attributes prove rendering, not automatic generation after the first real user input.

Before mounting this outcome, complete the App service's maintenance admission, wake/scan bound,
runtime/session ownership, cancellation/join, source revalidation and failure policy. Generation
must not occupy a foreground stream, import global developer instructions, create a visible catalog
thread/turn or publish CAS thread names. Typed Syndic title validation and immutable source witness
remain authoritative; obsolete helper behavior cannot replace that contract.

## Existing Evidence And Limits

- The [captured-paste qualification](composer-marker-feedback/captured-paste.md) records a 32-case
  sidecar receipt (`599d8a31-51d2-490a-9b27-9b47bf625382`) and real clipboard-to-mutation
  composition. Current `assets_v3`, `sidecar_durability` and streaming test sources cover paging,
  stale/forged references, atomic heads, reopen/removal, crash/fault/cancel and replay drift. These
  are inventory and scoped historical receipts; final asset qualification must assess reuse against
  its actual changed inputs and run the owning affected boundaries.
- `clipboard_image_admission` tests cover bounded header checks, CRC/JPEG header scanning, sidecar
  deduplication and byte/cancellation refusal. They do not decode pixels.
- The provider image-marker and native indexed-resource marker cases prove exact identity,
  realized geometry, keyboard/menu and focus fallback. They do not prove Ready pixels or Copy/Save.
- `syndic_transcript_media_actions` uses synthetic resident ranges to prove targeting and staleness.
  `image_preview_popup` proves geometry/outside-hit behavior. Neither supplies the new image reader,
  decoder or OS output effect.
- GPUI's current `image_render_resources` test source covers real existing per-window upload,
  clipped demand, cancellation, budget refusal/release and resource-clear reload. It does not
  establish process-shared surfaces/textures/pins or old-generation upload rejection.
- Accepted-input marker replay and active steering exercise labels and actual localImage input
  transport. The pending-generated startup case intentionally projects later text while preserving
  PendingAsset; it is not finalizer convergence evidence.
- The complete discussion audit supplies successful archive/navigation. Its unrelated five
  ancillary large controls retain their existing inconclusive scope until the owning final gate
  requires them. No new stress or performance measurement was run here.

## Execution Boundaries

Continue with explicit shared-image architecture readiness before image processing code. Then
qualify actual bounded renditions and original-image commands as separate outcomes. Complete the
generated-finalization owning contract before its source/coordinator implementation, and complete
title-maintenance lifecycle readiness before mounting that independent background outcome.

Checkpoint 7's final composed qualification must join paste/generated admission, sidecar/reference
ownership, labels, Host/WSL projection, visible resource release, exact original commands,
title/archive and deferred collection. Source or native evidence must distinguish original byte
admission from rendition readiness and must demonstrate bounded release or cache plateau across
the relevant operations. These gaps have clean implementation directions; no Operator-only blocker
or software installation requirement was identified by this readiness assessment.
