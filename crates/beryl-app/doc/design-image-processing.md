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

Bootstrap retains one process-owned foreground task for GPUI retirement maintenance. It awaits
the domain's bounded coalesced notification while idle, polls native retirement on the foreground,
and uses an asynchronous bounded timer only while completion is Pending. It survives graph
replacement and keeps running through zero-window shutdown until the domain drains. Native poll
errors remain observable in process failure custody and prevent disposal from claiming success;
they do not erase resource charges. On such failure the same task awaits GPUI's bounded recovery
notification, retaining error custody until shared device recovery clears it, then resumes ordinary
retirement maintenance. It never abandons maintenance when GPUI can subsequently recover.
Background source workers never invoke native retirement.

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

Current interests observe a bounded process native-progress revision captured when each request
is admitted. Foreground maintenance advances it after recovery and completed retirement, waking
only the existing bounded interests and refreshing the windows. A visible interest refused by a
transient native generation/capacity condition retains its compact demand and one notification
waiter, then retries through ordinary occurrence authentication after progress. This includes
requests invalidated before receiving a pin. Capturing the revision at admission prevents losing
progress that precedes failure delivery. Malformed, unsupported, stale source and cancelled interests
remain terminal. This creates no per-asset timer, historical retry map or source repair. The running
process retains access to its process media owner while graph services temporarily execute on a
worker, so foreground progress delivery survives graph retirement/replacement work.

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

## Original Command Adapter And Worker

The [original-command system contract](../../../doc/systems/image-assets/original-commands.md)
governs independent source and external-effect custody. App separates authenticated original
metadata/sidecar access from rendition format, dimensions and decode admission. Both paths use the
same exact `ImageSourceDemand` occurrence proof and current Home attachment; original commands
cannot request an asset by id alone. Original Save uses the authenticated length as its bounded
reader's exact source allowance, without inheriting a rendition length/pixel restriction.

The media process owns one original-command reservation shared across availability, Copy and Save.
Its fixed worker cohort services these jobs through the existing path/verification reservations;
original jobs do not spawn a thread, decoder, independent executor pool or unbounded queue. The
reservation remains held through a pending native picker and through any prepared output or effect
result. Waiting native picker state holds no source service or file. Availability preparation
returns only compact facts and releases its slot; a later command performs fresh authentication.
One detached foreground completion task retains the exact command and only weak GUI access until
the native picker result arrives, even after its presentation closes. It explicitly settles the
picker hold on success, cancellation or error; dropping an App receiver or cancelling the command
cannot stand in for native completion.
When admission is busy, an already mounted adapter retains only its one current compact demand and
may retry on the process's bounded capacity-change notification. It installs no timer, retained
closure history or extra source queue, and retires that retry with its exact surface/request.
The original-capacity notification owns a separate bounded broadcast of weak, future-owned waiter
registrations, capped by checked multiplication of the supported native-window count and configured
per-window interest allowance. Saturation returns typed capacity refusal. A dropped waiter removes
only its own wake registration; notification wakes all remaining admitted waiters after releasing
its lock. Capture registration/revision before attempting command admission so a release before
the future's first poll cannot be lost. A terminal operation failure does not automatically replay
that operation or retry because releasing its own slot emitted a notification.

Each visible image command surface owns one compact original request/result adapter alongside its
independent rendition interest. Its identity binds the exact occurrence, attachment, window and
surface revision. Original facts never derive from `Ready`, a rendition failure or a texture pin.
Stale completion is discarded and charged work cancelled; a fresh surface requests its own facts.
Prepared result state cannot retain a source file or expose a Home/native payload to GUI code.
Workers retain no GUI entities or callbacks. All live request handles are weakly registered within
the process's one bounded command slot and included in attachment drain.

Copy prepares one complete supported encoded image only after its capacity checks, then transfers
ownership to the foreground checked clipboard call without an uncharged clone. Save prepares its
same-directory stage on a worker. Both use one explicit preparing/prepared/publishing/settled
operation with a cancellation-versus-publication election. Foreground authorization requires the
same live occurrence, surface and window, and a still-current attachment immediately before the
election. The worker cannot infer election from a notification or a dropped reply. A rejected or
dropped preparation cancels and drains; an elected effect retains its original completion until
settled, even if its presentation closes. No failed or ambiguous effect is replayed automatically.

Atomic destination-file mechanics are shared with the existing text-resource Save writer through
a private worker-owned component: create-only same-directory stage, bounded writes, flush/sync,
authorized replace or non-replacing publication, and explicit cleanup failure. The component owns
no source, GUI, picker, command policy or cancellation election. Text and image adapters retain
their own exact source and publication owners; neither exposes its source through another adapter.
Existing text/table Save behavior and failure qualification remain required after this extraction.

Composer preview, transcript preview and direct transcript image menus use the same original
adapter and typed availability/outcome mapping. Preview commands are the existing feature-owned
contextual anchor plus its bounded anchored menu, with Copy followed by Save; the direct transcript
marker uses its bounded context menu. Inline placeholder text does not implement these placements.
Their actual focusable controls capture the
exact command anchor and a viable fallback before opening a surface or native picker. Disabled
activation consumes the event and leaves focus on that disabled command. A live preview delegates
focus to its child command surface/native picker while that flow is active, then returns to the
same eligible anchor; a closed preview follows its captured origin/fallback. A stale completion
cannot steal focus from a successor surface. Feedback is local to the exact command/image and
does not change admitted identity, draft content or rendition state.

## Qualification

Package tests exercise real authenticated source descriptors, queue deduplication, output-capacity
transfer, unsupported/malformed source and every stale/cancel/retire transition. Decoder tests use
large-height sources with small fitted outputs and bounded row/workspace evidence. Native mounted
tests demonstrate composer and transcript Ready pixels, two windows sharing one domain resource,
pin/close/reopen release and foreground generation refusal. Aggregate diagnostics use the image
system's privacy contract. Production checks exclude test-support fault injection. Required final
review includes graph replacement and actual GPUI resource owners, not only successful helper calls.
