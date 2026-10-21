# Generated Media Finalization

This supplement is normative for ordinary generated-output source evidence, admission, resource
publication and service-owned convergence. The [image system](design.md) owns shared storage and
resource limits; terminal-turn repair retains its separate complete-snapshot publication contract.

## Producer And Resource Evidence

The admitted producer is standalone `image_gen.imagegen` in CAS 0.146.0. Its authenticated
`imageGeneration` item is distinct from a dynamic-tool result with a similar name, hosted Responses
output or a path found in text. Inline base64 `result` remains structurally discarded at ingress.

First capture creates the resource in PendingAsset and, in the same Syndic command, creates its
generated-source record and pending-index entry. The source record binds the exact resource,
thread, turn, item, execution snapshot, initial source event and closed producer-contract tag.
Historical execution-snapshot and active-CAS-turn records supply runtime/root, process,
loaded-session and CAS identities; a current binding head cannot replace that historical join.

The source record advances from AwaitingCompletion to CompletedSource only with its exact admitted
completed provider frame. It retains that immutable completion source-event sequence and digest.
Later ordinary item/content freezing may advance revisions while preserving those completion
facts. Current item and manifest revisions are fresh concurrency guards, not permanent parts of
the source identity. A later differing completed source cannot replace an already selected source;
exact source replay follows ordinary capture replay rules.

The source fingerprint is SHA-256 over UTF-8 `beryl/generated-media-source/v1` followed by one NUL,
producer-contract byte `1`, source-state byte `0` AwaitingCompletion or `1` CompletedSource,
resource/thread/turn/item/execution-snapshot IDs as sixteen bytes each, selected source-event
sequence as little-endian u64 and its thirty-two-byte source-event digest. AwaitingCompletion
selects the initial event; CompletedSource selects the completion event. Syndic derives that
digest from its canonical bounded source-event encoding; callers do not authenticate a frame by
supplying a digest.

Syndic supplies a bounded opaque source-proof read with two closed branches. AwaitingCompletion
authenticates initial source-event and historical ownership; it permits only pending eligibility
or proven LostSourceAuthority resolution, never file access or Prepared admission. CompletedSource
additionally authenticates the sealed ProviderItemV1 completion frame, item kind, completed
observation and exact status, and extracts only the selected saved-path field. It follows reused text through existing typed content references
and skips revised-prompt bodies. The returned path is at most 4,096 UTF-8 bytes. Absent, empty,
oversized, malformed or unsuccessful output produces a closed unavailable classification; no
whole-frame value or fallback path is constructed.

## Original Source Ownership

Historical source evidence is descriptive. Opening producer bytes additionally requires fresh
authority from the same published service's original managed process and loaded projection. No
record, equal numeric generation, recreated session or replacement service can mint that authority.
There is no per-resource collection of retained source capabilities: admitted workers resolve
their source through the existing bounded live runtime/projection owners.

Eligibility distinguishes an authenticated current producer, transient owner/stage contention and
proven loss/retirement. Busy, capacity refusal or a pending owner lookup does not prove source
loss. It defers through existing bounded owner/capacity notification. Only a proven missing/retired
original authority can supply LostSourceAuthority; no short timeout invents that classification.

Host paths must be absolute native paths accepted by the Host path boundary; supported UNC paths
are not unconditionally rejected. WSL paths must be absolute POSIX paths in the exact recorded
distribution. Neither resolves relative paths against a current project or tries another runtime.
Ordinary OS symlink/reparse resolution is allowed at open; the opened object must be a regular file.
Every pass thereafter uses that same open object. File-identity continuity against external
tampering is not new authority, and a retained handle does not prove unchanged contents.

Backend owns the authenticated file-source capability. Host reads use the original File. WSL reads
use a fixed credential-dropped reader child inside the original managed namespace, under its
original supervisor/pidfd custody. They use no fresh WSL launch, host UNC read, shell, arbitrary
executable, process enumeration or replacement source owner. The backend and supervisor package
contracts own its exact request, framing, credentials and closure proof.

The source exposes bounded positional reads and independent zero-offset cursors over that one
file. HomeStore's digest, write/reuse and final-comparison passes may each obtain a cursor without
reopening the path or sharing a mutable file offset. Exact length, physical EOF, digest and
byte-comparison checks remain mandatory; changed bytes fail admission. Original source/process
custody remains held until actual reader closure and joined cleanup, including failure or cancel.

## Supported Admission And Durable Preparation

Ordinary generated admission supports PNG originals. Its fixed-size PNG signature/IHDR read
checks first-chunk type/length, IHDR CRC, legal depth/colour/compression/filter/interlace fields
and legal nonzero dimensions. These byte-derived facts establish media type and dimensions; it neither decodes a raster nor
applies rendition size, pixel, animation, interlace or output-allocation limits to the original.
Other formats terminalize the same resource as Unsupported. This admission support set does
not restrict generic Copy/Save for independently admitted JPEG, GIF, SVG or other originals.

The service worker streams source bytes through HomeStore's existing bounded sidecar admission.
Every zero-offset cursor compares the actual bytes it returns for the complete fixed PNG header
against the validated header, including HomeStore's first digest pass. Comparison is performed on
the returned bytes, not on a separate preceding probe. Short/mismatched headers fail that pass;
the committed media facts therefore describe the header of the bytes actually admitted even if
the producer changes the same file between preflight and hashing.
The authoritative image sidecar is durable before metadata or references are published. Reader
close uncertainty retains its original cleanup custody and prevents the worker claiming drain.

A bounded preparation HomeCommand has exactly one Asset participant and one Syndic validation
participant. Syndic authenticates the same PendingAsset resource and immutable completed source;
Asset consumes current-generation AdmittedSidecar authority and publishes metadata plus one
resource-qualified Prepared admission record. That record binds target IDs, source fingerprint,
AssetId and committed media facts. It is inert completion evidence, not a native source capability
or a canonical generated-resource reference. Preparation does not admit the visible resource.

Metadata creation and exact reuse share this participant. Reuse preserves existing creation and
metadata revisions, committed sidecar state and media facts. Media type and any existing dimensions
must agree with the validated bytes; absent optional existing dimensions remain absent. Conflicting
facts refuse publication. Equal AssetId never bypasses sidecar length/digest/byte comparison.

Prepared evidence is durable and independently readable through fresh State handles. It proves
completed byte admission under the trusted-home contract; ordinary replacement need not rehash
already admitted sidecars. Missing/corrupt bytes observed by a demanded read retain the existing
typed Home failure behavior. A bare AssetId, orphan sidecar or old AdmittedSidecar token is not
resumable evidence. Unselected evidence and sidecars are retained for future garbage collection.

## Atomic Resource Resolution

One final HomeCommand composes one Asset participant and one Syndic participant. The Asset
participant validates exact Prepared evidence and changes it to Published, creating the dedicated
resource-to-asset reference. Syndic validates the same immutable source and current resource,
changes that resource from PendingAsset to Asset, removes its exact pending-index entry and
invalidates the affected derived view/summary through ordinary selectedness rules. Both participants
bind the same target, fingerprint and Asset facts through HomeStore's checked witness composition;
neither can commit alone. Generated references contain no fabricated marker IDs, labels, marker
sets or transcript-projection owners.

An unavailable resolution instead requires the Asset participant to prove absence of usable
Prepared/Published evidence for the target, and Syndic publishes the closed unavailable reason and
removes the pending entry. A raced preparation is a conflict to reclassify, never authority to
discard admitted bytes. Missing source, failed/missing output, unsupported representation and lost
original authority use distinct closed reasons.

Resource resolution never rewrites provider lifecycle, completed source events or execution
outcome. An admitted resource may advance at most the next eligible frozen item's finalized
frontier; remaining items use the existing bounded finalizer. No sibling-resource or ancestry scan
enters this command. An unavailable resource is terminal and visible while its finalized-item
frontier remains behind and history_complete remains false. Existing terminal-history settlement
can release the ordinary gate; it does not invent a provider failure or repair resolution.

Preparation and publication use exact expected revisions and existing NotCommitted, Committed and
Indeterminate custody. Indeterminate ownership moves to the original Home command coordinator
before cancellation/reporting; fresh record reads cannot replace that settlement. Reconciliation
binds only the bounded changed/unchanged metadata, admission record, resource, pending index and
ordinary frontier/invalidation closure. No source read, sidecar I/O or whole-set traversal occurs
under the writer or reconciliation hook.

## Bounded Discovery And Coordinator Lifetime

Syndic maintains a global pending-generated-resource index from resource birth through terminal
resolution. Each resource receives a checked nonzero, never-reused increasing sequence. One sweep
captures its allocator high-water mark, then reads greater sequences only through that mark. New
entries cannot indefinitely insert ahead of an already eligible peer. Pages contain at most 256
records and 65,536 encoded bytes; continuation advances over scanned entries, including skipped or
already active work, without restarting on unrelated Syndic revisions. Every worker freshly
authenticates its exact target before preparation or publication.

One coordinator belongs to each published Beryl service. The immutable media configuration supplies
generated_media_queue_items (default eight), generated_media_worker_slots (default two) and
generated_media_page_bytes (default 65,536). Admission reserves one compact queue descriptor per
resource; scheduling transfers it into one fixed worker slot. A flight holds at most one configured
App I/O page, and joining an existing flight creates no second queue item or task. Backend fixed
reader framing buffers retain their separately declared fixed per-reader bounds. No window,
connection callback or waiter owns the coordinator lifetime or an independent finalization pool.

An AwaitingCompletion entry with an authenticated current producer remains pending without a
flight. Completion, terminal source loss, committed admission and released capacity coalesce into
ordinary service work notifications. Notification is registered before the next discovery/check;
full sweeps retain compact cursor/high-water facts and sleep when no eligible progress exists.
There is no per-resource timer, callback history, unbounded request list or global-revision reset.

Service retirement stops admission, cancels and joins queued/active source work, releases pages and
workers and transfers unresolved Home outcomes to their existing owner. Actual native reader close
must settle before its source custody drains. Replacement constructs a fresh coordinator and
discovers durable pending identities: exact Prepared evidence can complete publication; otherwise
only genuine original current-service source authority permits a read. Restart/replacement never
adopts old tasks, file handles or sessions. Without either authority it terminalizes the same
resource as unavailable, including resources whose producer never completed.

## Composed Acceptance

Qualification covers exact producer/release/frame and reused-path extraction; precompletion
restart discovery; fair high-water sweeps under unrelated capture and saturation; same-open-file
multiple passes, header change before the first digest pass, mutation/EOF/length refusal and
bounded large originals; Host and original-namespace
WSL credentials/read/close/cancellation proof; durable preparation followed by process/service loss;
metadata deduplication; atomic two-domain publication and every commit/acknowledgement-loss cut;
same-resource unavailable/history-incomplete settlement; and complete queue/worker/page/source drain.
Mounted qualification proves stable generated marker identity through pending, admitted and
unavailable states, shared rendition/original-command use and stale-safe focus without fabricated
image-marker storage identity. Component or source-only evidence is not the composed acceptance.
