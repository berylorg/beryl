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

## Discussion Creation

The package exposes an opaque prepared discussion-source witness from exact captured
`DiscussionContextSource`, bounded selected bytes and current source-thread/transcript proofs.
Preparation proves terminal finalized assistant content, exact current projection-set membership,
the selected source turn's membership in the named thread's current path, normalized UTF-8 range
and byte equality. It uses bounded existing text-range reads outside the writer and retains at
most the admitted 65,536-byte selection plus compact source records. It never copies the source
message, transcript, thread ancestry or image-label map. Read drift returns a conflict; no alternate
projection, selection, source thread or range is inferred.

The witness binds home/domain provenance and exact mutable source anchors. The creation mutation
rechecks those anchors and immutable source identities at the serialized writer, including exact
current transcript-entry and finalized projection-set membership. Captured selected bytes are
already authenticated against immutable finalized history; the writer need not reread a provider
message or scan the selected path. A stale witness cannot create a discussion. Fresh same-home
recovery reacquires and authenticates the same captured request rather than accepting a stale
generation's witness.

One typed creation participant publishes the child thread, immutable parent/context link and index,
context-owning initial draft, empty editor root/history and reverse binding, execution/attributes/
usage seeds, input and discussion gates, compact label/protection heads, history/catalog/activity
seeds, transcript initialization and unbound CAS binding together. New identities are caller-owned
operation facts checked against every created key and deterministic draft/turn/input alias. The
initial selected tail is the exact source turn and its digest, excluding later parent turns; the
draft's discussion-context intent derives its first submitted parent from that same envelope.

Execution inherits the exact immutable source-thread binding. The new discussion starts open with
no generated title or provider usage observation; compact title derivation follows the existing
policy for the selected prefix. Its inherited and permanent image-label frontiers start at the
parent's observed permanent frontier, and its draft-protection head starts at that same frontier.
The mutation fences that parent label-head revision and computes bounded lineage depth/digest/skip
proofs without copying origin spans. Initial history/transcript freshness follows normal selected-
tail creation; creation does not claim a rebuilt inherited transcript.

The operation's reconciliation closure is its exact created records, including the immutable
envelope and initial open discussion gate. Source observations are read-only preconditions. Exact
absence of the complete closure means old, complete equality means created, and partial/different
closure means collision. A public bounded natural-state read supports ordinary and candidate
access without exposing private storage encodings. Creation creates no CAS thread, view claim,
window or dispatch capability and provides no discussion deletion path.

## Discussion Handoff Gate

The package owns a compact `DiscussionHandoffGateRecord` keyed by discussion thread with its own
positive revision and closed `Open` or `Pending` state. Pending retains only the exact resolution
intent, handoff job and resolving Syndic turn identities. Every discussion is created with an open
revision-one gate. Ordinary threads have no gate; missing, foreign or orphaned discussion gates
are corruption. Title and archive records retain their independent revision semantics.

Typed ordinary and candidate reads expose this bounded record. Gate admission validates exact
Syndic domain, thread, attributes, input-gate, handoff-gate and resolving-turn revisions, current
selected turn, active binding/snapshot/CAS-turn correlation, immutable parent/context ownership,
and zero accepted future-turn input at the writer. It contributes only the open-to-pending gate
transition. The app must compose it with State intent/job admission; this package neither reads
State jobs nor allocates their identities. Parent identity and queue-frontier observations form a
separately typed proof validated inside that same Syndic admission mutation. The command contains
one Syndic participant and one State job participant, preserving the store's domain uniqueness rule.

Release requires the exact pending identities and gate revision and contributes a checked next
open revision. Terminal failure uses a gate-only release mutation. Successful handoff uses one
Syndic mutation that releases the gate and publishes exact archive together, preserving the current
title and independently checked attributes revision. The app composes that single Syndic participant
with the State terminal job transition. No retryable transition releases the gate. Mutations preserve
unrelated thread, draft, execution and history records. Natural reconciliation compares only the
named gate's exact old/new records, plus the exact attributes old/new pair for successful archive.
Parent, binding and turn observations remain read-only
preconditions and cannot substitute for outcome evidence. It never scans discussions or jobs.

Package-owned checks reject forbidden discussion mutation at its durable publication boundary,
including new input/steering acceptance, draft edit/history publication, replacement or selected-
path changes, and lifecycle successor admission. Existing capture, already-admitted steering,
stop/terminal convergence and cleanup preserve access to their exact admitted scope. The app
composes these checks with the system's whole-operation custody before exposing resolution.

## Discussion Child Settlement

- A bounded probe accepts the exact pending discussion gate, including intent, job and resolving
  turn. It authenticates that gate, the unarchived discussion, its selected resolving turn and
  terminal history, the input gate and current binding. A missing or contradictory identity is an
  error, never an empty queue. In-flight turn, steering or terminal-history convergence is `Waiting`.
- Settlement requires proven terminal resolving-turn state, the existing complete terminal-history
  publication proof (including explicit incomplete history), an idle input gate, no live steering
  and no active binding. Nonzero accepted future-turn count then yields `QueuedInput`; zero total
  live input yields `Ready`. Neither result consumes accepted input, opens another turn or calls CAS.
- Ordinary and explicit candidate probes use bounded typed point reads under stable source domain
  revision and fresh handles. Settled results carry opaque preparation bound to that original
  handle, home identity, domain revision and exact pending gate. `Waiting` grants no command
  capability. Candidate preparation does not publish ordinary home access or scheduling authority.
- A `Ready` preparation exposes one Syndic validation contribution for atomic composition with
  State's resolving-completion transition. A `QueuedInput` preparation exposes one gate-release
  mutation that revalidates the settlement proof inside the same Syndic participant. Wrong-result
  use is rejected. The latter composes with State's child-input terminal failure and preserves all
  queued content, draft and history. No separate same-domain validator is added beside that release.
- Writer preparation repeats the exact bounded settlement check before either publication. Source
  revision changes, stale handles, changed gate or changed disposition reject the whole command.
  Ready validation writes no Syndic records; queued release uses the existing exact gate old/new
  outcome witness. Read-only settlement observations are not mutation outcome evidence.

## Pristine Thread Abandonment

Ordinary fallback-thread pristine closure requires absence of a discussion gate. This abandonment
API accepts only ordinary root threads; discussion lineage or context is ineligible. Discussion
creation does not acquire deletion authority, and its durable result survives activation failure.

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

Turn state also owns the bounded dispatch provenance required by the CAS-live system. Ordinary and
lifecycle-continuation admissions initialize unattempted provenance; provider-operation admissions
initialize their distinct marker. Activated and cancelled provenance retain one exact snapshot and
active binding revision. Every turn-state constructor requires explicit provenance, and unrelated
state updates preserve it. Binding activation and exact cancellation publish the provenance and
its next turn-state revision atomically with their existing records and reconciliation closure.

The package authenticates cancellation through the exact snapshot, historical active binding,
immediate valid successor and absence of a published active CAS turn, with same-thread/turn/path
agreement. Ordinary activation and stable pending-proof reads use this fixed closure. A provenance
tag alone cannot authenticate cancellation. A pending proof binds the current home generation,
durable revision, turn state, gate, selected path and canonical input identity/content; mutable
anchor drift fails as concurrent change. It supplies durable evidence to the app, not live dispatch
or cleanup authority. Provider-operation receipts retain their separate proof contract.

Pending lifecycle-continuation descendants retain their exact initial canonical input, content,
source-free capture and item counters. Unattempted provenance retains the initial turn-state
revision; activated or cancelled provenance permits later pending revisions only with its exact
authenticated dispatch closure. Binding activation and cancellation do not require a provider
activation event. Whole-home validation and scoped consumed-compaction successor reads apply the
same distinction, preserving their existing bounds and concurrent-change behavior. A later revision
or provenance tag alone is insufficient; missing or mismatched historical authority is corruption.

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

Routine non-idle work discovery starts from the compact source below and then follows exact
candidate anchors. Source membership alone does not authorize dispatch, recovery or a provider
effect.

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

## Candidate History Metadata

Named candidate counterparts of `thread`, `turn`, `turn_state`, `input_gate`, `canonical_item`,
`content_manifest`, `resource`, `item_projection_head`, `item_projection_set`,
`item_projection_build`, `transcript_view_head` and `transcript_build` supply exact bounded
metadata for candidate history convergence. Each uses borrowed candidate recovery access and the
ordinary typed family, key, codec, byte limit and absence/error semantics. The content-manifest
ownerless/unsealed guard remains shared. These point reads do not stabilize a multi-record snapshot;
the owning convergence reader retains its existing confirmation checks. They confer no ordinary
execution, publication, dispatch or cleanup custody.

`turn_items_candidate` supplies the ordinary owner-qualified forward page through candidate access.
The exact turn range, exclusive ordinal continuation, caller item/byte bounds, page byte accounting
and `has_more` semantics remain shared. An ordinal is a position, not a revision-bound authority;
consumers retain their existing surrounding state confirmation. Old or foreign storage handles
remain invalid, and the read does not release ordinary admission.

`history_summary_candidate` reads the ordinary exact bounded history summary through borrowed
candidate access for source-less recovery publication. It preserves the typed point-read contract;
the app owns surrounding turn/gate/summary stabilization and event eligibility.

## Provider-Operation Finalization Reads

`compaction_admission_read_candidate` shares the ordinary stabilized current-operation/admission
reader through borrowed candidate access. It preserves exact selected binding, CAS owner/membership,
gate and operation observations. Returned facts neither create an operation nor release ordinary
dispatch; any candidate convergence command retains the existing exact mutation contract.

`compaction_recovery_read_candidate` classifies one exact retained operation through borrowed
candidate access and the ordinary two-pass recovery reader. Gate/source, provider turn, snapshot
and consumed-receipt checks remain bounded and unchanged. Its result authorizes only the existing
exact durable convergence choice; it cannot recreate a dispatch claim or release publication.

`stop_admission_read_candidate` shares the ordinary two-pass stop-admission classifier through
explicit borrowed candidate recovery access. Exact target, selected route, provider-finalization
and retained live-stop observations keep their existing bounded authentication and drift/corruption
semantics. These facts may guide fenced candidate convergence; they do not release the startup
fence or authorize ordinary execution, provider dispatch or cleanup.

A compacting gate may still select its exact operation after provider terminal publication and
before final compaction settlement. The provider turn is terminal, but this is deferred compaction
authority rather than settled ordinary history. Delivery recovery retains its deferred-compaction
classification, and stop admission returns the existing compacting-ineligible result.

The bounded read authenticates the gate's operation nonce and provider turn against the exact
operation, parentless provider-turn record, execution snapshot and admitted binding. The operation's
finalizing state, terminal status and recorded turn-state revision must agree with the provider
turn state. It does not apply ordinary committed-tail or still-blocking-turn requirements to this
state. Mutable facts retain the existing stabilized-read contract; stable missing, substituted or
contradictory authority remains corruption. Reads neither consume finalization nor publish another
stop or dispatch capability.

## Non-Idle Gate Discovery

The package owns one compact current source for each non-idle input gate. Direct pending turns,
active turns, stopping, compaction, awaiting-terminal and repair-required gates participate under
their existing lifecycle rules. Idle gates have no source, including idle threads with accepted
next-turn work; the accepted-route source families continue to represent that queued work.

Gate and source membership/revision change in one atomic mutation and one reconciliation closure.
Creation of an idle thread leaves no source; terminal settlement or exact gate deletion removes
it when the gate becomes idle or absent. Mutation preparation validates the source belonging to
the exact current gate before changing it. Missing or disagreeing current authority is not repaired
implicitly. Explicit schema validation checks both directions of the gate/source relationship in
bounded pages; routine discovery does not enumerate gates to prove absence of an omitted source.

Public source pages are ordered by exact thread identity and bounded to at most 256 records and
65,536 stored encoded bytes. Smaller caller limits are honored; larger limits clamp. Their opaque
cursors carry the home identity, home generation, Syndic domain revision and last scanned thread.
Reads check the selected domain revision before and after the page. Foreign or stale-generation
cursors and mismatched revisions produce typed failure without publishing a partial page.

Consumers resolve a returned source through bounded exact current-gate and required turn/binding
reads under the applicable revision or source-anchor checks. Stable key, identity, revision or
membership disagreement is corruption; concurrent domain or selected-gate change remains drift.
Pages and point resolution contain compact facts only and retain no live execution authority.

A mutating recovery/scheduler traversal may explicitly rebase its existing forward bookmark to a
fresh domain revision in the same explicitly admitted home generation, through ordinary healthy
access or unpublished candidate recovery access. Rebase grants no carried candidate
authority or consistent whole-scan snapshot. The owning traversal must restart from the beginning
when a change can create eligible work behind that bookmark. Startup convergence fences new
execution admission while it consumes its sources; live scheduling retains its existing typed
fresh-scan wakes. Neither traversal uses broad input-gate or history-family scans.

`delivery_recovery_startup_page_candidate` and
`rebase_delivery_recovery_startup_cursor_candidate` accept explicit borrowed home-store candidate
recovery access. They share compact-source traversal, exact gate resolution, revision checks,
cursor identity and item/byte limits with the ordinary methods. Returned sources are discovery
facts only; classification, convergence and service publication retain their separate authority.

`classify_delivery_recovery_candidate` consumes one such source through the same stabilized bounded
classifier as ordinary recovery. It preserves home/generation and selected-source fences, two-pass
dependent facts, provider-stop authentication and the distinction between drift and stable corruption.
Its result guides exact fenced convergence only; publication and ordinary execution remain separate.

## Privacy And Diagnostics

Persisted provider observations retain normalized kinds, identifiers, timing, counters, digests, and
references to bounded sealed ranges. They exclude access tokens, refresh tokens, API keys, cookies,
bearer headers, listener capabilities, raw request headers, and complete provider transport bodies.

Public errors and diagnostics identify records by typed identity, family, revision, lifecycle, count,
and redacted digest summaries. They do not include draft text, transcript content, resource bytes,
context selections, secrets, or unredacted provider payloads. Explicit content APIs are the only
boundary for sensitive bytes.
