# Shared Image Renditions

This normative supplement owns the cross-package source, decoder, cache and renderer boundary
assigned by [design.md](design.md). Its engineering rigor is declared by that entry point.

## Ownership And Identity

`beryl-app` owns one process media configuration and CPU rendition cache. Each published Home
graph attaches a fresh cancellable media service to that owner. The attachment alone grants
storage access; the process cache never retains a Home, Syndic service, editor or native window.
GPUI owns one shared native image-resource domain for the process renderer device family, including
the only texture cache, GPU/staging admission ledgers, LRU and per-window pins. App owns no second
texture cache or native allocation ledger. Windows using that domain share one texture allocation
for the same rendition. Driver-private allocations remain outside the documented texture estimate.

A rendition key binds the Home attachment generation, exact AssetId, fitted pixel extent, pixel
format and sampler revision. An opaque domain-local key transports this identity to GPUI without
exposing storage facts. Changing scale or requested extent changes rendition identity. Equal bytes
never merge marker identities, labels, source authorization or feature publication revisions.

The supported rendition is a single fitted, aspect-preserving image, with no upscaling and a
deterministic nearest-neighbour sampler. Visible marker inspection requests it on demand; no
history scan, whole-asset preload, full-resolution zoom or tile pyramid is required. Durable asset
size and reference count remain independent of these presentation limits.

## Original Source Authority

The initiating feature captures its exact live composer candidate/marker or transcript resource,
revision, window/request generation and focus origin/fallback before requesting preparation.
GUI capture uses prepared presentation facts; an admitted attachment worker resolves that demand.
Syndic authenticates the occurrence and returns its exact admitted Asset reference through its
ordinary bounded read boundary. State authenticates committed ordinary metadata for that reference;
selected repair resources, where supported, require their owner-qualified read rather than an
AssetId-only lookup. A path, label, unselected metadata or cache key cannot grant source authority.

An admitted worker opens only the canonical sidecar selected by those facts. It verifies exact
length and SHA-256 through bounded reads on that open file before decoder publication. It never
uses a provider path, clipboard buffer, generic GPUI file loader or temporary copy as fallback.
The same verified handle supplies preflight and decode after bounded seeks. Final sidecars are
immutable within the trusted-home envelope. Source handles live only in admitted workers; waiting
requests carry compact descriptors and weak attachment access. No handle is retained per marker.

Original readability is a separate result from rendition support. Unsupported rendering, cache
pressure or device loss cannot disable an otherwise eligible exact-original Copy or Save. Those
commands acquire their own bounded source authority and never recover originals from decoded pixels.

## Decoder Envelope

The supported software decoder is exact `png` 0.18.1, using incremental `Reader::read_row` into one
bounded transformed row, followed by direct sampling into the already reserved fitted RGBA8 output.
Supported input is static, non-interlaced, eight-bit truecolour RGB or RGBA PNG. Transparency is
preserved; colour-profile conversion is outside this rendition contract. JPEG, palette/greyscale,
sixteen-bit PNG, APNG and interlacing return local unsupported rendition outcomes. They may still
be admitted originals and remain eligible for original commands.

Before constructing the decoder, bounded streaming preflight validates signature, IHDR, supported
dimensions/format, chunk framing/order, all chunk CRCs, a complete IEND and physical EOF. It rejects
animation chunks, eXIf and unknown critical chunks. Text and ICC metadata are not decoded and their
decoder options explicitly skip them. Every chunk the decoder may buffer must fit the configured
processed-chunk bound; IDAT bytes are streamed, not assembled into a compressed-image buffer.
Checked width/height, source pixel count, raw row size and total encoded length must fit the
presentation envelope before decoder admission. Cancellation is checked at each bounded read and
row; a row and admitted source have finite work independently of total durable asset storage.

CRC checks, ancillary CRC strictness and Adler checking are explicitly enabled in the decoder.
Ready requires the expected complete rows and successful decoder finish after source preflight and
identity verification. This is the pinned decoder's interpretation of the authenticated PNG;
its public finish API tolerates some otherwise output-complete zlib trailers. This contract does
not claim a separate strict DEFLATE/Adler conformance validator. Identity mismatch, preflight or
decoder error publishes no pixels. Successful preparation never changes durable asset metadata.

Decoder private workspace is bounded by the admitted row/chunk limits and the pinned decoder's
row-buffer, metadata and fixed inflate scratch behaviour; it is not bounded by `png::Limits::bytes`
alone. That option bounds only the allocations to which the dependency applies it. Workspace
qualification must account for private Vec capacity growth as well as requested lengths and fixed
inflate tables. Source height must not cause retained decoder workspace growth. Allocator bookkeeping
and process RSS are outside the surface ledger, just as driver-private allocations are outside the
GPU estimate. A dependency change must requalify this workspace envelope before adoption.

## Configuration And Stage Admission

The immutable media configuration in the entry point is extended with positive
`rendition_source_max_bytes`, `rendition_source_max_pixels`, `rendition_source_max_dimension`,
`decoder_row_max_bytes`, `decoder_processed_chunk_max_bytes` and `rendition_max_dimension`.
Startup validates checked products and that one maximum fitted RGBA8 output fits a surface, CPU
cache, staging buffer, texture and window pin. Capacity shortage at runtime is local unavailable;
it never enlarges a limit or starts uncharged work.

Startup values are: generated queue 8, generated workers 2, generated page 65,536 bytes; paste queue
1; path-preparation and sidecar-verification slots 2 each, sidecar page 4,096 bytes; decoder slots 2
and queue 16; surface maximum 4,194,304 bytes; CPU cache 16 entries/67,108,864 bytes; upload staging
2 buffers/8,388,608 bytes; GPU cache 32 entries/134,217,728 estimated bytes; each window 8 pins/
33,554,432 estimated bytes. Encoded rendition source and source pixels each have a 67,108,864 limit,
source dimension 16,384, raw row 262,144 bytes, processed chunk 1,048,576 bytes and fitted dimension
1,024. These are presentation/service capacities, with no persistent asset-size rejection inferred
from them. Configuration remains fixed through same-process Home replacement.

Queued decode descriptors reserve queue entries; joining identical work adds only a bounded window
interest, not another worker or queue entry. Scheduling atomically transfers queue admission to a
decoder slot for bounded verification/preflight, then reserves the complete output before decode.
Verification/preflight use the configured sidecar slot/page and release that slot before decoder
construction; the same open source handle remains under the admitted decoder slot. A blocked
transition either keeps its existing bounded descriptor or returns unavailable.
Every request has one current stage and no unbounded completion channel or negative-result cache.

CPU output is charged by actual owned allocation capacity, including an unpublished decoder output.
Promotion transfers that reservation to the process CPU LRU. Active decode/upload readers pin CPU
entries until finished. Only unpinned entries may be evicted. Failed allocation, cancellation, stale
identity or refused publication drops the exact reservation and partial output.

GPUI staging admission reserves a buffer count and complete allocation capacity before App copies
RGBA pixels into that buffer on a worker. It also reserves the native texture estimate before
native upload. Staging, GPU and CPU copies are separately charged while simultaneously retained.
Native upload occurs on the renderer/platform foreground boundary; file I/O, decoder work and
pixel preparation remain off the GUI thread. Successful upload transfers its reserved texture into
the shared GPU LRU and releases staging. Refusal releases staging and the unpublished GPU charge.

## Pins, Retirement And Device Loss

Every native window has one bounded pin scope. Repeated presentations of one rendition in that
window share its unique entry/byte charge; separate windows each charge their own pin but share
the process texture. GPUI pins and frame-use references prevent eviction while native paint can
still consume a texture. Frame references retain the original charge until their last use ends;
retired resources cannot disappear from accounting while a native reference survives.

Leaving the realized range, changing source revision, closing inspection, replacing selection or
closing the window releases its exact interest and pins. Last-interest cancellation removes queued
work or cancels its admitted worker; completion cannot revive the interest. Unpinned cached pixels
and textures may remain only under their LRU capacities. Saturation does not evict another window's
pinned content. Local unavailable feedback retains no second source or deferred unbounded waiter.

Home attachment retirement fences new requests, cancels and joins its workers, discards its queued
descriptors/completions and releases its CPU entries, domain keys and pins before old-graph disposal.
The domain's whole-consumer retirement fences and drains its bounded native entries/in-flight owners
without requiring an App-side key registry. A fresh attachment starts a fresh consumer epoch only
after that drain; the device generation may remain unchanged.
The process owner and immutable configuration survive; no old Home capability or cached entry is
adopted into a fresh attachment, including same-home replacement. Window and process shutdown await
the same exact owned-resource drain without blocking GPUI on worker joins.

GPUI owns one non-reused device generation per shared domain. Device loss is serialized with native
upload/paint, invalidates all old upload tickets and opaque native payloads across every window,
releases their charges/staging and makes old handles unavailable. In-progress CPU copies retain
their staging charge until cancellation acknowledges and frees them. Recovery cannot borrow that
charge early. Old native payload references must drain before their charges disappear; device
generation cannot be reused on overflow or failed recovery. Active interests request lazy upload
from still-admitted CPU cache entries after recovery. Missing CPU entries re-enter ordinary decode;
old GPU handles never authorize source or stale publication.

## Acceptance Boundary

Qualification covers actual asset-backed composer and transcript inspection and native pixels,
independent original eligibility, hostile dimensions/chunk framing/CRC, unsupported formats,
bounded rows and fitted allocation without a full-original output, allocation/capacity refusal,
cancellation at each stage and stale feature/Home completion. It measures shared unique texture
identity and per-window pins with two native windows, eviction with pinned peers, frame-safe drop,
window-close/reopen plateaus and shared device-loss/late-upload refusal with visible lazy recovery.
Content-free counters distinguish CPU capacities, decoder slots/queue, staging, reserved/resident
GPU estimates, pins, cancellation/drain and generation. Deterministic fault seams supplement actual
native resource ownership; synthetic counters alone do not prove it. Required independent review
covers the complete cross-package source-to-paint and retirement boundary.
