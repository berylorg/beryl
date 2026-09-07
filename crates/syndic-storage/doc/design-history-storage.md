# History Storage

This supplement is normative only for package-owned durable thread, conversation-history, capture,
projection, resource, recovery, and privacy records. The package entry point controls scope and
rigor; [`design-schema-v7.md`](design-schema-v7.md) controls persisted bytes. CAS dispatch, stop,
compaction, accepted-input routing, and delivery lifecycle are owned by
[CAS live Syndic transcript](../../../doc/systems/cas-live-syndic-transcript/design.md). Product
presentation remains in feature authority; internal projection selection and scheduling are owned
by [Syndic conversation history](../../../doc/systems/syndic-conversation-history/design.md).

## Thread And Branch Records

A thread record binds stable identity, optional committed tail, current draft, thread revision,
optional immutable parent handoff, nonzero lineage depth, chain digest, deterministic ancestor skip,
and optional branch-context owner. A top-level thread uses canonical root-lineage facts. External CAS
ids never replace the Syndic primary identity.

Independent records prevent unrelated changes from fencing one another:

- Image-label authority owns inherited and permanent accepted frontiers and its own revision.
- Draft-label protection owns the greatest label protected by committed ordinary draft allocation
  and its own revision.
- Thread execution owns one immutable `ExecutionBinding`; a child inherits the exact parent value.
- Thread attributes owns state and accepted generated-title provenance under an attributes revision.
- Thread usage owns one bounded provider observation and its binding/generation provenance under a
  usage revision.
- Thread catalog summary is a compact rebuildable query value derived from current durable facts.

A discussion-context envelope is immutable, bounded, and keyed by its context-owner identity. It
retains exact selected UTF-8, source thread and turn provenance, and SHA-256 over those exact bytes.
The constructor observes no clock. Draft validation proves its immutable source and branch binding
without requiring the historical source to equal a later mutable child tail.

Every immutable turn stores stable identity, owning thread, optional immutable parent turn, nonzero
depth, deterministic ancestor skip, chain digest, accepted-input provenance, exact draft-submission
root, and bounded timestamps/metadata. Turn and thread lineage digests are structural proof anchors;
scoped reads validate exact depth, parent, skip, and digest progression without retaining an ancestor
set.

## Pristine Thread Abandonment

- The package exposes a bounded authenticated pristine-thread candidate covering the exact initial
  thread, current draft, empty draft root and history, immutable execution and attributes, compact
  summaries, gates, bindings, and every primary and index record created with that thread.
- A validation-only participant proves that a reused candidate still has the exact pristine
  closure and preserves it unchanged. A deletion participant accepts only the exact canonical
  fallback-creation fingerprint and deletes that complete pristine closure atomically.
- Any accepted input, draft mutation, turn, changed selector or history, live binding, active
  Syndic operation, mismatched creation fact, missing required record, or disagreeing index makes
  the candidate ineligible. Rejection deletes nothing and grants no authority over a replacement
  thread or draft.
- Natural-state inspection follows only the fixed bounded closure for the named thread and
  classifies it as the exact authenticated pristine closure, exact absence, or conflict. It does
  not scan thread families, infer application visibility, inspect Beryl jobs or claims, or decide
  whether the app may abandon a window.

## Conversation And Capture Records

Turn state is independently revisioned and carries one closed lifecycle, finalized-item frontier,
history disposition, provider-observation status, and the exact bounded provenance needed by package
reads. Source events are append-only normalized metadata referring to exact sealed provider-frame
ranges. They never embed a complete provider payload.

Canonical items have one exclusive source:

- A normal provider item is proven by its exact contiguous source-event sequence, provider identities,
  selected narrative generation where applicable, structural and chunk commitments, and completion
  frame.
- A terminal-repair item is proven by one exact immutable package-local repair snapshot, item ordinal,
  provenance, complete item-set commitment, and snapshot-backed manifests and ranges.

A completed normal item that is transcript-visible must have exact byte equality against its selected
append generation. A retained mismatch is history-incomplete or repair-required authority and cannot
select completion text for presentation. Digest equality alone is insufficient.

Content manifests bind content identity, optional canonical-item owner, encoding, lifecycle, exact
chunk frontier, encoded/logical lengths, atom and marker counts, and chain digest. Chunks and byte,
text, narrative, and piece spans are ordered bounded immutable records. Building content is
unreachable. Ownerless sealed content is immutable and content-addressed; item-owned live UTF-8
content may append only before finalization.

Provider structured values accept no more than 128 nested list/object containers. Validation streams
bounded state; strings and collection counts remain chunked rather than whole-value resident. Adapter
transport payloads are discarded after normalization and have no persisted catch-all JSON family.

## Fixed Lifecycle Content Publication

The package exposes one typed operation for the exact fixed lifecycle-continuation content defined
by the [lifecycle feature](../../../doc/features/lifecycle-yield/design.md). Its recipe and content
identity are package-owned; callers supply no alternative text, content identity, owner or build
frontier. The existing one-atom, one-chunk `ComposerV1` representation and V7 identities remain
unchanged.

The operation atomically publishes the complete fixed content, byte/text spans, pieces and sealed
ownerless manifest through the ordinary current-command and HomeStore mutation protocol. No
building state becomes visible between its writes. Its preparation and reconciliation inspect only
the fixed bounded record closure and obey package byte and record ceilings.

The fixed closure contains five records: one manifest, chunk, byte span, text span and piece.
Preparation uses one manifest point read and four owner-bounded cursor reads, each returning at
most two records and 65,536 encoded bytes so unexpected children can be rejected. The complete
canonical write closure is capped at 65,536 bytes. HomeStore owns the shared cursor lookahead,
decoding and reconciliation limits.

An already sealed object is reusable only when its complete canonical closure equals the fixed
recipe. Conflicting ownership, metadata or records, an incomplete object, or an unsealed object
produces an explicit non-success outcome without changing the existing records. This operation
does not repair, overwrite or finish a pre-existing partial object. Public content reads continue
to reject ownerless unsealed content.

Duplicate and concurrent requests converge on that one exact sealed object. Commit ambiguity
retains the existing opaque reconciliation custody; acknowledgement loss cannot authorize blind
resubmission or an alternative content identity. A sealed reference is consumable only after
durable publication or exact already-published classification under the mutation protocol.
Exact already-published classification is a typed noncommit outcome and leaves the stored manifest
revision and home/domain revisions unchanged.

## History, Activity, And Projection Records

History summaries, activity-query records, transcript views, item-projection sets, projection builds,
projections, and resource indexes are derived or immutable records with explicit source revisions and
generation commitments. Publication never mixes generations.

Activity-query authority binds owner/period, exact source memberships, source-event intervals,
provider lifecycle, ordering keys, retained bytes, cutoff, and counters. A rebuildable mismatch marks
the selected head stale and completes a bounded new generation before publication. It cannot expose
a partially rebuilt view.

Projection construction consumes one exact current live or immutable canonical source snapshot.
Source advance atomically stales the selected projection and supersedes an incomplete build;
completed older generations remain coherent history. Terminal closure freezes source content before
a visible item advances the finalized frontier. Operational items may advance after freezing because
they own no transcript projection.

A resource record stores bounded metadata and an exact immutable content or byte-range source.
Projection-resource mappings name resources; they do not copy payloads. Textual range reads are
explicit, bounded, and cursorable. Provider credentials and unnormalized transport envelopes are
never resources.

For a selected terminal-repair source, ordinary resource-metadata reads resolve the named item and
resource directly through its snapshot-backed locator. They return the exact target thread/turn,
item/resource ordinal, Asset page/entry locator, AssetId, and sealed media commitment needed by the
Asset boundary. The selected snapshot authenticates this reference; a merely staged or sealed
snapshot cannot supply it. Reads touch only the selected compact head and named bounded pages and
need no prior bulk materialization of resource records. Later projection/resource construction
preserves these same locators in bounded steps. Syndic never reads Asset private records or treats
AssetId alone as repair visibility authority.

The package stores facts needed by system services but does not own transcript selection,
presentation ordering policy, activity-row scheduling, title generation, retention UX, or renderer
residency.

## Public Bounds

- Metadata-only thread, turn-state, history-summary, binding, execution-snapshot,
  projection-metadata, and resource-metadata values are at most 65,536 payload bytes.
- Transcript-entry, transcript-path, item-projection, projection-resource, thread-lineage, and
  activity-query pages contain at most 256 records and 65,536 stored encoded bytes.
- One textual-resource response contains at most 65,536 payload bytes.
- One projection step consumes at most one 65,536-byte canonical chunk plus bounded UTF-8 carry and
  undecided Markdown state. Persisted undecided Markdown is at most 16,384 bytes plus one UTF-8
  scalar carry.
- Malformed or undecidable projection syntax is emitted as source-preserving spans of at most 8,192
  UTF-8 bytes.
- One recovery projection contains 1 through 262,144 nonempty canonical text items and at most
  262,144 logical UTF-8 bytes. The item ceiling follows from the byte ceiling.
- Callers may request smaller page bounds. Larger requests clamp to the declared ceiling and return
  a stable continuation cursor rather than allocating a larger page.

Every bounded collection count is decoded before allocation with checked arithmetic. Unknown tags,
trailing bytes, invalid UTF-8, noncanonical options, overflow, key/value disagreement, or an
impossible lifecycle combination fails closed.

## Recovery And Package-Local Repair

Routine recovery starts from an exact thread, turn, item, projection, resource, or operation anchor
supplied by the owning service. It follows only the anchor's bounded natural closure, double-observes
mutable heads, and reports concurrent drift separately from stable corruption. It does not enumerate
all threads, turns, gates, events, items, projections, or resources.

Recovery preflight for historical text first proves immutable topology, supported lifecycle,
media exclusion, nonempty item count, and the 262,144 item/byte ceilings without reading item text.
An accepted preflight returns only compact counts, tail, selected-path and represented-prefix facts,
sequence digest, and source revision. It retains no item vector or text accessor.

A ready proof opens one opaque non-cloneable cursor. Each read fills caller-provided bounded storage
with at most one nonempty 65,536-byte valid UTF-8 range and reports item ordinal, role, declared item
length, item offset, item terminal, and sequence terminal. EOF is returned only after exact
item/byte totals and the shared V1 sequence accumulator agree. Opening, every page, and EOF remain
bound to the exact source revision and selected path.

Recovery-complete means complete, interrupted, or failed with the full finalized-item frontier, no
history-incomplete disposition, and no provider-observation issue. It excludes incomplete.
Authority-lost immediate-tail context is a distinct scope-bound exception for owning-system recovery;
it does not rewrite the predecessor lifecycle or make earlier incomplete ancestors eligible.

Package-local terminal repair publishes one exact immutable repair authority and sealed item-set
commitment before repaired items become selectable. Repair never fabricates missing provider events,
changes cross-package dispatch authority, or weakens source provenance.

Each repair-media staging contribution commits one bounded witness page with its head advancement
and the matching Asset page. Complete item/resource membership and direct locators are validated as
pages arrive. Sealing uses the complete durable frontier and accumulated commitments; final
selection reads only the compact sealed head, fixed family commitments, gate, and matching Asset
publication witness. It selects snapshot-backed item/resource authority without scanning pages,
copying all resource records, or doing sidecar I/O. The final command and its reconciliation closure
remain fixed-size in media count. Fresh recovery can seal or select a fully staged durable candidate
through fresh handles but cannot fill missing stages or authorize another historical request.

## Privacy And Diagnostics

Persisted provider observations retain normalized kinds, identifiers, timing, counters, digests, and
references to bounded sealed ranges. They exclude access tokens, refresh tokens, API keys, cookies,
bearer headers, listener capabilities, raw request headers, and complete provider transport bodies.

Public errors and diagnostics identify records by typed identity, family, revision, lifecycle, count,
and redacted digest summaries. They do not include draft text, transcript content, resource bytes,
context selections, secrets, or unredacted provider payloads. Explicit content APIs are the only
boundary for sensitive bytes.
