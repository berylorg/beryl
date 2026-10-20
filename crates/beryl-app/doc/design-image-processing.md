# Image Processing

This supplement is normative for the beryl-app media-service and feature-adapter boundary, governed
by [design.md](design.md). The [image system](../../../doc/systems/image-assets/design.md) owns
shared source, decoder, configuration and accounting contracts; the
[feature](../../../doc/features/image-assets/design.md) owns presentation and original commands.

## Process Owner And Graph Attachments

Bootstrap constructs one validated media configuration, process CPU rendition owner and GPUI shared
image domain before publishing window services. The existing clipboard/paste owner consumes the
same configuration's paste capacity; construction never adds another clipboard queue. Generated
media, path and verification capacities are handed to their existing or separately specified
service owners from that configuration.

Every published service graph contains one fresh media attachment. The process owner outlives
Home replacement but retains no Home or domain services. The attachment supplies exact generation,
weak source access, cancellation and bounded worker/completion tracking. Requests upgrade source
access only after stage admission. Retirement rejects new work, cancels and joins admitted workers
and drains queued/completed results before graph disposal. Cache entries and GPUI keys are scoped
to that attachment and removed through GPUI's whole-consumer retirement/drain. This domain serves
one published attachment at a time; its fresh consumer epoch starts only after the prior drain.
App need not retain a mirror of keys evicted from its CPU cache to retire remaining GPU entries.
Fresh graph publication cannot reuse old capabilities.
The ordinary asynchronous process close predicate includes that drain; a window callback never
blocks waiting for it.

The CPU LRU is the only App pixel cache. Its bounded map owns output reservations, pixels, active
reader counts and rendition keys. GPUI owns texture/staging admission, the native texture LRU and
pins. App retains no mirror texture map: a current bounded presentation may own its opaque pin or
pending ticket. The one GPU domain is shared through every Beryl window; constructing another
window cannot instantiate another media budget.

## Typed Source And Request Boundary

Composer requests originate from the exact still-live candidate root, marker id and View operation.
Transcript requests originate from its exact provider-authenticated media occurrence and resource
revision. GUI adapters capture that compact demand from their prepared current presentation facts;
they perform no storage read. After queue/worker admission, the attachment's source provider resolves
the captured occurrence through existing Syndic marker/resource and State admitted-Asset reads into
an internal compact source descriptor. Public renderer-facing state receives no storage service,
raw sidecar path or proof constructor. Cache hits still require current occurrence authorization;
finding an equal AssetId in memory cannot bypass it. Prepared completion returns bounded facts for
the GUI's in-memory currentness election, never a storage callback to execute during render.

Source resolution transports ordinary committed asset evidence, or the existing owner-qualified
selected resource evidence where supported. It never discovers repair staging via AssetId.
Workers open and verify the canonical sidecar selected by the descriptor through the configured
path/verification stages. Missing bytes, metadata disagreement, integrity errors, unsupported decode
or capacity refusal become typed rendition failures without removing references or altering labels.

Each request binds window lifetime, inspection generation, source occurrence/revision, attachment
generation and rendition key. One bounded interest joins an existing exact-key flight or reserves
a queue descriptor. Completion rechecks all captured facts before mounting pixels; a new selected
marker never receives an old request's result. Removing a presentation releases that interest even
when other windows keep the shared flight alive. Results cannot retain retired editors or windows.

## Decoder And Upload Dataflow

Workers perform the image system's source verification and PNG preflight before decoder allocation.
They use the pinned decoder directly, with explicit transforms/CRC/options, checked row storage and
pre-reserved fitted RGBA8 output; the generic image crate's full-frame decode path is not a fallback.
Sampler coordinates use checked arithmetic and an explicit revision in the rendition key. Final
dimensions and pixels must agree with admitted metadata and preflight before Ready publication.

Queue, worker, output and completion custody have one owning move-only operation; bounded maps
track only admitted operations. No per-asset thread, retry timer, unbounded mailbox or historical
failure map is created. Cancellation is checked between bounded I/O/row/copy quanta. Decoder slots
and source/page leases release on every terminal path, including errors after partial preparation.
Pure pixel results contain no service or window handle.

For a CPU cache hit, or after successful decode, App obtains a GPUI staging/texture ticket for the
exact domain generation and output extent. Refusal returns local unavailable and drops unpublished
custody. A worker copies the admitted pixels into the separately reserved staging allocation; its
completion is delivered through the bounded foreground election. Native upload checks the ticket,
current domain generation and request cancellation before allocation/publication. No file read,
image decode or resampling runs inside render, paint or upload callbacks.

An admitted CPU surface, outstanding staging copy and reserved/resident texture remain separately
charged until their actual owners release. CPU reader pins prevent eviction during copy. App must
acknowledge cancelled staging work before GPUI can release that allocation's charge. Generation
change rejects old completions, retains eligible CPU renditions and refreshes only current visible
interests through ordinary admission. It never rescans history or changes durable records.

## Feature Mounting And Focus

The existing composer marker menu and transcript inspection mounts consume Pending, Ready or local
unavailable rendition facts for their same source identity. A Ready presentation holds the shared
GPUI pin and paints the admitted resource through its public paint boundary. It cannot use
`img(path)`, `RenderImage` full-frame construction or a renderer-local source cache as an alternate
canonical-asset path. Layout, contextual commands and origin/fallback focus remain in existing GUI
and feature contracts; the media service owns no popup or selection.

Closing inspection, leaving a realized resource, source-revision change, thread switch and native
window close drop the corresponding presentation/interest and pin. Multiple mounts for the same
window/key share one unique pin charge. Pending and unavailable states keep the same marker/media
identity and disabled command reasons; rendition failure is independent of original readability.
Copy and Save executors resolve their original source independently under their owning command
contracts and never consume fitted pixels.

## Qualification

Package tests exercise real authenticated source descriptors, queue deduplication, output-capacity
transfer, unsupported/malformed source and every stale/cancel/retire transition. Decoder tests use
large-height sources with small fitted outputs and bounded row/workspace evidence. Native mounted
tests demonstrate composer and transcript Ready pixels, two windows sharing one domain resource,
pin/close/reopen release and foreground generation refusal. Aggregate diagnostics use the image
system's privacy contract. Production checks exclude test-support fault injection. Required final
review includes graph replacement and actual GPUI resource owners, not only successful helper calls.
