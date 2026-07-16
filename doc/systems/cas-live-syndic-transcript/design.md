# Goals

Capture live Codex App Server turns into Syndic durable history and execute new Syndic work through one exclusive CAS projection per Syndic thread.

Preserve CAS as the live execution, authentication, sandbox, approval, skill, MCP, subagent, and enterprise-policy authority while making Syndic the only durable history source for captured threads.

Prefer CAS-native thread continuation, resume, and fork lineage whenever exact binding proof shows that CAS already owns the required parent context. Recover only stale, lost, unavailable, or unprovable CAS lineage by injecting the required Syndic context once into a fresh CAS thread without modifying CAS or sharing one CAS thread between Syndic threads.

## Non-goals

- Importing or backfilling CAS historical transcript data.
- Using CAS thread lists, names, metadata reads, or historical reads for Beryl catalog, title, restore, or transcript authority.
- Replacing CAS execution or policy behavior.
- Mutating submitted Syndic turn parentage to match a CAS operation.
- Summarizing or silently truncating history that does not fit a recovery budget.
- Injecting or replaying Syndic history when exact CAS-native lineage already owns the required context.
- Repeating recovered history on later turns or steering requests after one successful injection.
- Encoding recovered history in ordinary user input, developer instructions, or `additionalContext`.
- Deleting incomplete or unreachable Syndic turns as part of stop, edit, thread deletion, or projection cleanup.

# Decisions

## Ownership Split

- Syndic owns threads, current drafts, submitted turns, immutable parentage, accepted input, canonical events, transcript projections, resource metadata, and CAS projection-binding records.
- CAS owns live execution and provider policy for CAS-backed turns.
- Beryl-home metadata owns execution bindings, presentation metadata, window claims, settings, and host-orchestration jobs.
- Beryl shell coordinates typed storage commands and normalized backend requests without becoming another durable history owner.
- Transcript rendering reads only the Syndic provider boundary.

## Targeted CAS Contract

- Beryl targets `codex-cli 0.144.1` / Codex App Server 0.144.1 as its single app-server contract.
- Compatibility admission requires an exact 0.144.1 initialize-version match and non-destructive typed probes for exact thread continuation, resume, fork, rollback, turn start, steering, interruption, compaction, subscription cleanup, configuration/model inputs, the canonical conversation-tool profile, and stable `thread/inject_items`.
- Every persistent Beryl conversation CAS lineage receives one canonical versioned and
  deterministically ordered conversation-tool registry at its initial `thread/start`. The exact
  0.144.1 evidence must prove that native inclusive fork and process restart/resume retain
  byte-identical provider-visible definitions. A failed proof blocks native cache-stable branching;
  it does not authorize routine history reconstruction or per-turn tool replay.
- Conversation-tool profile V1 is SHA-256 over the exact compact UTF-8 JSON array emitted in Beryl's
  deterministic registration order. Every entry uses the exact tagged 0.144.1 namespace/function
  schema; Beryl neither emits the legacy flat compatibility form nor mixes representations. A
  deliberate whole-registry encoding change requires a new profile version and makes older
  bindings ineligible for silent native reuse.
- `thread/inject_items` must append an ordered supported subset of raw Responses API items to one loaded idle thread's model-visible history without starting a model turn, so the next ordinary user turn observes those items before its real user input.
- The branch-selection channel is one canonical assistant-role/output-text raw message injected once through stable `thread/inject_items` after exact native fork or fresh-lineage establishment and before the first branch-local user turn. Its bounded Beryl frame precedes the exact selected assistant passage without changing those selected bytes.
- Reproducible native-lineage, dynamic-tool lineage, and injection evidence is recorded in
  `doc/memory/topic/codex-app-server/native-lineage-0.144.1.md`,
  `dynamic-tools-lineage-0.144.1.md`, and `thread-inject-items-0.144.1.md`; rejected
  additional-context evidence remains in the sibling `additional-context-0.144.1.md` and
  `additional-context-runtime-0.144.1.md` notes.
- Schema or method presence is not enough. Retained source-backed and focused live evidence for the pinned 0.144.1 release proves accepted recovery item shapes, role and content preservation, ordering, payload limits, idle-thread enforcement, later-request visibility, failure and ambiguity behavior, resume, fork, compaction, and the absence of an implicit model turn; each configured runtime must then match that exact release and pass the non-destructive typed request probes before recovery injection is enabled.
- Runtime admission never starts a synthetic model turn or mutates user history merely to repeat the pinned-release semantic proof. Upgrading the target release requires regenerating and rerunning that retained evidence together with the runtime probes and normalized boundary.
- If the configured CAS cannot satisfy the exact target contract, affected execution is unavailable. Beryl does not select an older request path.

## Exclusive CAS Projection Invariant

- One executing Syndic thread has at most one current CAS projection binding.
- One CAS thread id is bound to at most one Syndic thread. A reverse uniqueness record enforces this invariant durably.
- Every CAS thread admitted into a durable Syndic projection is persistent. Ephemeral CAS threads
  are reserved for separate bounded maintenance workflows and never receive a Syndic binding.
- Different Syndic threads may have simultaneous active turns through different exclusive CAS threads.
- One Syndic thread may have at most one active turn, compaction operation, replacement execution, or handoff execution at a time.
- Every pending, active, or unknown-terminal turn remains the committed tail of its origin thread until it reaches a proven terminal state. Reopen rejects any other blocking turn, so the unique committed tail is the durable same-thread execution gate even before a CAS turn id exists.
- Thread activation and history browsing never resume or enumerate CAS merely to prove that a binding exists.

## Binding Records

- A Syndic-owned binding record is keyed by Syndic thread id and binding revision.
- Binding status is `unbound`, `valid`, `active`, or `stale`.
- Every record stores the exact current Syndic selected-path proof used to classify it. That proof
  is distinct from the exact prefix already represented by CAS: after submission the selected tail
  is the pending turn, while a pre-start CAS projection represents only that turn's parent prefix.
- A valid record stores exact runtime id, root path, CAS thread id, reverse-uniqueness proof,
  lineage mode, canonical conversation-tool profile version and digest, and a structurally
  distinct CAS-represented-prefix proof. For a pending turn the represented prefix is exactly its
  parent, or the canonical empty prefix for a root turn.
- Every usable binding also stores the exact cumulative number of actual CAS model turns in that
  represented prefix. This native count is structurally distinct from Syndic DAG depth: injected
  recovery items and provider-operation Syndic turns do not increment it.
- Lineage mode distinguishes CAS-native continuation or fork from a fresh lineage established through one completed recovery injection.
- Recovered-injection establishment provenance retains the exact injected prefix, sequence proof,
  completion time, managed CAS process generation, and loaded-thread generation independently of
  the represented prefix later advanced by ordinary CAS turns. It is not durable proof that CAS
  can reconstruct the injected prefix after losing that loaded session.
- An active record additionally stores the immutable execution snapshot id, start time, and the
  exact current same-thread input-gate correlation. The immutable snapshot stores the selected
  path, represented base prefix and its native CAS turn count, execution binding, and exact loaded
  process/thread generation; accepted input identities are not embedded in it.
- A CAS turn id that becomes known after active publication is a separate one-way exact
  correlation whose reverse index records the checked next native CAS turn count. Publishing it
  never rewrites the execution snapshot and rejects a different second identity.
- A stale record retains the prior execution binding, CAS id, any observed represented-prefix and
  lineage facts, exact generation facts when known, bounded stale reason, and timestamp only as
  provenance. It prohibits reuse for later execution.
- If a newly returned inclusive fork cannot publish valid authority, its first durable membership
  may instead retain the exact nonempty fork prefix and native position as terminal stale
  provenance. This does not authorize the fork for execution or permit another publication retry.
- An unbound record states that the Syndic thread has no usable CAS projection.
- Merely entering or cancelling replacement-edit mode changes only the current draft and leaves an exact CAS binding usable because the selected path is unchanged. Accepting the replacement changes the committed tail and publishes a new unbound binding for that path; losing exact CAS lineage or failing an exact resume proof likewise marks the applicable binding stale or unbound without mutating submitted Syndic history.
- CAS threads abandoned as stale are not deleted. Optional CAS archive cleanup requires a later proof that it cannot damage other backend relationships.

## Revisioned Input Gate And Live Routes

- Every Syndic thread has exactly one durable input-gate record selected directly by thread id. Its monotonic revision is independent of thread, draft, binding, and accepted-input revisions.
- The closed gate states are idle, pending-turn start, active steering, compaction, and stopping. Pending-turn start names the exact submitted tail that blocks another ordinary turn. Active steering stores one exact target proof containing the binding revision, execution snapshot id, Syndic turn id, CAS thread id, and the CAS turn id when known. Compaction and stopping are queue-only admission states.
- A CAS-turn id that is not known remains explicitly absent. No worker may infer or manufacture it. Publishing or replacing a steering target revision-checks the gate and updates the current active binding, snapshot correlation, and all affected bounded live routes atomically.
- Every input admission names the expected input-gate revision. The committed gate state determines exactly one outcome: idle submission creates a submitted turn; active steering creates a steering route; pending-turn, compaction, or stopping admission creates a next-turn route; a stale gate revision rejects the whole mutation.
- `accepted-order` is permanent retained history and may grow with the thread. Live steering and next-turn indexes contain only nonterminal delivery work. Delivered or terminally failed accepted input remains in retained order but is removed from every live-route index.
- Delivery-unknown is also a terminal accepted-input outcome. It records that one request was
  dispatched without an authoritative provider response, removes the fragment from every live
  route, preserves it in permanent accepted order and canonical history, and prohibits automatic
  replay.
- The input gate stores the accepted-order high-water mark plus exact live steering count, next-turn count, and logical UTF-8 byte total. Admission and reclassification use those counters rather than scanning retained history. Reopen validates the counters against the bounded live indexes and rejects disagreement.
- V1 permits at most 256 simultaneously live accepted fragments and 268,435,456 logical UTF-8 bytes across both live routes for one thread. Exact backend, selected-model, image, request, and worker limits may be lower. Overflow rejects before draft consumption or composer clear; these bounds do not limit retained accepted-input history or total thread turns.
- One retained accepted-input record carries the exact gate revision and steering-target proof or next-turn reason accepted at admission. Steering rejection, stop, compaction, or worker-capacity reclassification changes that record and its one live route under expected revisions without allocating a new accepted-input identity.
- Execution snapshots remain immutable exact execution facts. Accepted inputs refer to the snapshot through their target proof; an execution snapshot never contains an inline accepted-input vector or imposes a retained-history count ceiling.

## Submission Admission

- Submission first validates the selected thread, current draft revision, execution binding, store health, same-thread gates, CAS capability, local-image preparation, any required recovery budget, and pending resolution state.
- A validation or preparation failure leaves the current draft unchanged.
- Acceptance is one `SyncAll` home-store command that freezes the exact resolved text and image atoms into the appropriate durable lifecycle, moves every durable per-marker asset reference to that admitted owner, creates the replacement current draft, records exact ordering and idempotency identity, and marks required CAS delivery intent.
- The command names the expected sealed draft content as well as exact thread, draft, and input-gate revisions. Any mismatch rejects the whole command before draft consumption.
- An idle submission transitions the same draft identity into a submitted Syndic turn whose parent is the prior committed tail or immutable draft parent, advances the thread tail, and records the turn pending CAS execution.
- That pending turn owns exactly one sealed canonical user-input item while its finalized-item
  frontier remains zero. Finalization is terminal-only; execution preflight reads the sealed input
  without pretending that the pending turn is already recovery-complete.
- Submission during an ordinary active turn freezes the draft into an ordered accepted-input record for exact active-turn steering.
- Submission during compaction or when steering cannot be used freezes the draft into the bounded next-turn queue.
- Pending, steering, retryable, and next-turn queue states preserve one stable accepted-input identity. Reclassification or movement between those states never allocates a queued-input identity or duplicates the admitted fragment.
- Durable admission occurs before the composer clears, transcript-visible input appears, image-label protection advances, or a CAS request is sent.
- Once admitted, delivery failure does not fabricate CAS success and does not discard the accepted input. The record remains queued, retryable, explicitly failed, or delivery-unknown according to its exact lifecycle. Only proven pre-dispatch failure or exact provider rejection may authorize a later delivery attempt.
- Duplicate user activation or recovery first reconciles the draft-derived natural identity. An exact durable result is published as the original acceptance without replay; an absent result may be attempted under its original revisions; a collision blocks. Re-executing a consumed draft mutation is rejected and cannot create another turn or accepted fragment.
- Rotating only the current draft may advance the thread record revision without invalidating a binding whose observed selected-path tail and digest remain exact. Advancing or replacing the committed tail publishes a new unbound current binding for the pending path while retaining the prior binding revision as native-lineage evidence; it never claims that CAS already contains the undelivered turn.

## Live Turn Start And Identity Proof

- A valid idle binding starts the submitted turn on its exact CAS thread through ordinary `turn/start`.
- A branch or replacement whose required parent prefix is nonempty uses an inclusive CAS-native
  fork through the exact proven terminal CAS turn. The fork always creates a distinct CAS thread;
  it never mutates the source thread.
- A branch or replacement whose required parent prefix is empty starts one fresh native CAS thread.
  It does not roll an existing CAS thread back to zero and does not inject an empty recovery batch.
- Native resume preserves the exact native CAS turn count. Native fork seeds the new thread from
  the proven source CAS-turn position, while a fresh thread starts at zero. Syndic depth is never
  used as a substitute for an exact native CAS position.
- A resume establishment proof retains the exact durable source prefix that CAS resumed. The new
  binding may carry a later Syndic source-thread revision for the same stable tail and digest, but
  it never rewrites the older establishment event as though CAS had resumed from that later local
  revision.
- Production projection orchestration never calls in-place `thread/rollback`. That operation has
  no idempotency key, and a process crash after remote mutation but before durable valid-or-stale
  publication would permit unsafe historical-source rediscovery and repeated truncation.
- An unbound or stale thread, or a branch whose exact native parent lineage cannot be reused, first establishes a fresh recovered projection through one-time injection before ordinary `turn/start`.
- CAS acceptance binds the exact returned CAS turn id to the active Syndic turn and confirms the targeted CAS thread id.
- `turn/start` has no CAS idempotency key or authoritative delivery readback in the targeted
  contract. If its request may have been dispatched but its response cannot be classified, Beryl
  never repeats that start automatically. It retires the unprovable projection and, once the owning
  execution session is proven gone, closes local capture for the submitted turn as incomplete.
- Exact rejection or proven non-dispatch cancels the durable activation and leaves the submitted
  turn pending. The same loaded projection is reusable only when its exact connection and target
  authority also survive; a transport-level pre-dispatch proof may invalidate that authority, in
  which case Beryl reacquires a projection instead of fabricating a retained capability.
- A matching `turn/started` observed before a lost `turn/start` response proves the CAS turn
  identity but does not make delivery replay-safe. Because that event already crossed exact target
  routing, it authorizes `TurnActivated` even when the target closes before response classification.
  Beryl publishes that identity, captures any admitted prefix, and preserves completion-unknown as
  the controlling start classification if the target is then lost.
- If an exact returned turn id cannot confirm the live target because that target already observed
  a conflicting identity or closed, Beryl retains the returned correlation only as stale execution
  provenance and closes the submitted turn source-less. It does not emit `TurnActivated` or claim
  that live capture began on an unconfirmed target.
- Live stream events must carry matching CAS thread and turn identities before they update active state.
- The durable proof records CAS thread id, CAS turn id, Syndic thread id, submitted turn id, accepted-input ids, committed-tail digest, binding revision, runtime/root binding, lineage mode, and injected-prefix digest when present.
- A mismatched response or stream event is rejected from the selected projection and retained only as bounded diagnostic failure evidence.
- Exact response identity is enforced at the normalized backend boundary: resume and the retained
  low-level rollback primitive must return the requested CAS thread, fork must return a distinct
  CAS thread, and steering must return the expected active CAS turn. Retaining normalized rollback
  support does not authorize production projection orchestration to dispatch it.
- Projection cancellation is observed before each remote dispatch. Once a synchronous CAS request
  has been dispatched, Beryl drains and classifies that exact request and converges any returned
  target to durable valid-or-stale authority; cancellation cannot suppress a completed remote
  outcome or make it safe to reuse an uncommitted injection target.

## Live Event Capture

- Each admitted projection connection has exactly one bounded connection worker that exclusively owns the stream-capable backend session. The worker serializes request commands, polls only while no command is executing, and is the only consumer of that connection's normalized live-event stream.
- Projection admission requires proof that the initialized backend session retained the complete foreground notification profile. A request-only or selectively opted-out session cannot own a Syndic execution projection.
- A notification or server request observed before a matching JSON-RPC response is routed before that response becomes caller-visible. Request execution and live polling therefore share one connection-ordering boundary rather than competing transport readers.
- Connection commands receive a request-only capability that exposes no polling or buffered-drain
  operation. An interleaved approval request is admitted to the bounded FIFO before its immediate
  denial and is still routed to the exact turn target before the original response is published.
  The normalized approval states whether its response remains required or was already auto-denied;
  the session marks it auto-denied only after the denial write succeeds and rejects duplicate denial.
- The connection worker retains an exact request outcome separately from any later buffered-event routing failure. Whole-connection routing failure still retires connection authority and blocks ordinary result publication, but it cannot rewrite an already observed non-idempotent outcome into proven non-dispatch.
- A buffered failure confined to one registered target revokes only that target, but still blocks the
  matching command result and reports the exact target and close reason. Whole-connection routing
  failure retires the connection; a normal routed `thread/closed` retirement is not a routing failure.
- Before dispatching a turn, Beryl registers one exact provisional target using the connection generation, runtime and process generation, loaded-session generation, CAS thread, and Syndic owner. The first observed or returned CAS turn identity binds that target one way; a different later identity fails the target instead of being rerouted.
- Per-turn delivery queues have deterministic record and retained-byte bounds. Overflow, receiver loss, conflicting turn identity, or connection retirement closes the exact target with a typed outcome and revokes its loaded projection authority; no authoritative event is silently dropped or offered to another target.
- CAS events do not carry Beryl's loaded-session generation. After abnormal target retirement, the
  connection therefore fences that remote CAS thread from replacement registration instead of
  allowing an old-generation event to reach a new local target. The fence retains at most 256
  remote thread identities; capacity exhaustion retires the connection fail-closed. Proven-terminal
  sequential reuse must preserve the same loaded authority and cross an explicit ordered handoff.
- Account and bounded connection-lifecycle facts publish through one shared path keyed by exact
  runtime and managed-process generation, stamped with their source connection generation. Every
  connection to that process observes the same latest facts; they never enter a per-turn queue
  merely because one selected window currently displays that runtime.
- The ingester consumes normalized live events serially in source order and assigns monotonic
  per-turn sequence numbers.
- The durable normalized vocabulary is closed for the pinned public turn-item contract: turn
  activation, typed item start, typed bounded item delta, typed item completion, and status-only
  turn-ending outcome plus an independent optional typed history-incomplete reason. The outcome
  preserves exact provider or local execution terminal authority; the reason controls captured
  history completeness and never rewrites that outcome. CAS-backed item events must carry the exact
  active CAS thread, turn, and item tuple. Every admitted pinned public item variant retains an exact
  closed typed provider representation; a presentation-only activity disposition never substitutes
  for fields present in that representation. Unknown, malformed, or unresolved history-relevant
  input produces a typed unsupported-history outcome that prevents history-complete publication.
- One provider-created item owns one versioned `ProviderItemV1` content stream. Its immutable start,
  delta, and completion frames preserve field identity, order, optionality, indices, public
  lifecycle, and every admitted public value in the pinned normalized item union. Large strings and
  structured-value leaves occur once in bounded content chunks; bounded source and canonical
  records carry exact frame references instead of inline copies.
- Both materialized and constant-resident frame paths emit one typed history-support result. That
  result accumulates monotonically across the item stream, so a retained unsupported observation
  such as Web-search `Other` cannot later authorize history-complete publication.
- Exhaustive normalized capture does not re-admit fields deliberately rejected at backend ingress.
  For standalone `ImageGeneration`, the upstream base64 `result` is transport-only and is discarded
  before a retained JSON value or normalized item exists. The admitted typed item contains identity,
  lifecycle timestamps, status, optional revised prompt, and optional `savedPath`; only those fields
  enter `ProviderItemV1`. Status is closed to the pinned `in_progress`, `failed`, and `completed`
  producer values, and completion cannot retain `in_progress`. A missing or empty path never
  activates a base64 fallback.
- MCP and dynamic-tool structured fields use a closed recursive value algebra for null, boolean,
  exact number, string, list, and ordered object values. Raw JSON, opaque payload blobs, ignored
  public fields, and generic future-variant escape hatches are not durable history authority.
  Explicitly typed image-byte payloads inside those surfaces may not be encoded as Fjall strings;
  they must cross the image-asset resource boundary or make captured history explicitly incomplete.
- The submitted user-message lifecycle correlates with Syndic's already durable user input and
  validates its exact identity and content. It never creates a second provider-authored copy of that
  input; its provider frame retains only exact provider metadata and a checked reference to the
  already sealed submitted content. A pinned item variant may be accepted on completion without a
  preceding start only when its exact normalized kind permits that lifecycle; CAS 0.144.1
  `SubAgentActivity` is such an instantaneous completion-only item and still retains its complete
  typed payload.
- Turn, item, assistant delta, item completion, terminal status, token usage, generated media, and supported operational events update only the exact Syndic turn and item identities they name.
- Each committed event writes its source record, canonical item/content changes, lifecycle/frontier changes, and transcript-staleness effect atomically. Bounded transcript/resource projection consumes that admitted canonical frontier separately.
- Live-source admission, binding publication, canonical finalization, item-projection advancement,
  and transcript advancement use one-domain writer-admitted commands. Their exact turn, gate,
  binding, item, build, and projection revisions remain mutation authority, while unrelated Syndic
  commits cannot conflict merely because they advanced the enclosing physical domain revision.
- Preflight, reconciliation, and convergence stabilize the exact thread, binding, gate, turn
  state, item, manifest, projection head/build/set, or transcript head/build records relevant to
  that operation. They neither wait for a globally quiet Syndic domain nor restart an unbounded
  whole-domain revision loop when another thread commits.
- Streaming assistant and supported command/file operational text, including any exact prefix
  already present at item start, is coalesced into provider-frame additions of at most 65,536 UTF-8
  bytes per staging command. The source event stores the resulting bounded typed frame reference,
  not a second inline text copy. Committed text remains exact and a crash may lose only an
  unpublished suffix.
- Frame-specific logical-text span indexes over the same provider content bytes expose only the
  authoritative snapshot's explicitly selected narrative or operational text fields used by
  canonical projection. A completion frame may revise an earlier delta-derived view while reusing
  unchanged byte ranges; stale delta bytes do not enter final narrative. Field boundaries and
  non-narrative provider metadata remain typed and durable without entering transcript narrative.
  Raw reasoning and unsupported item payloads are not converted into invented text records.
- Durable coalescing does not set visible streaming cadence. The sole routed stream owner also
  exposes each bounded normalized transcript-visible text delta as an ordered process-local live
  presentation fact for the exact target; this creates no second CAS consumer and grants no
  durable or recovery authority. The transcript host may publish that fact on its next GUI frame
  without synthetic character pacing, then relinquishes the matching transient prefix only after
  exact Syndic projection agreement.
- Every item-specific delta names the expected normalized item kind. Beryl validates that kind,
  exact item identity, and every bounded nonnegative protocol index before any durable text or
  resource mutation; a delta can never reinterpret an item created with another kind.
- One active capture retains only one at-most-65,536-byte pending delta, regardless of the number
  of active or completed provider items. Exact CAS-item index, canonical-item revision, owned
  provider-content manifest, typed frame frontier, and bounded logical text pages are the durable
  prefix and completion proof; process-local per-item digests or completed-item maps are not
  authority.
- Arbitrarily large item-start and item-completion observations are staged in bounded provider
  chunks while the published source/canonical frontier remains unchanged. One final writer command
  atomically publishes the sealed frame, its source event, canonical revision and lifecycle, and
  projection invalidation. A crash therefore leaves either the exact old frontier or the whole new
  event; unreachable staged bytes have no history authority. The authoritative completion frame may
  reference unchanged earlier ranges and append only changed fields, so final reconciliation does
  not duplicate large output already captured from deltas.
- Pinned CAS 0.144.1 `turn/completed` is a status-and-ordering fence only: it carries no item
  snapshot. For a normally finishing ordinary turn on one uninterrupted, fully subscribed
  foreground connection, the pinned source queues every preceding same-thread item lifecycle
  notification before that fence and the connection writes them in FIFO order. Beryl serially
  admits that stream, flushes its one pending delta, and scans already admitted durable item indexes
  in fixed bounded pages to require every completed item to name a sealed, structurally complete,
  kind-consistent final provider frame whose referenced content frontier is durable, and to classify
  any observed open, malformed, unsupported, or otherwise unresolved item before publishing the
  terminal status. The exact provider outcome is still published, while a
  typed history-incomplete reason keeps captured-history completeness behind when that audit is not
  clean. A provider-completed item may retain an explicit pending-resource disposition;
  that keeps its canonical finalization and history-complete frontiers behind without rewriting the
  provider item or turn lifecycle. The fence cannot enumerate,
  backfill, or repair an item notification Beryl did not receive, and thread-idle state is not a
  substitute terminal authority. Logical item count has no terminal-audit ceiling.
- Forced-abort terminal ordering is not part of that Phase 13 proof. Stop and interruption handling
  remains fail-closed until its later checkpoint proves that no item send can cross the terminal
  boundary.
- A routed dynamic-tool request is answered only through its exact owning live target. The feature
  handler receives the exact durable Syndic thread and turn context plus normalized CAS request;
  another target cannot respond, and the connection worker preserves response/event ordering.
- Protocol error, transport loss, subscription loss, worker failure, process exit, or app shutdown
  before proven terminal completion leaves the submitted turn durable with explicit incomplete,
  failed, interrupted, or unknown-terminal state. CAS 0.144.1 has no notification cursor or replay:
  reconnect, late subscription, resume, and process restart cannot repair the capture. A replacement
  connection is never resumed into the same authoritative live-capture target. Unknown-terminal
  remains open only while exact late evidence is still possible; proven loss of the owning process
  or loaded execution session retires the projection and permits source-less incomplete convergence.
- Late provider events may be admitted only before proven-terminal publication and only when idempotency and sequence checks allow. After proven-terminal publication, bounded work may finish stale or incomplete canonical and projection frontiers solely from source events that were already admitted; no later source event is accepted. Once that work becomes current it is finalized, and no later event can mutate the turn's canonical items, projections, resources, ordering, parent edges, thread bindings, or selected paths.
- An exact retry at an occupied source sequence is recognized as already durably admitted before stale event-local revisions are considered. Different content at that sequence is a collision, and a sequence gap is rejected; none of these cases rewrites the stored event.
- Under the pinned CAS 0.144.1 supported producer contract, hosted Responses image generation is
  not client-reachable because CAS cannot send the required native `image_generation` tool
  declaration. Its parser support is receive/history tolerance, not an admitted producer. The
  standalone `image_gen.imagegen` extension is a separate admitted producer whose generated-media
  lifecycle must be preserved. A custom provider that injects an unsolicited hosted item is
  nonconforming and outside the supported runtime contract; parser tolerance does not give Beryl a
  complete-history guarantee for that provider behavior.
- The release-scoped proofs are retained in that commit's
  `notification-ordering.md`, `item-lifecycle-coverage.md`,
  `reconnect-notification-replay.md`, and `hosted-image-generation-reachability.md` memory notes
  under `doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/`.

## Pinned Public Item Dispositions

- `UserMessage` correlates with the exact already durable submitted or accepted input. Provider
  lifecycle validates it and never creates duplicate user authorship.
- `HookPrompt` is exact provider-generated user-role model-history text. It remains distinct from
  ordinary user-authored input and from transcript narrative.
- `AgentMessage` and `Plan` retain exact canonical assistant text, source phase, visibility, and
  type. Plan text is not flattened into an ordinary agent-message item.
- `Reasoning` retains only the exact permitted activity lifecycle and backend-provided summary
  surface. Raw or encrypted reasoning content is neither exposed nor converted into invented
  canonical text.
- `CommandExecution` and `FileChange` retain typed operational identity, lifecycle, bounded
  metadata, and arbitrarily large public text or change payload through chunked canonical content.
  They remain outside transcript narrative.
- `McpToolCall`, `DynamicToolCall`, `CollabAgentToolCall`, `SubAgentActivity`, `WebSearch`, and
  `Sleep` retain exact typed public operational/activity fields and chunked public payload where
  present. `SubAgentActivity` explicitly permits completion without start.
- `ImageView` retains a typed operational media reference. `ImageGeneration` from the admitted
  standalone extension retains its exact non-base64 metadata and one `savedPath`-located
  generated-media reference intended for assistant output. Provider item completion is preserved
  immediately, but canonical resource finalization and history completeness wait until Beryl reads
  that exact runtime-local file and resolves its bytes into Beryl-owned asset authority. A runtime
  path alone is not a durable asset, and an absent or unusable path has no inline-result fallback.
- `EnteredReviewMode`, `ExitedReviewMode`, and `ContextCompaction` retain exact typed activity or
  provider-operation markers and never become invented conversation text.
- Any unknown variant, malformed required field, impossible lifecycle, delta-kind mismatch, or item
  whose required canonical/resource payload cannot be durably represented or explicitly retained
  as pending closes captured history with an explicit typed incomplete reason. A later exact
  provider terminal outcome remains exact and independent; the item cannot be ignored while history
  is published complete.

## Branch Selection Context

- The transcript's synthetic discussion-context group and the one-time CAS branch-selection projection derive independently from the same immutable Syndic envelope. Rendering the group never creates CAS input, and assembling CAS context never reads rendered text.
- Exact selected discussion text remains untrusted model-visible context. It never becomes ordinary user-authored input or developer instructions merely because Beryl stored or projected it.
- Because the selected passage originated in an assistant reply, its canonical CAS projection preserves that provenance as exactly one assistant-role message containing exactly one output-text item. This avoids fabricating a hidden user-turn boundary and does not rely on CAS-private contextual-wrapper recognition.
- The bounded Beryl frame identifies the projection version, source role, selected UTF-8 byte length, selected-text digest, and durable provenance identity before the unmodified selected bytes. Beryl framing is descriptive assistant-history context, not an instruction channel.
- The context is projected once while establishing the discussion's CAS lineage and is never resent on later turns or steering requests.
- The selected-context mechanism must preserve the complete accepted value, provenance framing, ordering, and trust semantics through one proven targeted CAS boundary. If no such boundary supports the accepted limit exactly, branch execution remains unavailable and the architecture blocks for review.

## Recovery Item Projection

- Recovery injection projects the complete required committed Syndic path into an ordered, versioned sequence of supported raw Responses API items. It does not wrap the conversation in one JSON or prose blob.
- Beryl admits only a closed canonical subset: user-role messages with exactly one input-text item and assistant-role messages with exactly one output-text item. Unknown raw item types, arbitrary roles, extra fields, tool records, images, and CAS-private wrapper conventions are rejected unless later target authority adds a separate exact proof.
- Every admitted recovery text item contains at least one UTF-8 byte. An empty item is not a lossless representation of unsupported or missing source content, so a path that would require one is unavailable instead of being projected as an empty message.
- User-authored history remains user-role history. Assistant commentary and final output remain assistant-role history. Beryl never promotes recovered user or model content to developer instructions.
- Required model-visible provider operation and tool records are preserved only through exact supported raw item shapes. If a required item has no proven lossless target representation, recovery is unavailable rather than approximated.
- The projection excludes the current submitted input, hidden developer instructions, raw reasoning, activity-only summaries, authentication, policy-private fields, diagnostics, and resource bytes.
- Heavy resources are represented only through the exact model-visible references or labels supported by both canonical Syndic history and the targeted CAS item contract.
- Injected items are synthetic only inside the disposable CAS execution projection. They create no Syndic turn, transcript record, user-authored draft, or Beryl-home catalog item.
- The binding proof stores the projection version, canonical item-sequence digest, source tail id,
  source revision, byte count, item count, injected CAS thread id, and the app's local Unix
  wall-clock observation taken immediately after exact injection success. Clock conversion failure
  abandons the target rather than substituting request time or a clamped value.
- The injected-prefix proof remains establishment provenance after later native CAS turns advance
  the binding's represented prefix; the two facts are never conflated.
- Assembly reads immutable turn topology, canonical item indexes, and exact logical content ranges
  directly from Syndic under one stable domain revision. It never uses the selected transcript
  head, rendered Markdown, or the whole-encoded composer assembler as recovery authority.
- Root-to-tail ordering may retain only a deterministic recovery-bounded identity frontier and the
  final bounded item sequence. It stops as soon as the independent item or byte ceiling proves the
  complete path unavailable; it never materializes an unbounded thread or canonical item.

## Recovery Budget

- The complete recovery item projection is accepted only when its canonical UTF-8 payload is no greater than both 262,144 bytes and one half of the exact selected model context-window token count interpreted conservatively as a byte count.
- One recovery projection contains at most 262,144 items. Because every item is nonempty, this explicit allocation and wire-shape ceiling follows from the independent 262,144-byte canonical-text ceiling and does not impose a smaller turn-count limit.
- Branch context, current user input, developer instructions, registered tool schemas, and normal CAS overhead are budgeted separately and must leave at least the other half of the known model context window available.
- One selected discussion context entry is limited to 65,536 UTF-8 bytes before branch creation is accepted.
- Beryl does not summarize, omit older turns, truncate items, split one logical history into repeated per-turn fragments, or silently drop media labels to satisfy these limits.
- If the complete required recovery projection or selected context exceeds its proven channel limit, execution or branch creation rejects before draft admission and preserves the user's current state.
- Missing exact model context-window metadata makes fresh recovery unavailable rather than causing Beryl to guess a budget.

## Native Lineage Precedence

- An exact valid CAS binding continues on its existing CAS thread and sends no recovered Syndic history.
- A branch, replacement, or resume uses CAS-native inherited parent context whenever exact Syndic parentage and binding proof map to the required CAS lineage and canonical conversation-tool profile.
- Recovery injection is a resilience fallback only for missing, stale, unavailable, or unprovable native lineage. Implementation convenience is not a reason to select it.
- Beryl retires a native binding and establishes a fresh recovered projection only from authoritative
  proof that the exact source can no longer supply the requested lineage. A method-specific
  machine-readable CAS outcome or a locally proven loss already defined by this system may supply
  that proof; a generic request error, its code, its message text, or repetition count may not.
- A source-preserving or unclassified native resume/fork rejection retains the exact source binding
  and enters bounded automatic retry against that source. Retry exhaustion keeps the binding and
  publishes an exact recovery-decision-required outcome; it never silently converts the attempt
  into recovery injection.
- Retry from that outcome targets the same exact source and request proof. Recover from Syndic
  history is a separate explicit authority that revision-checks the retained source and establishes
  one fresh recovered target projection. When the source is the target thread's own binding, it is
  retired first. When a fork source belongs to another Syndic thread, the target recovers without
  mutating that source thread; it can independently prove or recover its own lineage later. A stale
  or mismatched recovery command rejects without changing either binding. Target retirement may
  advance the exact decision basis by one binding revision only; bounded replanning that observes
  any later revision rejects instead of inheriting authority from a concurrent mutation.
- Speculative warm-up may remember recovery-decision-required state but does not itself interrupt
  draft editing or acquire GUI focus. The decision gates execution when submission or an already
  admitted pending turn actually requires the projection.
- Explicit abandon-and-recover is unavailable while an active or unknown-terminal CAS turn remains
  unresolved. Beryl never uses this decision to replay an input whose CAS activation may already
  have occurred.
- Once a recovered CAS projection is established, its later turns continue through ordinary native CAS history while the exact loaded CAS session remains proven. Beryl never sends the injected prefix again to that CAS thread.
- Losing the managed process generation, loaded-thread proof, or exact recovered binding makes that projection stale even if CAS could ordinarily resume its rollout. Beryl creates another fresh projection rather than depending on unobservable injection persistence.

## Fresh Projection Recovery Injection

- Recovery injection requires an opaque compatibility admission that owns the exact initialized
  backend client session it probed and binds that session to the selected runtime and managed CAS
  process generation. A detached report or admission from another session, runtime, or generation
  is not authority to inject.
- Recovery creates a new empty loaded CAS thread in the selected execution binding, establishes all required thread-level initial context, proves the thread idle, calls `thread/inject_items` once with the ordered recovery item projection, and waits for successful completion before starting the pending submitted turn through ordinary `turn/start`.
- Beryl requests metadata-only lineage results, including `excludeTurns = true` where CAS provides it, and exposes only bounded identity/status/metadata projections from lineage and turn-start control responses. Incidental historical turn or item bodies never become caller-visible history or replace Syndic reads; a response that exceeds the transport-wide message-byte ceiling fails closed.
- Injection itself starts no model turn and supplies no current user input.
- Successful injection establishes the in-memory model-visible prefix for that exact loaded CAS session. The resulting binding records the exact injected Syndic prefix and session generation, then treats later CAS-produced suffixes as ordinary native lineage only inside that proven session.
- A recovered-lineage turn cannot activate before the recorded injection completion time and must
  use the exact loaded process/thread generation named by that proof.
- A failed injection never falls back to user input, developer instructions, `additionalContext`, chunked replay, truncation, or summary. The admitted Syndic turn remains durable and explicitly pending, retryable, or failed.
- Success, structured rejection, transport loss, and unknown completion all consume the one-use fresh-idle injection capability. Any non-success causes that fresh CAS thread to be abandoned; recovery may create another fresh thread rather than risking a second injection into the same thread.
- Beryl publishes a usable binding only after injection success and the durable local proof commit. That proof authorizes the exact loaded session, not CAS-rollout durability. An abandoned, unloaded, resumed-after-loss, or uncommitted CAS thread id remains provenance only and is never reused as a valid recovered projection.
- Public CAS thread reads are not injection readback and cannot turn an ambiguous delivery into proof. Beryl never retries injection in place.
- CAS threads abandoned during recovery are not deleted. Later cleanup requires the future garbage-collection design.

## Loaded Projection Leases

- Process-local loaded-thread authority is represented by an explicit subscription lease owned by
  the exact connection generation, runtime id, managed-process generation, CAS thread id, Syndic
  thread id, and loaded generation. A map entry, another connection to the same process, or a
  durable CAS thread id alone is not evidence that a recovered injected prefix remains loaded.
- Loaded leases are bounded by actual live coordinator ownership rather than an arbitrary durable
  thread-count ceiling. Releasing the last owner performs the exact CAS unsubscribe choreography,
  removes process-local authority, and prevents later recovered use until a new projection is
  established.
- Explicit consuming release removes local authority before its bounded unsubscribe request; every
  unsubscribe status or error remains non-authorizing. Implicit drop performs no backend I/O. It
  removes the exact token and retires the connection when that forgotten token was the last owner,
  so GPUI cannot block on cleanup and no untracked subscription stays reusable.
- One process-owned connection service may hold multiple exact per-thread leases. A recovered
  source fork must execute through the source lease's owning connection and may create a distinct
  child lease there; a process-local registry observation never licenses dispatch through another
  connection.
- Connection retirement linearizes with only the bounded retired-check-plus-registry-acquisition
  section. If acquisition wins, retirement removes it; if retirement wins, acquisition rejects.
  Backend calls and storage work occur outside that gate, and no retired connection may insert a
  later loaded-thread entry.
- CAS subscription loss, `thread/closed`, connection loss, process replacement, or coordinator
  shutdown invalidates the matching loaded leases. Late events or releases from an older
  generation cannot remove or authorize a newer lease. On one still-live connection, abnormal
  target retirement fences the remote CAS thread from acquiring a newer target generation because
  the wire event itself cannot prove which local load produced it.
- Ordinary persistent native CAS lineage may later resume through its proven durable CAS contract.
  A recovered injected lineage cannot treat resume after loaded-lease loss as proof that its
  synthetic prefix survived; it becomes stale and uses fresh recovery only when execution is next
  required.

## Active Turn And Steering

- The active execution snapshot is immutable after CAS accepts the turn.
- While that turn is active, its binding retains the complete parent prefix as the represented
  committed prefix and names the active submitted turn separately. Only proven terminal capture
  carrying the exact one-way-published CAS thread and turn identity may publish a later valid
  binding whose represented prefix advances through that turn. A source-less terminal lifecycle
  update cannot create that external-authority claim.
- Process loss, loaded-session loss, or another proven loss of active CAS projection authority uses
  one atomic active-abandonment transition. It permanently retires that CAS thread, publishes its
  exact stale provenance, and distinguishes accepted-input delivery by dispatch evidence.
  Undispatched admitted or retryable steering fragments move to the ordered next-turn queue with
  the explicit projection-lost reason. A `Delivering` fragment whose request may have crossed the
  transport becomes terminal delivery-unknown, leaves every live route, remains durable
  accepted-input user history, and is never replayed automatically.
- Projection retirement and delivery-unknown publication are one atomic transition. No standalone
  accepted-input mutation may terminalize ambiguous delivery while leaving that CAS projection
  active, current, or steerable.
- A submitted turn whose `turn/start` was proven not dispatched remains pending and may be rebound
  through a fresh exclusive projection. Once `turn/start` may have crossed the transport, neither
  a missing response nor a missing activation event authorizes replay. Proven loss of its owning
  execution session first retires the projection; source-less terminal convergence can then close
  the turn as incomplete, retaining submitted input and any durably captured assistant prefix.
- Source-less activation, output, item completion, or successful completion is forbidden and never
  makes the stale projection usable or advances its represented prefix. A still-usable valid
  projection represents only the pending turn's parent; it must be retired to stale or replaced by
  an unbound record before source-less local incomplete convergence is admissible.
- Accepted steering records target the exact CAS thread id and expected active CAS turn id and preserve admission order.
- Steering responses must match the expected active turn. A non-steerable or stale-turn rejection changes the already delivering record to retryable and moves it, with its original accepted-input identity, to the ordered next-turn queue under the explicit steering-rejected reason without dropping, duplicating, or merging it.
- `turn/steer` has no CAS idempotency key or authoritative delivery readback. Transport loss,
  timeout, malformed response, or response-identity failure after possible dispatch is not a
  steering rejection and never becomes retryable work. It produces the delivery-unknown outcome
  above and makes the projection's represented history unprovable.
- Steering never repeats a recovered-history prefix or branch-selection context.
- Concurrent steering workers are bounded. Lack of worker capacity queues admitted input for the next turn rather than accumulating unbounded tasks.
- A stop request targets only exact CAS thread and turn ids. Stopping never deletes the Syndic turn; it converges to an explicit interrupted, incomplete, failed, or terminal state from observed evidence.
- Closing the owning main window requests interruption and waits for the durable lifecycle update required by the main-window contract before releasing the thread claim.

## Context Compaction

- CAS compaction remains a provider operation targeted at the exact exclusive CAS thread.
- Compaction records belong to a provider-operation Syndic turn or the active turn that owns the emitted compaction item; they are not standalone graph nodes.
- User input accepted during compaction is durably ordered in the next-turn queue and is not sent as active-turn steering.
- Compaction completion requires observed compaction activity followed by exact thread-idle state; request acceptance alone is not completion.
- CAS compacted internal history is not Syndic history and cannot become Beryl's durable-history authority. A still-valid CAS thread nevertheless continues through its own native compacted lineage without recovery injection.

## Replacement, Branching, And Retention

- Creating another Syndic thread from existing history has no immediate CAS effect.
- Exact inclusive CAS fork is used only when immutable Syndic parentage and durable binding proof
  map a nonempty requested path to an exact terminal native CAS prefix. An empty requested prefix
  uses a fresh native thread.
- Otherwise the affected Syndic thread becomes stale or unbound and uses one-time fresh recovery injection on its next execution.
- Replacement editing creates a new submitted turn from the edited turn's parent and atomically moves only the selected Syndic thread's tail/current-draft binding. It never detaches or rewrites the original turn.
- Unreachable turns, resources, projections, and abandoned CAS provenance remain durable until the future explicit garbage-collection design.

## Recovery And Idempotency

- Startup recovers admitted-but-undelivered input, active records without proven terminal events, stale CAS bindings, and pending stream ingestion from durable identities.
- Recovery retries only work proven not dispatched or exactly rejected. It never converts an
  ambiguous dispatched `turn/start` or `turn/steer` into retryable work. After incomplete
  convergence, fresh projection recovery may include the retained interrupted input as durable
  history context, but projection establishment itself starts no replacement model turn.
- Exact CAS resume may restore live control only when runtime, root, CAS thread id, Syndic thread id, reverse uniqueness, selected-path proof, and lineage mode all permit it. Losing the loaded session invalidates a recovered-injection binding even when the CAS thread id remains resumable.
- An authoritatively missing or unusable CAS source marks the binding stale and prepares one-time
  fresh recovery injection; it never causes a CAS history import. A source-preserving or
  unclassified resume/fork rejection retains the binding and follows the bounded retry policy
  instead.
- Every automatic or explicit retry remains correlated to the exact binding revision and
  projection request. A failed or ambiguous fork may leave an unbound CAS child, but no such child
  becomes Beryl authority; the retry bound limits unobservable orphan creation until future
  garbage collection exists.
- Delivery attempts, stream events, terminal updates, and binding transitions carry stable idempotency identities and expected revisions. Stream-event recovery distinguishes exact already-admitted identity from same-sequence collision and future-sequence ordering conflict.
- Recovery cannot start a second same-thread turn while an active or unknown-terminal record remains unresolved.

## Concurrency And Resource Bounds

- Same-thread submission, steering, edit, compaction, resolution handoff, stop, and binding transition pass through one revisioned thread-operation gate.
- Projection establishment additionally uses one process-wide bounded flight registry keyed by
  exact Beryl-home id, healthy home generation, and Syndic thread. Multiple coordinator instances
  cannot dispatch duplicate same-thread start, fork, or injection work; releasing the flight
  physically removes its process-local entry.
- Different threads share process-wide runtime and account projections without sharing active-turn ownership or stop targets.
- Live-event queues, accepted-input queues, steering tasks, retry sets, proof records, diagnostic payloads, recovery item sequences, and branch-context projections have deterministic count and byte bounds.
- Queue overflow rejects new admission before composer clear or durable state mutation.
- Quiet live streams remain active until a terminal event, transport failure, protocol error, or managed-process exit is observed.

## Security And Policy

- CAS remains responsible for authentication, ChatGPT workspace selection, managed configuration, enterprise policy, sandboxing, approvals, skills, MCP, tools, subagents, and rate limits.
- Recovery injection and branch-selection context never contain authentication material, loopback capability tokens, hidden developer instructions, policy-private fields, or raw approval payloads.
- Stored protocol errors and diagnostic evidence are bounded and redact secret-like values before durable commit.
- Syndic capture and recovery projection do not broaden CAS permissions or bypass policy decisions.
