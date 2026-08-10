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

## Documentation Set

- `doc/systems/bounded-resource-dataflow/design.md` owns the risk-based fixed-page, payload, queue,
  backpressure, concurrency, and transactional-streaming invariants consumed by CAS transport, app
  routing, Syndic staging, and live presentation.
- This document owns the exact CAS and Syndic semantic identities, event ordering, lifecycle,
  correlation, publication, recovery, and incomplete-history behavior carried through that
  substrate.

## Ownership Split

- Syndic owns threads, current drafts, submitted turns, immutable parentage, accepted input, canonical events, transcript projections, resource metadata, and CAS projection-binding records.
- CAS owns live execution and provider policy for CAS-backed turns.
- Syndic thread records own immutable execution bindings and intrinsic thread properties. Beryl-
  home application state owns runtime/root availability, window claims, settings, host-
  orchestration jobs, and rebuildable catalog copies.
- Beryl shell coordinates typed storage commands and normalized backend requests without becoming another durable history owner.
- Transcript rendering reads only the Syndic provider boundary.

## Targeted CAS Contract

- Beryl targets `codex-cli 0.146.0` / Codex App Server 0.146.0 as its single app-server contract.
- Compatibility admission requires an exact 0.146.0 initialize-version match and non-destructive
  typed probes for exact thread continuation, resume, fork, rollback, turn start, steering,
  interruption, compaction, subscription cleanup, configuration/model inputs, the canonical
  conversation-tool profile, stable `thread/inject_items`, and the effective native collaboration-
  tool profile.
- CAS owns subagent creation, lineage, scheduling, communication, and lifecycle. The native
  `spawn_agent` tool exposes optional bounded `model` and `reasoning_effort` inputs to the
  orchestrating model. Each explicit value precedes its configured subagent default. When neither
  resolves, the ordinary CAS-managed collaboration child retains the parent model and effort. A
  resolved effort alone is validated against the parent model; a resolved model without an effort
  uses that model's catalog default; and a resolved pair is validated together. The history seed
  selected through `fork_turns` does not constrain that profile selection, including for a full-
  history child. Beryl supplies orchestration instructions and observes normalized activity, but
  registers no imitation spawn tool and maintains no parallel child-agent registry. Compatibility
  admission requires the
  effective native tool profile to expose both inputs; exact executable version alone is
  insufficient when configuration disables them.
- Every persistent Beryl conversation CAS lineage receives one canonical versioned and
  deterministically ordered conversation-tool registry at its initial `thread/start`. Exact 0.146.0
  evidence must prove that native inclusive fork and process restart/resume retain
  byte-identical provider-visible definitions. A failed proof blocks native cache-stable branching;
  it does not authorize routine history reconstruction or per-turn tool replay.
- Conversation-tool profile V1 is SHA-256 over the exact compact UTF-8 JSON array emitted in Beryl's
  deterministic registration order. Every entry uses the exact tagged 0.146.0 namespace/function
  schema proven by the retained compatibility evidence; Beryl neither emits the legacy flat
  compatibility form nor mixes representations. A
  deliberate whole-registry encoding change requires a new profile version and makes older
  bindings ineligible for silent native reuse.
- `thread/inject_items` must append an ordered supported subset of raw Responses API items to one loaded idle thread's model-visible history without starting a model turn, so the next ordinary user turn observes those items before its real user input.
- The branch-selection channel is one canonical assistant-role/output-text raw message injected once through stable `thread/inject_items` after exact native fork or fresh-lineage establishment and before the first branch-local user turn. Its bounded Beryl frame precedes the exact selected assistant passage without changing those selected bytes.
- The prior 0.144.1 native-lineage, dynamic-tool-lineage, injection, and rejected additional-context
  evidence remains historical in
  `doc/memory/topic/codex-app-server/native-lineage-0.144.1.md`,
  `dynamic-tools-lineage-0.144.1.md`, and `thread-inject-items-0.144.1.md`; rejected
  additional-context evidence remains in the sibling `additional-context-0.144.1.md` and
  `additional-context-runtime-0.144.1.md` notes. It is not compatibility proof for 0.146.0.
- The source-backed 0.146.0 native-spawn exposure evidence is retained in
  `doc/memory/topic/codex-app-server/subagent-model-selection-0.146.0.md`. Complete current-release
  lineage, injection, ordering, and normalized-boundary proof must be retained before runtime
  admission is implemented.
- Prior-release steering request, rejection, response-before-lifecycle ordering, and client-
  correlation evidence remains historical in
  `doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/turn-steer-delivery-correlation.md`.
  Exact 0.146.0 evidence must refresh those proofs before steering admission.
- Schema or method presence is not enough. Retained source-backed and focused live evidence for the
  exact 0.146.0 release must prove accepted recovery item shapes, role and content preservation,
  ordering, payload limits, idle-thread enforcement, later-request visibility, failure and
  ambiguity behavior, resume, fork, compaction, and the absence of an implicit model turn; each
  configured runtime must then match that exact release and pass the non-destructive typed request
  probes before recovery injection is enabled.
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
- A valid record stores the exact immutable `ExecutionBinding`—runtime id, configured root id, and
  canonical runtime-native root path—plus CAS thread id, reverse-uniqueness proof, lineage mode,
  canonical conversation-tool profile version and digest, and a structurally distinct CAS-
  represented-prefix proof. For a pending turn the represented prefix is exactly its parent, or the
  canonical empty prefix for a root turn.
- Every usable binding also stores the exact cumulative number of actual CAS model turns in that
  represented prefix. This native count is structurally distinct from Syndic DAG depth: injected
  recovery items and provider-operation Syndic turns do not increment it.
- Lineage mode distinguishes CAS-native continuation or fork from a fresh lineage established through one completed recovery injection.
- Recovered-injection establishment provenance retains the exact injected prefix, sequence proof,
  completion time, managed CAS process generation, and loaded-thread generation independently of
  the represented prefix later advanced by ordinary CAS turns. Those generation facts identify
  where the injection occurred. The managed-process component remains an execution-authority
  boundary. The loaded-thread component may advance only through the exact overlapping
  same-process handoff defined below; ordinary lease loss or process replacement does not promote
  persisted rollout reconstruction into recovered-prefix authority.
- An active record additionally stores the immutable execution snapshot id, start time, and the
  exact current same-thread input-gate correlation. The immutable snapshot stores the selected
  path, represented base prefix and its native CAS turn count, execution binding, and exact loaded
  process/thread generation used by that execution. After an exact recovered-lineage handoff, the
  current thread-generation component may differ from the historical injection generation while
  the process component remains equal; accepted input identities are not embedded in the snapshot.
- Active binding status is ordinary conversation-execution authority. A provider operation instead
  keeps the represented-lineage binding valid and stores its distinct immutable provider-operation
  snapshot under the compacting record. The same-thread operation gate makes those two forms
  mutually exclusive; provider ingress never treats the valid binding alone as compaction
  authority.
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
- The closed gate states are idle, pending-turn start, active steering, awaiting terminal evidence,
  terminal-history finalization, compaction, and stopping. Pending-turn start names the exact
  submitted tail that blocks another ordinary turn. Active steering stores one exact target proof
  containing the binding revision, execution snapshot id, Syndic turn id, CAS thread id, and the
  CAS turn id when known. `AwaitingTerminal` names an active unknown-terminal turn and selects one
  route generation that retains that same exact steering target while admitting no further
  steering. Terminal-history finalization names the exact proven-terminal tail whose already
  admitted canonical and selected-transcript work has not yet reached a durable convergence fixed
  point. Awaiting-terminal, finalization, compaction, and stopping are queue-only admission states.
- A `Compacting` gate names the provider-operation turn and caller-generated operation nonce that
  select one exact current compaction record. The gate excludes ordinary execution and controls
  accepted-input routing; the record and provider-operation snapshot control remote dispatch and
  ingress. A gate without the matching record or a record outside its gate is coherent corruption.
- Entering `AwaitingTerminal` requires a source-backed unknown-terminal observation, an exact
  active binding and CAS turn, and a selected steering generation with no `Delivering` leaf. One
  atomic transition removes ready-source eligibility and makes every admitted or retryable member
  effective next-turn work under `UnknownTerminal` without rewriting leaves. The retained route
  target remains exact late-evidence and abandonment authority; it is not stop authority or a
  proven-terminal fact.
- A `Stopping` gate names the blocked exact operation and requires the matching current
  stop-operation record defined below. The gate is admission and scheduling authority; the record
  is interruption authority. A gate without that record, a record without that gate, or a target
  disagreement between them is coherent corruption rather than an invitation to infer or repeat
  an interrupt.
- A CAS-turn id that is not known remains explicitly absent. No worker may infer or manufacture it.
  Publishing or replacing a steering target revision-checks the gate and updates the current active
  binding, snapshot correlation, compact counters, and affected durable paged live routes atomically.
- Every input admission names the expected input-gate revision. The committed gate state determines
  exactly one outcome: idle submission creates a submitted turn; active steering creates a
  steering route; pending-turn, awaiting-terminal, terminal-history finalization, compaction, or
  stopping admission creates a next-turn route with its exact reason; a stale gate revision rejects
  the whole mutation.
- `accepted-order` is permanent retained history and may grow with the thread. Durable live steering
  and next-turn indexes contain only nonterminal delivery work and are read through bounded cursor
  pages. Delivered or terminally failed accepted input remains in retained order but is removed
  from every live-route index.
- Delivery-unknown is also a terminal accepted-input outcome. It records that one request was
  dispatched without an authoritative provider response, removes the fragment from every live
  route, preserves it in permanent accepted order and canonical history, and prohibits automatic
  replay.
- Promoted-to-turn is a terminal accepted-input route outcome distinct from provider delivery. It
  proves that Syndic moved the accepted content into one exact fresh pending ordinary turn and
  canonical item, removes the fragment from every live route, and preserves its accepted identity
  and order as the durable predecessor of that turn.
- A leaf-local retry or structured-rejection witness survives later whole-route stop,
  unknown-terminal, terminal-history, or projection-loss reclassification. Those transitions
  replace scheduling authority for the generation; they do not rewrite the accepted leaf's
  already-proven delivery history.
- The input gate stores the accepted-order high-water mark plus exact checked `u64` live steering
  count, next-turn count, and logical UTF-8 byte total. Admission and reclassification use those
  counters rather than scanning retained history. Reopen validates the counters through fixed
  cursor pages and rejects disagreement.
- Live-route logical count and bytes have no smaller Beryl memory-safety ceiling. Durable admission,
  scheduling, and delivery retain fixed resident pages and active worker slots regardless of
  backlog size. Exact backend or selected-model constraints may reject an individual dispatch, and
  checked `u64` exhaustion or durable-store failure rejects admission explicitly; neither case
  authorizes a whole-route collection.
- One retained accepted-input record is the permanent receipt for the exact source thread, draft,
  gate, content, assets, admission time, and caller-named replacement draft accepted by the
  command. Steering target or next-turn reason remains shared route-generation authority rather
  than copied admission intent. Unknown-terminal transition, closed structured steering rejection,
  stop, compaction, projection loss, or promotion changes selected route authority under expected
  revisions without allocating a new accepted-input identity. A rejection without a closed target
  verdict instead uses the atomic active-abandonment disposition defined below.
- Execution snapshots remain immutable exact execution facts. Accepted inputs refer to the snapshot through their target proof; an execution snapshot never contains an inline accepted-input vector or imposes a retained-history count ceiling.

## Submission Admission

- Submission first validates the selected thread, current draft revision, execution binding, store health, same-thread gates, CAS capability, local-image preparation, any required recovery budget, and pending resolution state.
- A validation or preparation failure leaves the current draft unchanged.
- Acceptance is one `SyncAll` home-store command that freezes the exact resolved text and image atoms
  into the appropriate durable lifecycle, rebinds one already sealed paged asset-reference set
  through compact owner heads, creates the replacement current draft, records exact ordering and
  idempotency identity, and marks required CAS delivery intent.
- The command names the expected sealed draft content as well as exact thread, draft, and input-gate revisions. Any mismatch rejects the whole command before draft consumption.
- An idle submission transitions the same draft identity into a submitted Syndic turn whose
  ordinary parent is the then-current committed tail selected inside the atomic admission,
  advances the thread tail, and records the turn pending CAS execution. Draft records carry no
  ordinary parent; branch-context and replacement submissions derive their exceptional parentage
  only from their explicit typed provenance.
- That pending turn owns exactly one sealed canonical user-input item while its finalized-item
  frontier remains zero. Finalization is terminal-only; execution preflight reads the sealed input
  without pretending that the pending turn is already recovery-complete.
- CAS input projection walks the immutable sealed composer pieces and their exact marker
  resolutions in bounded pages. At the first ordered occurrence of an image label, it appends the
  generated `Image X:` label to the current text run, emits that maximal nonempty run as one CAS
  `Text`, and then emits the exact image once as `LocalImage`. A later occurrence of the same label
  appends generated `[Image X]` text to the current run without resending the image. Storage-piece,
  source-page, JSON-encoder, and WebSocket-frame boundaries never create additional CAS input
  records, and no whitespace or delimiter is invented.
- Marker-resolution and asset-reference-set pages carry exact matching first-label-occurrence
  disposition, so projection never retains a seen-label set proportional to marker count.
- The projection moves no whole text value through an app worker command. One bounded source broker
  services exact absolute page requests from the connection worker while retaining the submitted
  content reference, marker ownership, runtime-path projection, cancellation, and store-health
  authority on the app side. It neither reopens the home nor creates a request spool.
- Each prepared text run receives one request-local unique broker source id and one immutable source
  proof. The private id selects the exact retained run descriptor; the proof independently binds
  that descriptor's logical bytes, declared length, and provenance for outbound and echoed-input
  validation. Caller-supplied proofs are never the broker's routing key, and equal proofs may not
  name different bytes, lengths, or backing ranges.
- A retained run descriptor maps source-relative offsets explicitly onto either a generated text
  fragment or an opaque Syndic segment proof plus its content-absolute range. Storage offsets never
  pass through the broker as though they were source-relative coordinates.
- Target preparation merge-joins Syndic content-marker pages with the sealed Beryl-state
  asset-reference set and retains compact source/cursor authority rather than one plan entry per
  physical chunk, span, marker, or image. Request encoding and both lifecycle echoes replay one
  descriptor at a time and validate the same count and sequence digest.
- Beryl-owned processes never replace or delete a referenced content-addressed sidecar. Under the
  Operator-selected trust boundary, arbitrary same-user filesystem tampering is external
  corruption rather than an actor Beryl guards against through CAS consumption. One verification
  handle remains live only for bounded length/digest verification and runtime-path derivation, then
  releases before cursor advancement. Whether exact CAS 0.146.0 still internally materializes or
  clones `Vec<UserInput>` is a release-scoped evidence question; Beryl's app, broker, encoder, and
  correlation layers
  still retain no proportional topology. See
  `doc/failures/cas-phase13-materialized-input-descriptors.md`.
- Submission during an ordinary active turn freezes the draft into an ordered accepted-input record for exact active-turn steering.
- Submission during compaction or when exact gate or target proof makes active steering ineligible
  freezes the draft into the durable paged next-turn route. Local scheduler saturation does not.
- Pending, steering, retryable, and next-turn queue states preserve one stable accepted-input identity. Reclassification or movement between those states never allocates a queued-input identity or duplicates the admitted fragment.
- Durable admission occurs before the composer clears, transcript-visible input appears, image-label protection advances, or a CAS request is sent.
- Once admitted, delivery failure does not fabricate CAS success and does not discard the accepted input. The record remains queued, retryable, explicitly failed, or delivery-unknown according to its exact lifecycle. Only proven pre-dispatch failure or exact provider rejection may authorize a later delivery attempt.
- Duplicate user activation or recovery first reconciles the draft-derived natural identity. An exact durable result is published as the original acceptance without replay; an absent result may be attempted under its original revisions; a collision blocks. Re-executing a consumed draft mutation is rejected and cannot create another turn or accepted fragment.
- Rotating only the current draft may advance the thread record revision without invalidating a binding whose observed selected-path tail and digest remain exact. Advancing or replacing the committed tail publishes a new unbound current binding for the pending path while retaining the prior binding revision as native-lineage evidence; it never claims that CAS already contains the undelivered turn.

## Accepted Next-Turn Promotion And Dispatch

- Syndic owns one durable accepted-input queue. CAS does not own a mirrored queue and Beryl does
  not try to keep two queued representations transactionally equal.
- Only when the input gate is idle, a revision-bound next-source cursor finds the earliest effective
  next-turn leaf without retaining the backlog. Its continuation advances across scanned source
  order even when a bounded page contains only terminal history.
- Before promotion, the process shell synchronously issues one non-cloneable scheduled-ordinary
  execution lease or reports that no exact execution is currently admissible. The lease binds the
  healthy Beryl-home generation and Syndic thread to one exact admitted CAS session and runtime
  generation, late-bound model, reasoning, developer-instruction, and turn-start policy, exact
  asset authority, feature-owned ordinary dynamic-tool handlers, and one long-lived worker permit.
  The provider retains no candidate, payload, task, waiter, or queue when it declines admission.
  Before returning an issued lease, the service revalidates its current healthy home generation,
  requires the supplied admitted session to be one still-attached connection registered by that
  exact service, reconfirms runtime and managed-process generation, and proves the supplied typed
  asset handle belongs to the owned home generation. Foreign, detached, retired, or generation-
  stale authority drops the provisional lease and performs no durable mutation or CAS request.
  Home shutdown joins the accepted-input scheduler workers, then requires the provider to fence
  issuance and release every idle or returned admitted-session checkout before the home is closed.
- Immediately before the durable promotion command, the scheduler acquires one non-cloneable
  promotion reservation from the exact service-owned connection named by the lease. Reservation
  acquisition rechecks service acceptance, connection ownership, attachment, runtime, and managed-
  process generation while serialized with service shutdown and connection retirement. It is the
  pre-promotion linearization point: shutdown or retirement that wins first rejects the reservation
  and leaves the accepted input queued, while a reservation that wins permits only that one
  promotion publication and fixed-work reconciliation. After reservation, the scheduler still
  revalidates exact home identity, healthy generation, and Syndic authority, but a later service-
  acceptance fence cannot revoke the winning reservation.
- The reservation retains no candidate or payload and holds no service-registry or connection-
  retirement mutex during storage work. A later retirement immediately fences new authority but
  defers final loaded-registry invalidation and connection detachment until the reservation is
  released. The scheduler releases it before projection establishment or any CAS request; the
  ordinary pending-turn path must therefore revalidate the connection after promotion.
- After global source discovery names a thread, one same-thread flight covers source-local
  candidate validation, execution-lease reservation, promotion, publication reconciliation,
  projection establishment, and ordinary dispatch. Releasing that flight coalesces a scheduler
  wake. A competing flight, unavailable or changed execution authority, worker saturation, or
  candidate drift performs no promotion and sends no CAS request.
- Worker completion always wakes the scheduler so it can join the finished worker and release its
  bounded bookkeeping. That wake alone does not authorize another next-turn pass. A completion
  opens immediate continuation only when its typed disposition proves that durable authority
  advanced or drift requires a fresh scan. Authority-unavailable, ambiguous `Prior`, cancellation,
  and retirement-lost outcomes park until fresh durable, execution, flight, cancellation,
  recovery, or genuinely awaited capacity authority arrives; they have no self-wake or timer. A
  parked accepted-next completion also cannot relay its released permit through the recovered-
  pending lane and back into a second accepted-next attempt.
- Scheduler wake facts are lane-specific. Ordinary completion, execution, projection-flight, and
  recovered-continuation wakes do not run a speculative steering scan. A steering scan's
  provisional reserve permit is not worker-capacity release authority; only a connection permit
  or a steering permit committed to an actual worker may satisfy scheduled-ordinary capacity
  demand. When one real release satisfies both steering and scheduled demand, both typed facts
  publish atomically.
- A redundant pass that reaches a Syndic thread already owned by an active next-turn worker stops
  without arming a same-thread flight waiter, so that worker's mandatory lease release cannot
  masquerade as fresh external flight authority. The worker pool likewise retains steering and
  scheduled-ordinary demand as separate facts inside one coalesced release waiter. Any permit may
  satisfy steering demand, but a scheduled-ordinary permit cannot satisfy scheduled-ordinary
  demand: only a connection release or a steering permit committed to an actual worker is fresh
  capacity for that role, while the scheduled worker's typed completion decides whether its own
  release permits continuation.
- A strictly newer healthy Beryl-home generation is not parked same-service unavailability. It
  invalidates the old scheduler's domain handles, connection, flight, execution lease, and every
  proof derived from them. The old-generation scheduler fences issuance, cancels and joins its
  workers, fails its obsolete service closed, and leaves any unpromoted accepted input durable for
  explicit new-generation recovery. It never relabels the old lease or resumes it on a readiness
  wake.
- Promotion is one durable pre-dispatch transaction. It requires the exact idle gate, selected
  thread tail, binding head, next-source generation/head, earliest effective leaf, and matching
  current-draft reverse binding. It creates fresh turn and item identities, parents the pending
  ordinary turn to the precommit tail, advances the selected path and pending gate, publishes the
  new unbound binding, updates transcript/summary/activity and live-route counters, and terminalizes
  the accepted route with an immutable successor witness.
- Transcript projection lifecycle does not gate promotion. An exact `Current` or rebuildable
  `Stale` transcript head for the selected tail and digest is accepted; the transaction supersedes
  any active build and advances that head to the new pending tail as `Stale`.
- A queued accepted-input admission may advance the broad thread revision without changing the
  selected tail or digest. It preserves an active transcript build or already-completed `Current`
  generation for that semantic path, advances the history summary to the new current thread
  revision, preserves derived completeness, and advances draft activity. Only a selected-path or
  canonical-source change supersedes the active build.
- The promoted item reuses the accepted input's sealed content and asset-set proof, while the
  accepted input remains permanent ordered history. The cross-domain home command moves only the
  compact asset owner from accepted input to submitted item. The current draft record, content,
  revision, and asset owner are neither read as parent authority nor mutated by promotion.
- Immediately after promotion, Syndic owns a pending turn whose parent prefix may be represented
  by CAS, but CAS is not yet proven to contain the new input. The current binding is unbound until
  exact projection establishment proves a usable projection for that parent prefix. Neither state
  is described as a second CAS queue.
- A known promotion-command conflict is benign authority drift and releases the unused execution
  lease. An ambiguous storage or persistence result is reconciled before any CAS work from the
  immutable terminal promotion witness, its exact successor identities, and current compatible
  monotonic descendants. A later valid accepted-input admission against the promoted pending gate
  may rotate the current draft, advance thread revision and image-label frontier, input-gate
  revision, accepted and next-turn aggregates, history summary, thread-parent revision, and new
  accepted-route authority without erasing proof of that promotion. The promoted selected tail,
  pending-turn identity, original terminal route witness, successor records, binding, transcript
  source, and activity source must remain exact or compatible with that admitted descendant. A
  current-draft save may independently advance only its draft revision and the matching summary
  activity time without invalidating the promotion.
  The cross-domain app read stabilizes only the accepted-input and submitted-item asset heads
  around that selective Syndic classifier; it never requires a quiet home revision.
  Reconciliation never requires the complete immediate post-promotion mutable shape to survive.
  Only `Exact` may continue; `Prior` stops without dispatch, while a changed witness, successor,
  incompatible lineage, partial publication, or unresolved state fails the scheduler closed as a
  collision.
- Dispatch begins only from that durable pending turn through ordinary `turn/start`. A failure
  proven before any request byte can cross the transport may retry the same pending turn after
  reacquiring exact projection authority. An exact provider rejection before acceptance may retry
  that same turn only under its existing rejection proof.
- Once any request byte may have crossed the transport, the pending turn is in a
  possible-dispatch interval. Loss in that interval never re-promotes the accepted input, creates
  another turn, or repeats `turn/start`; Beryl retires the unprovable projection and converges the
  one existing turn from durable dispatch and lifecycle evidence.
- An exact matching `turn/started` binds the pending Syndic turn to the exact CAS turn and begins
  live capture. Proven terminal completion advances the CAS-represented prefix to that turn. Thus
  Syndic promotion, possible dispatch, exact activation, and represented-prefix advancement are
  separate monotonic facts rather than two queues that can drift.

## Live Turn Start And Identity Proof

- A valid idle binding starts the submitted turn on its exact CAS thread through ordinary `turn/start`.
- JSON-RPC request encoding writes directly into a fixed-capacity transport sink. WebSocket uses
  one RFC 6455 text message composed of a Text frame followed by ordered Continuation frames and a
  final FIN frame, with a fresh client mask applied in place to each reusable frame buffer. Stdio
  uses an equivalent bounded buffer followed by exactly one newline. Neither transport constructs
  a whole request string or a second whole masked payload.
- A streamed text run opens one JSON string, escapes each bounded valid-UTF-8 source page into that
  same token, and closes it only at the run boundary. JSON and transport segmentation therefore
  cannot change CAS-visible `UserInput` structure.
- The paired streamed-input and echoed-lifecycle verifier is a production WebSocket capability.
  The retained detached stdio compatibility transport rejects this specialized start before any
  byte is written; a future live stdio path must first share the same session-owned incremental
  ingress rather than relying on its bounded whole-line reader.
- Before any transport byte is accepted, source, validation, serialization, masking, or
  cancellation failure is proven non-dispatch. After any request byte may have crossed the OS
  transport, every such failure is completion-unknown, the incomplete WebSocket message or stdio
  line poisons that client session, and Beryl never retries the non-idempotent request automatically.
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
- For streamed ordinary input, CAS acceptance becomes caller-usable only after both checked
  submitted-user lifecycle controls have crossed the ordered broker. The first such control binds
  the exact CAS turn and publishes `TurnActivated`; the exact response path proves that durable
  broker state before exposure and never publishes or reconciles activation itself.
- `turn/start` has no CAS idempotency key or authoritative delivery readback in the targeted
  contract. If its request may have been dispatched but its response cannot be classified, Beryl
  never repeats that start automatically. It retires the unprovable projection and, once the owning
  execution session is proven gone, closes local capture for the submitted turn as incomplete.
- Exact rejection or proven non-dispatch cancels the durable activation and leaves the submitted
  turn pending. The same loaded projection is reusable only when its exact connection and target
  authority also survive; a transport-level pre-dispatch proof may invalidate that authority, in
  which case Beryl reacquires a projection instead of fabricating a retained capability.
- A matching `turn/started` observed before a lost `turn/start` response proves the CAS turn
  identity but does not make delivery replay-safe. Before acknowledging that ordered control, the
  connection broker proves or publishes the exact active CAS identity and `TurnActivated` from the
  target's immutable pending authority, then records one broker-issued activation proof for the
  exact response wait. It does not enqueue or expose a generic start event. The caller preserves
  completion-unknown as the controlling start classification and holds no activation publication
  or reconciliation authority.
- If that target closes after binding a CAS turn but before start exposure, cleanup derives its
  gate, state, and source-event frontier from exact durable status: absent, active-only, or fully
  activated. The pre-publication frontier is valid only when no CAS turn was ever bound.
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

- Each admitted projection connection has exactly one bounded connection worker that exclusively
  owns the stream-capable backend session. The worker serializes request commands, polls only while
  no command is executing, and is the only consumer of that connection's ordered compact-control,
  approval, dynamic-tool-argument, and provider-observation boundary.
- The candidate session is created with immutable full-profile intent and configured parser, page,
  pre-bind control, queue, and concurrency limits before initialize reads its first byte. Until the
  app-owned ordered consumer is bound, eligible decoded compact controls occupy the session's
  fixed-capacity FIFO prefix through later reconciliation and acknowledgement; a full prefix
  closes the candidate. Permission approval is not eligible because an unbound session cannot
  establish the required durable stop owner.
- Projection admission requires proof that the initialized backend session retained the complete foreground notification profile. A request-only or selectively opted-out session cannot own a Syndic execution projection.
- A notification or server request observed before a matching JSON-RPC response is completely routed
  or staged and sealed before that response becomes caller-visible. Request execution and live
  polling therefore share one connection-ordering boundary rather than competing transport readers.
- A successfully denied command-execution or file-change approval's target-local presentation
  failure is handled as interleaved progress while another client request is awaiting its response.
  A permission approval may use that local disposition only when exact target closure proves a
  separate interruption unnecessary. Missing durable stop ownership is fatal and leaves permission
  response authority unexercised. None of these outcomes replaces, abandons, or authorizes retry of
  the enclosing client request; the sole session reader continues only while connection authority
  remains valid.
- Connection commands receive a command-only capability view with no polling or buffered-drain
  operation. An interleaved approval request is routed before denial and before the original
  response is published. Command-execution and file-change denial need no separate stop;
  permission routing must admit or join the exact durable stop and monotonically add its
  interrupting-approval cause before acknowledgement permits the denial write. The normalized
  approval states whether its response remains required, was denied, or owns the exact durable stop
  correlation and whether that stop's sole attempt was already possibly dispatched; duplicate
  denial is rejected. The event contains only bounded request, kind, route, response authority, and
  that compact correlation. Raw params, command, cwd, reason, and permission bodies are discarded
  during incremental normalization and never enter a FIFO or diagnostic string.
- The incremental envelope selector has no ordinary mode. Canonical method-first messages select a
  closed schema machine before payload. A pinned `id,result` response validates the id before its
  method-owned result, while an `error,id` response consumes fixed bounded rejection facts under
  the sole installed response expectation and validates the trailing id before publication. An id-
  first request-like or otherwise ambiguous prefix enters irreversible fixed-state quarantine,
  including on classification-prefix pressure. Quarantine retains only parser/classifier state,
  structurally discards values, and reports a late root request discriminator as an envelope-shape
  failure. Unknown notifications discard in order; unsupported server requests discard and then
  retire the connection. No prefix pressure, reordered field, or unknown method can activate raw
  capture, a root DOM, or reconstruction of discarded bytes. See
  `doc/failures/cas-phase25-late-approval-discriminator.md`. The response-order proof under
  `doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/`
  is prior-release history; exact 0.146.0 evidence must refresh it before admission.
- If the unbound bounded FIFO cannot admit an eligible approval or any earlier ordered message, the
  backend writes no pre-admission denial, releases the complete retained prefix and its accounting
  before any potentially blocking graceful close-frame write, and closes the exact transport. A
  typed capacity-full error without connection closure cannot
  preserve an unanswered server request or authorize later reuse of that session. If automatic
  denial fails after admission, the response disposition remains required while the same retirement
  boundary closes the transport and releases the now-unanswerable prefix.
- An unbound permission approval closes the candidate with its response authority unexercised. It
  cannot enter the prefix or be auto-denied because no durable stop owner or exact live target
  registration exists yet.
- Permission denial does not itself stop the active CAS turn. Exact route acknowledgement therefore
  deposits one bounded post-ack obligation correlated to the already admitted or joined stop
  operation, independently of the droppable target presentation event. The sole session-owning
  driver settles it only after the denial write succeeds and the ordered poll or enclosing client
  request returns, and before admitting another poll or client command. If no request byte from the
  stop's sole attempt crossed before denial, the driver claims or consumes that attempt and
  dispatches it once after denial. If the approval itself arrived while that exact attempt was
  already awaiting its response, the durable cause joins the in-flight cut; the driver waits for
  its result and never issues a second interruption. Rejection, nondispatch, or loss after
  interrupting-approval cause is present cannot safely reopen the target. Later target presentation
  close cannot cancel or redirect durable stop ownership. Command and file-change denials create no
  interruption obligation.
- Provider-broker cancellation racing approval admission cannot take the occupied interruption
  slot away from the sole ingester. Before stop ownership commits, cancellation returns fatal
  permission ownership without denial. After it commits, cancellation cannot erase the durable
  stop; connection retirement abandons the exact target without reconstructing or redirecting the
  moved approval request.
- Connection shutdown retires exact route authority and signals its independently owned broker
  before waiting for the runtime mutex that protects driver take and join. A foreground caller may
  hold that mutex while its driver is blocked on the broker acknowledgement, so cancellation cannot
  be placed behind the same wait.
- A reverse dynamic-tool request crosses the same connection-ordering boundary. Pinned compact
  thread, turn, call, namespace, and tool identity selects the exact installed feature sink before
  `arguments`; that sink consumes the argument structure under admitted backpressure and seals one
  typed request before a later response or notification becomes visible. Generic connection and
  target routing retain no argument value or cloneable request. Target loss or cancellation keeps
  one response owner and never reroutes the call to another Syndic thread.
- A dynamic-tool request observed before the ordered registry sink is bound fails the exact
  connection closed before `arguments`; it never enters the pre-bind compact prefix. Once a valid
  pinned envelope has selected its route, an unknown installed tool or feature-schema violation is
  sealed as one bounded typed rejection and answered once through that exact target. Reordered,
  duplicate, missing, or late-mutated envelope discriminants are compatibility failures that retire
  the connection rather than becoming tool responses.
- The connection worker retains an exact request outcome separately from any later buffered-event routing failure. Whole-connection routing failure still retires connection authority and blocks ordinary result publication, but it cannot rewrite an already observed non-idempotent outcome into proven non-dispatch.
- A buffered failure confined to one registered target revokes only that target, but still blocks the
  matching command result and reports the exact target and close reason. Whole-connection routing
  failure retires the connection; a normal routed `thread/closed` retirement is not a routing failure.
- Before dispatching a turn, Beryl registers one exact provisional target using the connection
  generation, runtime and process generation, loaded-session generation, current healthy home
  generation, CAS thread, Syndic owner, and immutable pending Syndic activation tuple. The first
  observed or returned CAS turn identity binds that target one way; a different later identity
  fails the target instead of being rerouted.
- Routed start publication holds a non-cloneable exact-target permit but no router lock during
  storage work. An ambiguous durability failure verifies only the same home generation before exact
  status classification. A close that wins before permit acquisition prevents publication; a
  permit acquired first delays final removal until publication reports, and the start is never
  exposed before both durable activation commands succeed or reconcile exact.
- Closed compact-control handoffs and feature-owned per-turn queues have deterministic admitted
  slot bounds, but they are not the provider ordering boundary. One independently progressing,
  home-bound, connection-scoped capacity-one broker receives compact controls and approvals,
  dynamic-tool argument operations,
  and provider operations in their exact wire order,
  transfers the sole admitted fragment into unpublished Syndic staging, and acknowledges completion.
  After emitting an operation, the backend does not advance later parser input, refill the admitted
  fixed parser window, or publish a later response before that acknowledgement. The fixed window may
  already contain bounded read-ahead. Local capacity exhaustion, receiver loss, conflicting turn
  identity, or connection retirement closes the exact target with a typed outcome and revokes its
  loaded projection authority; no authoritative observation is silently dropped or offered to
  another target.
- Connection admission starts the broker before ordered sink binding. Binding synchronously drains
  the bounded initialization/probe compact prefix through that broker before it returns; no bound
  poll can read a newer transport message while an older pre-bind control remains deferred.
- Pinned item wire order may expose a size-unbounded `params.item` before the sibling `threadId` or
  `turnId`. The broker therefore opens one connection-scoped unattached Syndic provider build,
  streams and validates typed fields into it, then obtains one non-cloneable publication permit and
  binds its compact sealed handle only after the trailing route matches the exact admitted
  connection lane. Seal is acknowledged only after the final ordered consumer publishes or fails;
  no receipt or sealed handle enters a FIFO. Missing, malformed, mismatched, cancelled, or retired
  routing leaves no published source event; unreachable staging remains non-authoritative future
  garbage-collection input. Once an exact route is proven, a sealed structurally valid observation
  that conflicts with the durable item lifecycle is itself authoritative source evidence and cannot
  be discarded as though routing had failed. See
  `doc/failures/cas-phase13-split-provider-control-ordering.md`.
- Provider begin snapshots the current healthy home generation and reacquired Syndic handle before
  opening unattached staging. Every control and fragment remains fenced to that exact authority;
  seal may bind and consume the observation only when the target publication permit names the same
  generation. A same-home recovery therefore invalidates the in-flight observation instead of
  reinterpreting its staging under later authority.
- The final seal consumer uses bounded durable replay to resolve the exact CAS item identity and
  lifecycle frontier. A legally admissible observation stages the typed provider frame without
  materializing the normalized item and atomically admits its source, canonical, lifecycle,
  activity, and projection-staleness effects. An exact-route observation that is structurally valid
  but inadmissible at that frontier instead atomically publishes a bounded provider-observation
  issue source event referencing the sealed observation, exact CAS item, and closed lifecycle
  conflict reason. That issue advances source order and durable history-incomplete state without
  replacing, completing, or otherwise mutating the canonical item. Either publication finishes
  before broker acknowledgement. Materialized item lifecycle and delta events cannot enter the
  generic target FIFO and provide no second publication path.
- The broker generates the build's caller-owned 128-bit identity once from the operating system
  cryptographic random source. It never rotates that identity after an ambiguous or advanced
  staging result. After an indeterminate store command, same-generation home verification precedes
  an exact build point read: an offered `Next` batch is accepted, `Expected` retries the same
  operation and identity, and `Conflict` retires the path. Generation-changing home recovery is a
  service lifecycle transition, never an implicit broker retry.
- CAS events do not carry Beryl's loaded-session generation. After abnormal target retirement, the
  connection therefore fences that remote CAS thread from replacement registration instead of
  allowing an old-generation event to reach a new local target. The fence retains at most 256
  remote thread identities; capacity exhaustion retires the connection fail-closed. Proven-terminal
  sequential reuse must preserve the same loaded authority and cross an explicit ordered handoff.
- Account and bounded connection-lifecycle facts publish through one shared path keyed by exact
  runtime and managed-process generation, stamped with their source connection generation. Every
  connection to that process observes the same latest facts; they never enter a per-turn queue
  merely because one selected window currently displays that runtime.
- Rate-limit payloads scan incrementally against the process owner's fixed admitted active-model
  interests. Ingress retains one bounded current bucket while the owner retains at most one exact
  match or ambiguity fact per interest; envelope completion publishes those compact results. No
  connection, process projection, or window retains the complete backend bucket collection.
- Compact controls form a closed typed ingress union and go directly to their final owner. Turn
  activation, checked-user lifecycle, terminal publication, thread close, account facts, token
  usage, and bounded agent metadata never enter a generic `TurnStreamEvent` or target presentation
  FIFO. A consumer that must outlive ordered acknowledgement occupies its owning fixed-capacity
  handoff until consumption; otherwise acknowledgement follows synchronous publication. Target
  queues are reserved for feature-owned approvals and dynamic-tool calls, not generic backend
  controls.
- Backend thread-name updates and incidental thread summary, path, preview, configuration, and
  history fields are structurally discarded. Subagent nickname and rate-limit identities use their
  owning bounded representable domains; an oversized required value leaves that metadata
  unavailable and cannot be truncated into a false identity.
- A consumed `thread/status/changed` or token-usage control carries only bounded route identity,
  closed status facts, a fixed recognized-active-flags bitset, and fixed-width required counters.
  Exact token usage publishes to the matching Syndic thread-usage record only after the immutable
  `ExecutionBinding`, current binding revision and CAS thread, managed-process generation, loaded-
  thread generation, connection generation, and monotonic provider-control ordinal authenticate the
  same thread; stale or mismatched controls are rejected. Unknown flags and incidental fields are
  discarded; a field with no final owner causes the whole control to be discarded rather than
  entering a generic queue.
- The ingester consumes compact control events or sealed streamed observations serially in source
  order and assigns monotonic per-turn sequence numbers only at atomic publication.
- Activation and source-publication permits retain the exact target's healthy home generation.
  Before storage work the broker proves that generation is still current and reacquires the typed
  Syndic handle for it; it never uses an admission-time handle after a loaded projection survives
  same-home recovery, and a later recovery race fails the expected-generation command closed.
- History-producing compact controls never delegate publication to the target FIFO. The ingester
  publishes checked submitted-user start, checked submitted-user completion, and status-only turn
  completion before acknowledging them; the target receives only one bounded proven-terminal
  outcome after terminal durability succeeds. Abnormal target loss waits for an acquired
  publication permit to resolve and converges a source-less incomplete outcome from the exact
  durable frontier. Ordinary execution retains no parallel source sequence, revision, or
  publication-time authority.
- The durable normalized vocabulary is closed for the pinned public turn-item contract: turn
  activation, typed item start, typed item delta, typed item completion, and status-only
  turn-ending outcome, plus a typed provider-observation issue for an exact-route sealed lifecycle
  conflict and an independent optional terminal history-incomplete reason. The terminal outcome
  preserves exact provider or local execution authority; history-incomplete facts control captured
  history completeness and never rewrite that outcome. CAS-backed item and issue events carry the
  exact active CAS thread, turn, and item tuple. Every admitted pinned public item variant retains
  an exact closed typed provider representation; a presentation-only activity disposition never
  substitutes for fields present in that representation. Unknown, malformed, or unresolved
  history-relevant input produces typed incomplete-history authority rather than permitting a
  history-complete publication.
- A provider-observation issue retains only a compact reference to an immutable sealed observation,
  its exact build identity and digest-covered frontier, exact CAS item identity, and a closed
  lifecycle-conflict reason. Its writer validates the referenced observation and proves that the
  current canonical frontier cannot legally admit it; a legally admissible observation cannot be
  relabeled as an issue. Structural observation sealing validates grammar, required fields, and
  internal status consistency but retains a structurally valid start for a completion-only kind;
  normal frame preparation rejects that lifecycle, while issue classification alone may publish
  `CompletionOnlyItemStarted`. The first issue remains a monotonic turn-state fact through normal
  terminal publication, reopen, and source-less loss convergence. Normal provider terminal
  publication uses `CompletionMismatch` when that fact is present. Source-less convergence retains
  its primary loss reason, such as `StreamLost`, while preserving the separate durable issue fact.
- One provider-created item owns one versioned `ProviderItemV1` content stream. Its immutable start,
  delta, and completion frames preserve field identity, order, optionality, indices, public
  lifecycle, and every admitted public value in the pinned normalized item union. Large strings and
  structured-value leaves occur once in bounded content chunks; bounded source and canonical
  records carry exact frame references instead of inline copies.
- The sole constant-resident provider-frame path emits one typed history-support result. That result
  accumulates monotonically across the item stream, so a retained unsupported observation such as
  Web-search `Other` cannot later authorize history-complete publication. There is no materialized
  provider-frame compatibility path.
- Unpublished Syndic staging independently validates the complete admitted normalized
  field-to-value grammar, including scalar and enum domains, text and container lifecycles,
  duplicates, indices, and completion. Wire-only fields that the backend deliberately consumes and
  discards at the privacy or resource boundary are not normalized fields and cannot be required or
  reconstructed by Syndic. Upstream backend validation is not a durable substitute for fields that
  do cross the normalized boundary. See
  `doc/failures/syndic-phase13-provider-presence-validation.md`.
- Web-search `Other` is the sole pinned lossy ingress exception. The backend emits its closed marker
  and structurally consumes the unknown action payload through fixed discard state; Syndic retains
  only the marker and typed unsupported-history evidence, never raw JSON, arbitrary field names, or
  a generic unknown-variant container.
- Exhaustive normalized capture does not re-admit fields deliberately rejected at backend ingress.
  For standalone `ImageGeneration`, the upstream base64 `result` is transport-only and is discarded
  before a retained JSON value or normalized item exists. The admitted typed item contains identity,
  lifecycle timestamps, status, optional revised prompt, and optional `savedPath`; only those fields
  enter `ProviderItemV1`. Status is closed to the pinned `in_progress`, `failed`, and `completed`
  producer values, and completion cannot retain `in_progress`. A missing or empty path never
  activates a base64 fallback.
- Raw `item/reasoning/textDelta` text is a private wire-only field. Backend ingress validates that
  the required JSON string is structurally complete while consuming it through fixed discard state;
  the normalized `ReasoningTextObserved` delta carries only exact item identity and
  `content_index`. No decoded reasoning-text byte, field lifecycle, page, durable chunk, diagnostic,
  or replay surface may cross into Syndic.
- A completed command-execution, file-change, MCP-tool, dynamic-tool, collaboration-tool, or
  standalone-image item cannot retain its kind's `in_progress` status. Backend ingress rejects the
  illegal normalized combination early, and Syndic independently revalidates it before sealing
  unpublished state.
- MCP and dynamic-tool structured fields use a closed recursive value algebra for null, boolean,
  exact number, string, list, and ordered object values. Raw JSON, opaque payload blobs, ignored
  public fields, and generic future-variant escape hatches are not durable history authority.
  Explicitly typed image-byte payloads inside those surfaces may not be encoded as Fjall strings;
  they must cross the image-asset resource boundary or make captured history explicitly incomplete.
- The submitted user-message lifecycle correlates with Syndic's already durable user input and
  validates its exact identity and content. It never creates a second provider-authored copy of that
  input; its provider frame retains only exact provider metadata and a checked reference to the
  already sealed submitted content. A pinned item variant may be accepted on completion without a
  preceding start only when its exact normalized kind permits that lifecycle. Compatibility
  admission requires retained exact-0.146.0 evidence that `SubAgentActivity` remains such an
  instantaneous completion-only item; it still retains its complete typed payload.
- Compatibility admission requires retained exact-0.146.0 evidence that CAS echoes the complete
  ordered `UserMessage.content` in both `item/started` and `item/completed`. The schema-specific
  ingress path consumes each text value incrementally and
  compares its bytes and typed boundaries against the exact submitted-input projection while the
  JSON stream is live. It never materializes the echoed text, a complete `UserInput` vector, or a
  whole lifecycle `serde_json::Value` merely to perform correlation, and the generic retained
  message ceiling is not a whole-user-input limit for this admitted path.
- WebSocket reads permit a protocol-sized lifecycle frame but retain only fixed payload/parser
  chunks; stdio is unavailable for live foreground capture until it presents newline-delimited
  lifecycle input through the same bounded decoder without first retaining the complete line. Every
  admitted pinned provider lifecycle and delta schema selects its incremental path before a
  size-unbounded field is retained. An unrecognized oversized message is rejected rather than
  gaining a general unbounded ingress exemption.
- Fresh turn-start correlation is request-scoped on the sole serialized connection worker.
  Immediately before writing the targeted `turn/start`, that worker installs the exact immutable
  submitted-input verifier. Compatibility admission requires retained exact-0.146.0 evidence that
  CAS synchronously publishes and awaits both complete user-message lifecycle notifications before
  returning that same start request, so no other start
  can share or replace the verifier. Because each lifecycle notification places
  `params.item.content` before its sibling thread and turn ids on the wire, ingress first performs
  the bounded exact content comparison and then validates the remaining envelope against the
  command target before publishing compact checked evidence. A lifecycle outside that start scope,
  a second start or completion, or a later identity mismatch fails closed. Delayed steering
  lifecycle instead uses the accepted-input correlation defined under Active Turn And Steering;
  neither path guesses a comparator from another target or substitutes a digest, spool, or
  whole-message buffer.
- User-message correlation requires the exact item count, variant at every position, text bytes,
  empty text-element semantics, local-image runtime path, image-detail semantics, client id, and
  order submitted by Beryl. Completion must name the same CAS item and reproduce the exact started
  content. Joining text, regrouping images, applying model-history defaults, accepting a digest
  without the bounded byte comparison, or using legacy/history reconstruction is an invariant
  violation that makes captured history incomplete.
- Turn, item, assistant delta, item completion, provider-observation issue, terminal status, token
  usage, generated media, and supported operational events update only the exact Syndic turn and
  item identities they name.
- Each committed event writes its source record and all applicable canonical item/content,
  lifecycle/frontier, history-incomplete, and transcript-staleness effects atomically. A
  provider-observation issue has no canonical item/content effect. Bounded transcript/resource
  projection consumes the admitted canonical frontier separately.
- Transcript visibility and Markdown projection ownership are distinct. A visible generated-media
  revision dirties transcript/resource presentation without fabricating or invalidating an item-text
  projection, while item-projection invalidation requires an actual canonical projection source.
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
- One selected provider narrative view over the same ProviderItemV1 bytes exposes only the
  transcript-visible text fields used by canonical projection. Start and coalesced delta frames
  append exact narrative selections to one item-owned generation; they do not repeat or replace its
  accepted prefix. For `AgentMessage` and `Plan`, completion does not select a second narrative or
  revise already admitted text. Its normalized narrative must equal the complete live narrative
  byte-for-byte, independent of provider or storage segmentation. Beryl proves that equality with
  bounded reads while staging the exact completion frame. An equal completion field may reuse the
  already stored narrative ranges without copying cumulative text.
- A completion/live narrative disagreement is a pinned-protocol invariant violation. Beryl retains
  the exact completion frame as provider evidence, leaves the append generation as the sole
  transcript source, records a typed history-incomplete reason, and never rewrites or guesses the
  narrative. The selected view retains bounded span and logical-byte frontiers plus one exact chain
  digest over ordered provenance and logical ranges. Field boundaries and non-narrative provider
  metadata remain typed and durable without entering transcript narrative. Raw reasoning and
  unsupported item payloads are not converted into invented text records.
- Durable coalescing does not set visible streaming cadence. The sole routed stream owner also
  exposes each arrived bounded fragment of a normalized transcript-visible text delta as an ordered
  process-local live presentation fact for the exact target; this creates no second CAS consumer,
  does not turn fragments into separate provider events, and grants no durable or recovery
  authority. The transcript host may publish that fact on its next GUI frame without synthetic
  character pacing, then relinquishes the matching transient prefix only after exact Syndic
  projection agreement.
- Every item-specific delta names the expected normalized item kind. Beryl validates that kind,
  exact item identity, and every bounded nonnegative protocol index before any durable text or
  resource mutation; a delta can never reinterpret an item created with another kind.
- One active capture retains only one at-most-65,536-byte pending delta fragment, regardless of the number
  of active or completed provider items. Exact CAS-item index, canonical-item revision, owned
  provider-content manifest, typed frame frontier, and bounded logical text pages are the durable
  prefix and completion proof; process-local per-item digests or completed-item maps are not
  authority.
- Arbitrarily large item-start and item-completion observations are staged in bounded provider
  chunks while the published source/canonical frontier remains unchanged. One final writer command
  atomically publishes the sealed frame, any exact narrative append or completion-equality result,
  its source event, canonical revision and lifecycle, and projection invalidation. A crash therefore
  leaves either the exact old frontier or the whole new event; unreachable staged bytes and
  narrative spans have no history authority. Completion's final public item remains exact evidence
  in the sole ProviderItemV1 stream without becoming a second presentation authority.
- Exact CAS 0.146.0 `turn/completed` may be admitted as a status-and-ordering fence only after
  retained release-scoped evidence proves that it carries no item snapshot and that, for a normally
  finishing ordinary turn on one uninterrupted fully subscribed foreground connection, the source
  queues every preceding same-thread item lifecycle
  notification before that fence and the connection writes them in FIFO order. Beryl serially
  admits that stream, flushes its one pending delta fragment, and scans already admitted durable item indexes
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
- Forced-abort terminal handoff uses the same target-operation election as steering and explicit
  stop. Once an exact terminal observation wins that election, no later item, approval,
  dynamic-tool, steering, or stop dispatch may enter the target. Operations admitted before the
  cut either reach their exact durable disposition first or make the projection authority-lost;
  terminal publication never guesses across an outstanding possibly dispatched operation.
- Exact CAS 0.146.0 forced-abort `turn/completed` is not treated as an upstream no-later-item barrier
  without retained release-scoped proof. An interrupted terminal therefore retains
  `ForcedAbortOrderingUnproven` history incompleteness unless a release-scoped exact-target
  regression proof establishes the stronger ordering. A later same-target provider event is
  rejected after the local terminal cut and retires the connection; it cannot reopen source
  admission, mutate finalized history, or be silently discarded as though the interrupted capture
  were complete.
- A routed dynamic-tool request is answered only through its exact owning live target. The feature
  handler receives the exact durable Syndic thread and turn context plus normalized CAS request;
  another target cannot respond, and the connection worker preserves response/event ordering.
- Protocol error, transport loss, subscription loss, worker failure, process exit, or app shutdown
  before proven terminal completion leaves the submitted turn durable with explicit incomplete,
  failed, interrupted, or unknown-terminal state. Unless exact-0.146.0 evidence proves a
  notification cursor and replay contract, reconnect, late subscription, resume, and process
  restart cannot repair the capture. A replacement
  connection is never resumed into the same authoritative live-capture target. Unknown-terminal
  remains open only while exact late evidence is still possible; proven loss of the owning process
  or loaded execution session retires the projection and permits source-less incomplete convergence.
- A source-backed unknown-terminal observation against a still-owned active target atomically enters
  `AwaitingTerminal` through the same target-operation election as steering, terminal handoff, and
  stop. Existing admitted or retryable work becomes effective `UnknownTerminal` next-turn work,
  later accepted input enters distinct next-turn generations with that reason, and no steering or
  stop dispatch may start from the uncertain target. Exact late item evidence remains admissible.
  A later exact activation reopens only a fresh empty steering generation, so work accepted or
  reclassified during uncertainty is never retroactively steered. Exact late terminal evidence
  enters ordinary `FinalizingHistory`; target or session loss uses active abandonment and
  projection-loss convergence.
- Late provider events may be admitted only before proven-terminal publication and only when idempotency and sequence checks allow. After proven-terminal publication, bounded work may finish stale or incomplete canonical and projection frontiers solely from source events that were already admitted; no later source event is accepted. Once that work becomes current it is finalized, and no later event can mutate the turn's canonical items, projections, resources, ordering, parent edges, thread bindings, or selected paths.
- An exact retry at an occupied source sequence is recognized as already durably admitted before stale event-local revisions are considered. Different content at that sequence is a collision, and a sequence gap is rejected; none of these cases rewrites the stored event.
- Under the exact CAS 0.146.0 producer contract, hosted Responses image generation remains
  unadmitted unless retained release-scoped evidence proves that CAS can send the required native
  `image_generation` tool declaration. Parser support is receive/history tolerance, not an
  admitted producer. The
  standalone `image_gen.imagegen` extension is a separate admitted producer whose generated-media
  lifecycle must be preserved. A custom provider that injects an unsolicited hosted item is
  nonconforming and outside the supported runtime contract; parser tolerance does not give Beryl a
  complete-history guarantee for that provider behavior.
- Prior-release proofs are retained in that commit's
  `notification-ordering.md`, `item-lifecycle-coverage.md`,
  `reconnect-notification-replay.md`, and `hosted-image-generation-reachability.md` memory notes
  under `doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/`.
  They are not exact-0.146.0 compatibility proof and must be refreshed before admission.

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
- A completion/live narrative mismatch proves the exact observation session violated the pinned
  capture contract, not that CAS's stored native thread is corrupt. Beryl keeps receiving the
  already active turn until an exact terminal outcome arrives or that authority is lost, but the
  turn can never become history-complete and no later turn may start through that loaded session.
  After terminal convergence, Beryl quarantines the old lease as a non-execution subscription
  anchor, opens a fresh connection to the same managed CAS process, and resumes the same CAS thread
  there before releasing the anchor. This overlap proves that CAS joined the existing in-memory
  thread rather than reconstructed it from rollout. Exact same id and idle state establish the new
  foreground lease; only then may the old connection unsubscribe or retire.

## Branch Selection Context

- The transcript's synthetic discussion-context group and the one-time CAS branch-selection projection derive independently from the same immutable Syndic envelope. Rendering the group never creates CAS input, and assembling CAS context never reads rendered text.
- Exact selected discussion text remains untrusted model-visible context. It never becomes ordinary user-authored input or developer instructions merely because Beryl stored or projected it.
- Because the selected passage originated in an assistant reply, its canonical CAS projection preserves that provenance as exactly one assistant-role message containing exactly one output-text item. This avoids fabricating a hidden user-turn boundary and does not rely on CAS-private contextual-wrapper recognition.
- The bounded Beryl frame identifies the projection version, source role, selected UTF-8 byte length, selected-text digest, and durable provenance identity before the unmodified selected bytes. Beryl framing is descriptive assistant-history context, not an instruction channel.
- The context is projected once while establishing the discussion's CAS lineage and is never resent on later turns or steering requests.
- The selected-context mechanism must preserve the complete accepted value, provenance framing, ordering, and trust semantics through one proven targeted CAS boundary. If no such boundary supports the accepted limit exactly, branch execution remains unavailable and the architecture blocks for review.

## Recovery Item Projection

- Recovery injection exposes the complete required committed Syndic path as one logical ordered,
  versioned sequence of supported raw Responses API items. That sequence is replayed through a
  revision-bound cursor and is never retained as a collection or wrapped in one JSON or prose blob.
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
- Recovery preparation is two-pass and constant-resident. The first pass walks immutable topology
  and canonical indexes in bounded pages to prove role/shape support, nonempty items, exact item and
  byte totals, and one sequence digest. The second pass exposes a revision-bound replayable item/text
  cursor that reproduces exactly that digest directly into backend encoding. Neither pass retains a
  root-to-tail identity frontier or final item sequence.

## Recovery Budget

- The complete recovery item projection is accepted only when its canonical UTF-8 payload is no greater than both 262,144 bytes and one half of the exact selected model context-window token count interpreted conservatively as a byte count.
- One recovery projection contains at most 262,144 items. Because every item is nonempty, this
  wire-shape ceiling follows from the independent 262,144-byte canonical-text ceiling and does not
  impose a smaller turn-count or resident-allocation limit.
- Branch context, current user input, developer instructions, registered tool schemas, and normal CAS overhead are budgeted separately and must leave at least the other half of the known model context window available.
- One selected discussion context entry is limited to 65,536 UTF-8 bytes before branch creation is accepted.
- Beryl does not summarize, omit older turns, truncate items, split one logical history into repeated per-turn fragments, or silently drop media labels to satisfy these limits.
- If the complete required recovery projection or selected context exceeds its proven channel limit, execution or branch creation rejects before draft admission and preserves the user's current state.
- Missing exact model context-window metadata makes fresh recovery unavailable rather than causing Beryl to guess a budget.

## Native Lineage Precedence

- An exact valid CAS binding continues on its existing CAS thread and sends no recovered Syndic history.
- A capture invariant violation retains the native CAS thread binding because it does not prove
  native history loss. Reacquisition targets that same CAS thread; Beryl does not automatically
  fork it, label it corrupt, or replace it with an injected thread.
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
- Recovery injection is unavailable when the required Syndic prefix crosses a typed
  history-incomplete turn, including a completion/live narrative mismatch, except for the narrow
  authority-lost tail-context case defined below. If native resume cannot be re-established, exact
  continuation otherwise remains unavailable; Beryl never injects a shorter prefix and pretends it
  represents the original path.
- Once a recovered CAS projection is established, its later turns continue through ordinary native
  CAS history while its exact loaded authority remains proven. Beryl never sends the injected
  prefix again to that CAS thread.
- Losing the managed process generation or the last exact subscription anchor invalidates recovered
  execution authority even when CAS retains a resumable thread id. A complete recovery-eligible
  Syndic prefix may establish another fresh injected projection. An incomplete prefix cannot,
  except when its immediate tail is the exact authority-lost context for an already admitted
  pending successor and satisfies the bounded eligibility contract below.
- Completion/live mismatch is the sole recovered-lineage rotation defined here. It keeps the old
  same-process subscription alive as a non-execution anchor until a fresh connection has resumed
  and subscribed to the exact existing in-memory thread. A source-preserving or unclassified resume
  failure retains that anchor and follows bounded retry plus explicit Retry. If the anchor or managed
  process is lost first, exact continuation becomes unavailable. Beryl does not cold-resume or
  reinject across the incomplete prefix.

## Fresh Projection Recovery Injection

- Recovery injection requires an opaque compatibility admission that owns the exact initialized
  backend client session it probed and binds that session to the selected runtime and managed CAS
  process generation. A detached report or admission from another session, runtime, or generation
  is not authority to inject.
- Recovery creates a new empty loaded CAS thread in the selected execution binding, establishes all
  required thread-level initial context, proves the thread idle, and calls `thread/inject_items` once
  while streaming the ordered recovery cursor through the fixed-page encoder. It waits for
  successful completion before starting the pending submitted turn through ordinary `turn/start`.
- Pending-successor recovery may treat only its immediate predecessor as authority-lost tail
  context. That predecessor must be an ordinary-user turn ended by its latest exact source-less
  `Incomplete(AuthorityLost)` event, retain a nonempty fully finalized item frontier with no open or
  history-blocking item and no provider-observation issue, and pass every ordinary item-level
  recovery proof. Every earlier ancestor remains recovery-complete under the general contract.
  Injection sends only that exact durable item sequence as history context; it neither changes the
  predecessor's incomplete outcome nor replays its old `turn/start`.
- The cross-thread page broker preserves cancellation, broker unavailability, source-revision drift,
  durable dependency-read failure, and invalid source/proof rejection as distinct typed causes.
  After possible dispatch they share completion-unknown and target abandonment without losing that
  cause or guessing whether CAS consumed a prefix.
- Beryl requests metadata-only lineage results, including `excludeTurns = true` where CAS provides
  it, and exposes only bounded identity/status/metadata projections from lineage and turn-start
  control responses. Incidental historical turn or item bodies are skipped incrementally and never
  become caller-visible history or replace Syndic reads; they do not force whole-response retention
  or rely on a transport-wide message ceiling.
- Injection itself starts no model turn and supplies no current user input.
- Successful injection establishes the model-visible prefix in the exact loaded CAS thread. Pinned
  normal-path source and provider-boundary proof show that supported injected items are written to
  rollout and normally survive process restart, but CAS does not expose a transactional persistence
  acknowledgement or complete readback. Beryl therefore records the exact injected Syndic prefix
  and injection-session generation and treats later CAS-produced suffixes as ordinary native
  lineage only while the process-local recovered authority remains proven.
- A recovered-lineage turn cannot activate before the recorded injection completion time and must
  use one exact currently proven loaded process/thread generation. The first such generation is the
  injection generation. A later thread-generation component may be established only by the exact
  overlapping same-process handoff; its process component must remain the injection process.
- A failed injection never falls back to user input, developer instructions, `additionalContext`, chunked replay, truncation, or summary. The admitted Syndic turn remains durable and explicitly pending, retryable, or failed.
- Success, structured rejection, transport loss, and unknown completion all consume the one-use fresh-idle injection capability. Any non-success causes that fresh CAS thread to be abandoned; recovery may create another fresh thread rather than risking a second injection into the same thread.
- Beryl publishes a usable binding only after injection success and the durable local proof commit.
  That proof authorizes the exact successful injection in its loaded process-local thread. An
  ambiguous, abandoned, unloaded, process-replaced, or locally uncommitted injection target remains
  provenance only and is never promoted by later resume.
- Public CAS thread reads are not injection readback and cannot turn an ambiguous delivery into proof. Beryl never retries injection in place.
- CAS threads abandoned during recovery are not deleted. Later cleanup requires the future garbage-collection design.

## Loaded Projection Leases

- Process-local loaded-thread authority is represented by an explicit subscription lease owned by
  the exact connection generation, runtime id, managed-process generation, CAS thread id, Syndic
  thread id, and loaded generation. A map entry, another connection to the same process, or a
  durable CAS thread id alone is not current execution authority.
- A pre-activation loaded projection or ordinary same-native quarantine anchor owns one
  non-cloneable surrender child derived from the exact admitted worker that may produce it. The
  child and worker share one counted admission unit, which is released only after both are gone;
  there is no separately acquirable residency pool. Loaded-thread registry authority is created
  only with that child. Activation releases the child only after the owning router has accepted the
  live target. A target-to-projection handoff derives a fresh child from its still-admitted worker
  before removing router authority. Same-service quarantine transfer moves the child directly;
  cross-service transfer receives the replacement worker's child before consuming the source.
- Live targets do not consume worker-capacity-derived residency slots. Each mounted router admits
  at most 64 exact targets, and each mounted connection already owns its two admitted connection
  workers, so steady-state target accumulation and failure-time target escrow remain structurally
  bounded by the actual owning boundary. A target surrendered after failure moves under its frozen
  router guard without another capacity decision. Durable thread count and historical registry
  entries create no capacity. Releasing the last owner performs the exact CAS unsubscribe
  choreography, removes process-local authority, and prevents later recovered use until a new
  projection is established.
- Explicit consuming release removes local authority before its bounded unsubscribe request; every
  unsubscribe status or error remains non-authorizing. Implicit drop performs no backend I/O. It
  removes the exact token and retires the connection when that forgotten token was the last owner,
  so GPUI cannot block on cleanup and no untracked subscription stays reusable.
- One process-owned connection service may hold multiple exact per-thread leases. A recovered
  source fork uses its exact live lease when present. After lease loss, that source is not licensed
  for resume or fork merely because the CAS id persists. A process-local registry observation never
  licenses dispatch through an unproven connection.
- Narrative-mismatch rotation uses a separate non-cloneable quarantine handoff, not ordinary lease
  release. It consumes every old-generation execution token while preserving one exact remote
  subscription anchor. Before `thread/resume`, the registry admits exactly one bounded,
  non-execution replacement reservation on an otherwise fresh connection for the same runtime,
  managed process, CAS thread, Syndic owner, root, and tool profile. Transfer atomically consumes the
  exact old anchor and exact replacement reservation while serialized against retirement of both
  connections, then publishes a fresh loaded-thread generation before cleanup releases the old
  anchor.
- A connection named by an unconsumed replacement reservation admits no ordinary loaded-thread
  registration, sibling acquisition, or second reservation. Consuming or abandoning the
  reservation, or retiring either named connection, removes that exclusive admission state. Local
  abandonment retires the unused replacement connection so an untracked remote subscription cannot
  remain reusable, while the exact old anchor remains eligible for a later fresh replacement.
- Two-sided reservation and transfer acquire connection retirement gates in ascending connection
  generation order, consult the old router lane and then the replacement router lane without holding
  both router locks together, and only then enter the process-wide loaded-thread registry. Backend
  requests and durable storage work occur after those gates are released. An old-lane fence revokes
  the anchor synchronously and makes continuation unavailable; a replacement-lane fence rejects that
  replacement while preserving the old anchor for Retry.
- Connection retirement linearizes loaded-thread authority with only the bounded retired-check-plus-
  registry-acquisition section. If acquisition wins, retirement removes it; if retirement wins,
  acquisition rejects. Session release, cleanup completion, and scheduled-promotion release elect
  ordinary detachment only through their still-owned master command permit. Its short retirement
  commit shares the same gate mutex as typed failed-health observation, so failure-first rejects
  detachment and detachment-first becomes the sticky ordinary owner. Scheduled promotion may
  additionally acquire one non-cloneable reservation inside the connection gate. On persistent
  failure that reservation becomes an exact cut-identity token rather than a boolean marker; only
  consuming cut-identified recovery may release its barrier. The gates are released before storage work;
  retirement fences new authority immediately and waits without holding either gate until the
  reservation publishes and reconciles, is abandoned, or is retained for the cut. No backend call
  occurs under the reservation, and no retired connection may insert a later loaded-thread entry.
- CAS subscription loss, `thread/closed`, connection loss, process replacement, or coordinator
  shutdown invalidates the matching loaded leases. Late events or releases from an older
  generation cannot remove or authorize a newer lease. On one still-live connection, abnormal
  target retirement fences the remote CAS thread from acquiring a newer target generation because
  the wire event itself cannot prove which local load produced it.
- Connection-scoped `thread/closed` is an authority event even when no turn target exists. Before
  handoff transfer it revokes the exact old anchor or poisons the exact replacement reservation on
  the connection that observed it. After transfer it revokes the new-side lease, while a late close
  on the obsolete old connection cannot affect the new generation. A close that linearizes before
  transfer therefore prevents transfer rather than being counted only as unmatched telemetry. The
  stable connection forwarding boundary records the router-lane close fence before entering
  connection authority, so a concurrent reservation or transfer either observes the fence or
  completes first and is then revoked by the waiting close. That boundary consumes the compact
  ordered close before replaceable service-epoch broker cancellation and remains authoritative on
  either side of the persistent-failure cut.
- Ordinary native CAS lineage may later cold-resume through its proven persistent-rollout contract.
  Recovered lineage may change its loaded-thread generation only through the overlapping
  same-process handoff above; process loss or last-anchor loss requires fresh recovery from a
  complete eligible Syndic prefix rather than trusting rollout reconstruction.
- Input preparation borrows a loaded projection before durable activation. A non-health-changing
  pre-activation failure returns that exact capability for an ordinary same-generation retry. A
  structural preparation failure may instead fail its Beryl-home generation closed while the CAS
  lease remains live; the old-generation wrapper is then non-executable even after same-home
  recovery and may not be relabeled in place.
- A finished persistent-failure cut may contain several complete pre-activation wrappers. Every
  wrapper's authority remains preserved because each carries a distinct already-admitted worker
  surrender; the cut does not nominate one candidate or authorize an order-dependent winner.
  Wrappers with the same connection, loaded-thread identity, and complete projection witness form
  one candidate group retaining every lease token and admission hold. A witness disagreement makes
  that whole group non-promotable instead of selecting one wrapper. Consuming the cut first moves
  its pointer-identical filled escrow slot into one checked-out inventory while keeping the exact
  identity reservation continuously occupied, joins every old scheduler worker that could still
  surrender a wrapper, and seals the entire worker-bounded set. An old scheduler operation rejected
  by the same exact failed home generation is expected cut-correlated quiescence, while an unrelated
  invariant failure, poisoned boundary, or worker panic makes the owning inventory non-promotable.
  Scheduler exit must preserve that distinction rather than collapse both cases into one fatal bit.
  Scheduler-main unwind remains outside the runtime that owns its child workers. Panic containment
  releases any owner-side launch barrier, records local failure, cancels both worker families, and
  joins every child before propagating the parent panic to inventory conversion; no intermediate
  bookkeeping collection may detach an unjoined handle. Therefore a panicked parent is
  non-promotable without weakening the pre-seal all-children-joined proof.
  The command permit reads the exact gate cause that invalidated it instead of reconstructing cause
  from a later home-health sample. Local failure or poison dominates exit classification on either
  side of the persistent-failure election without displacing cut ownership, and recovering a
  poisoned owner is permitted only to retain and join its authority, not to certify promotion
  safety. Inventory conversion rechecks that exact gate after the scheduler join so a local
  fatality or gate poison arising after thread exit but before sealing remains non-promotable.
  Inventory drop performs one bounded
  in-memory return to that same slot unless a later consuming stage disarms it. A publication
  observed after sealing stays owned and makes that inventory non-promotable. A second consuming
  boundary converts a stable inventory into cut-identified local quarantine groups and consumes or
  preserves every other retained target, raw lease, reacquisition, promotion, and cleanup barrier
  according to its exact disposition. That boundary checks out every retained collection under one
  coordinator lock only after the sealed counts, finished cut, zero late publication, and one-way
  stage all agree. A publisher that crosses checkout remains owned and prevents a promotable
  installation; a second checkout cannot observe or drain the same owners. Before mutation, the
  conversion proves pointer-exact agreement among the cut, retained service, and current connection
  sets; complete registry sibling coverage; an aggregate read-only match against each router's
  complete frozen-or-spent target-guard set; and exact connection-gate promotion and cleanup
  topology with no live barrier left able to publish. The connection-gate audit complements the
  loaded registry because those two barrier kinds have no loaded-thread token.
- Candidate grouping first uses the exact stable connection and loaded-thread/session identity,
  then requires one complete equal home, Syndic owner, binding revision, execution binding, CAS
  thread, loaded generation, and lineage witness. Witness values do not participate in the grouping
  key, so disagreement cannot split into apparently valid groups. Every equal wrapper becomes a
  retainer-free local owner that preserves its distinct lease token and worker admission hold.
  Noncandidate loaded authority is converted to local disposition ownership before every retained
  connection atomically exchanges its complete promotion and cleanup barrier set for one
  non-cloneable quarantine connection owner under the connection gate. The same hold is installed
  when the exact barrier set is empty: a stable connection `Arc` proves identity but cannot prevent
  retirement from invalidating its loaded tokens. One registry-lock commit then keeps exactly the
  candidate subset and removes every other audited token. Frozen router guards settle afterward by
  rechecking and consuming the already-audited complete batch; neither boundary authorizes external
  work.
- Any preflight mismatch leaves the checked-out drain in one inert installed authority. Any race,
  poison, or disposition failure after local normalization preserves the candidate owners,
  remaining dispositions, completed-disposition count, retained service, and exact identity
  reservation in the same non-promotable aggregate. Public observation exposes only cut/home
  identity, bounded counts, late-publication count, and promotability; it exposes no candidate,
  connection handle, token, or executable operation. Neither inventory conversion nor quarantine
  conversion performs a recovered read, backend request, unsubscribe, durable mutation, candidate
  selection, generation rebind, service publication, or old-gate reopening.
- Publication crossing the checkout-to-install interval is atomically drained into a distinct
  inert installed authority before installation returns. Publication after installation bypasses
  the old side collections and enters another inert authority immediately, conflicting the stage
  and making both success and owning-error metadata non-promotable. Thus a late lease, anchor,
  reservation, promotion, or cleanup owner cannot be diagnosed while remaining outside the
  quarantine ownership graph.
- Registry disposition owners disarm only after the exact batch commit removes their tokens. An
  inert owner dropped without that commit performs conservative local revocation by its globally
  unique primary token, recovering a poisoned registry guard and rebuilding per-connection
  authority counts from the surviving entries and reservations before releasing worker admission.
  This destructor path cannot authenticate promotion, issue external work, or clear registry
  poison for ordinary operations.
- A quarantined connection separates its stable backend core from its service epoch. Stable
  identity comprises the connection generation, runtime and managed-process generations, backend
  transport and sole stream driver, one pre-reserved adoption-control slot, one backend-bound
  ordered-stream forwarding hub, the connection-scoped process fact and final retirement authority,
  loaded-thread registry entries, loaded-session generations, and exact lease tokens. Content-free
  transport and page diagnostics may remain with that core but never authorize work. The epoch
  comprises home and Syndic handles, the master command authorizer, event-publication router,
  forwarding-hub endpoint and ordered-broker publication context, stop and compaction coordinators,
  scheduler wake authority, failure notification and retention authority, and every driver,
  ingester, scheduled-worker, and pre-activation surrender admission attached to that service
  generation.
- The stable process fact is registered once with the stable core. A router receives only a
  non-owning observation path and may retire its own targets, but router retirement or destruction
  cannot publish connection retirement. Only final stable-core retirement may remove the process
  fact. Replacing or dropping an old router, broker, gate, worker context, or service owner is
  therefore unable to retire an adopted connection.
- Adoption consumes one promotable quarantine and one non-cloneable never-published
  replacement-service authority. Its construction occurs only behind the startup publication fence
  and proves the pointer-identical retained `HomeStore` and canonical home identity, the same home id, a
  strictly newer currently healthy home generation, a strictly newer freshly allocated service
  generation, the exact reacquired Beryl and Syndic handles for that generation, an open new master
  gate, an empty connection registry, no prior attachment, adoption, worker start, or scheduler
  dequeue, and complete stop, compaction, scheduler, failure, worker, router, ingester, and
  forwarding-endpoint context. All replacement execution remains behind the same startup fence. A
  published service or a collection of cloned handles is not replacement authority. This dormant
  constructor does not reuse ordinary startup recovery: it performs no recovered-pending read,
  Syndic revision read, durable convergence, or scheduler pass before returning the authority. A
  closed worker gate alone is not proof of read-free construction.
- Successful same-generation verification occurs before any persistent-failure cut and leaves the
  current service generation intact. It cannot construct replacement-service authority. A
  promotable quarantine proves that the old home generation reached `failed`; only exact same-home
  forced recovery can then publish the strictly newer generation used by adoption.
- An accepted-input scheduler that observes the exact current home and generation in `verifying`
  enters a typed nonterminal verification pause. It signals or joins the process recovery-
  supervisor flight, retains or safely restarts bounded scan authority, closes no master gate, and
  performs no timer polling. Healthy verification completion validates the pointer-current home and
  service generations in the process slot, publishes `VerifiedCurrent` to the exact provider-
  waiter flight while retaining its completed-flight snapshot, and only then wakes that exact
  service through a dedicated same-generation-verified signal. That signal resumes every applicable scheduler lane without
  creating ordinary active-steering retry eligibility. A stale completion cannot wake a replacement
  epoch, and failed verification issues no resume before the existing persistent-failure cut.
  Replay, projection, admission, and publication layers retain an exact typed health-gate cause
  through worker settlement; converting it to an invariant string or resampling mutable health at
  the scheduler boundary is not valid classification.
- Mounted service-epoch workers join that same sole supervisor-owned verification without sharing
  the scheduler's consumable wake. The exact service notification owns a monotonic multi-waiter
  completion epoch. Provider staging and frame-build committers atomically register against their
  home, home generation, and service generation, signal or join the flight, then wait without
  polling or holding gate or process-slot locks. Before a failed-service cut drains live-command
  permits, the supervisor publishes verified-current, failed, stale, or shutdown completion and
  wakes every registered waiter. Verified-current preserves the exact staged batch, prepared frame,
  or build frontier for the existing point-read reconciliation; every other outcome returns typed
  authority loss. An exact pre-command `verifying` state joins before dispatch, while a point-read
  that observes a new `verifying` epoch joins that epoch and repeats the same exact read without
  dropping the frontier. No service-epoch worker invokes home verification or rotates durable
  identity to resolve an ambiguous command.
- Persistent-failure notification orders the complete cut boundary: the exact failure election
  occurs first, failed-or-stale completion is published to every registered provider waiter second,
  and only then may the cut worker be signalled or any live-command permit drain begin. Shutdown or
  unavailable completion likewise publishes before ordinary or terminal service drain. Provider
  completion never consumes or substitutes for the scheduler resume wake.
- The adoption set is the quarantine's complete pointer-identical retained-connection set, with one
  live connection-quarantine owner for every member. It is sorted by stable connection generation
  and must equal the old retained-service registry exactly; duplicate identities, missing or extra
  connections, a candidate outside that set, a retired core, or a foreign cut rejects the whole
  operation. An exact empty set is valid only with zero candidates and zero connection owners and
  still consumes both authorities into the same adopted-unpublished result. After commit, that
  exact connection set remains immutable through reauthentication, seal, and final publication. A
  retired or unauthenticatable member cannot be pruned, including a zero-candidate member or one
  whose candidates were all explicitly disposed.
- Before any connection changes, adoption reserves the complete replacement topology. It acquires
  one replacement driver-and-ingester admission pair per retained connection, one replacement
  scheduled-worker admission converted directly into a pre-activation recovery hold for every
  candidate's exact old hold, every fixed broker resource, and every replacement router and
  endpoint. Replacement ingesters remain start-blocked and other replacement workers remain dormant
  behind the startup fence. Old candidate holds stay live until commit; replacement holds are mapped
  one-for-one and do not change loaded generations, registry tokens, leases, or candidate grouping.
  The separately bounded old and replacement service pools account for their own complete sets
  from preflight through commit; the successful output keeps the complete old set charged in its
  closed old-epoch attachment until explicit retirement. Capacity, allocation, thread start, poison,
  or topology failure before commit moves every prepared replacement resource already acquired,
  together with all old holds, into the owning inert result; nothing reached by the attempt is
  released outside that authority.
- Each stable driver owns one pre-reserved capacity-one non-service adoption-control slot independent
  of its permanently closed old master gate and epoch command queue. The one-shot control names the
  exact failure cut and the command frontier sealed when that gate closed, so later ordinary queue
  traffic cannot starve or overtake it. The driver settles already-dispatched work without issuing
  another request, explicitly rejects every not-yet-dispatched old-epoch command through that
  frontier with one typed cut-correlated nondispatch completion, and lets each owning scheduler
  worker surrender before the driver parks. This completion performs no durable accepted-input
  mutation and arms no retry; scheduler quiescence joins every old worker before the recovery
  inventory seals and before adoption begins. No live old-epoch command crosses adoption.
  The driver
  finishes the synchronous acknowledgement of any already-selected ordered observation, selects no
  later stream observation once the control is pending, and then parks without backend work. Old
  ordered ingesters must be terminal, ownership-clean, explicitly joined, and unable to acknowledge
  another operation before commit. The join result returns the exact old ingester admission and
  terminal receipt into the attempt-owned old-epoch attachment; neither is dropped or released to
  the old pool. Cancellation or detached thread-handle drop is not that proof. Every command carries
  its admitting epoch. Ordinary dequeue validates that identity under the stable adoption-slot
  execution guard; an unexpected mismatch receives the same typed nondispatch completion before
  provider work as a defensive invariant, never silent drop or cross-epoch execution.
- The capacity-one slot remains part of the stable core across successfully published service
  generations; only each cut-bound control message and its park token are one-shot. Successful
  publication returns the slot empty and eligible only for a later strictly newer failure cut.
  Inert adoption failure permanently disables the touched core and cannot reuse the slot as retry
  authority for the failed cut.
- Every driver cycle, including its one-time initial approval-interruption drain, first acquires the
  stable adoption-slot execution guard. If hub or epoch coordination becomes unavailable while
  that guard is held, the driver moves its backend session and worker admission into a stable
  non-executable quiesced state instead of falling through to transport shutdown. Exact-cut inert
  conversion changes empty, pending, parked, starting, or quiesced state into the same disabled
  terminal state and retains every reached admission. Ordinary stop notification and implicit
  handle drop may wake or detach ownership but cannot turn quiesced or disabled state into shutdown
  authority. Backend shutdown occurs only from a typed proven ordinary-lifecycle exit or the sole
  explicit consuming inert disposition; the driver loop has no unconditional shutdown epilogue.
- Ordinary retirement of a stable core remains distinct from adoption quiescence and inert
  disposition. The epoch's sticky ordinary-versus-exact-failure election also controls one
  ingester-admission disposition slot. Ordinary retirement arms release before cancellation; the
  ingester's terminal guard then drops its exact permit, or an ordinary arm arriving after terminal
  drops the still-escrowed permit. Exact failure arms retain-for-adoption with the cut identity;
  unresolved, poisoned, or mismatched state retains conservatively. The later ordinary join returns
  terminal proof without admission ownership, while exact-cut adoption join must return terminal
  proof plus that retained permit. Last-owner Drop only performs these bounded authority, release,
  cancellation, and wake transitions; it never joins.
- Before a later admission reserves a connection-worker pair, and before consuming ordinary service
  close drains its registry, the service snapshots stable registry membership, releases the
  registry lock, and reaps only cores already proven ordinary-retired with both driver and ingester
  finished. It then reacquires the registry solely to remove those exact pointer-identical detached
  shells. This reaper is secondary lifecycle cleanup, not worker-permit release authority. Joins,
  hub access, driver access, and connection-authority checks never run while the service registry is
  held. An already-reaped ordinary shell is settled rather than a second shutdown failure; a
  persistent-failure, quiesced, disabled, or adoption-owned core is never eligible for this reaper.
- One connection-local ordinary-shutdown settlement serializes worker checkout, joins, diagnostics,
  and forwarding-hub detachment. Exactly one lifecycle caller performs that sequence. Concurrent or
  later callers observe its terminal clean-or-failed classification: clean is idempotent success,
  while failed remains a shutdown failure and cannot be reclassified merely because the hub is now
  detached. The reaper enters this same settlement before treating an absent checked-out worker as
  finished, so it cannot overtake another caller's join.
- Fallible preflight sends and awaits every driver park control in ascending stable connection
  generation, then joins every old ingester in that same order, and only then acquires every
  forwarding-hub epoch barrier in ascending stable connection generation. It next acquires the old
  and replacement service-connection registries in ascending service generation. It holds no
  quarantine-coordinator, router-lane, connection-authority, loaded-registry, home, or Syndic lock
  while parking or joining. Exact quarantine-owner and service-registry topology checks occur before
  parking and are repeated without co-holding router and connection locks. The service registries
  are never held while acquiring a driver, hub, router, connection-authority, or loaded-registry
  boundary.
- A stable forwarding hub holds its epoch barrier from endpoint selection through synchronous
  acknowledgement. For `thread/closed`, it holds that outer barrier while recording the selected
  epoch router's lane fence, releases the router lane, and then enters stable connection authority
  and the loaded registry. Thus a close selected before adoption fully settles against the closed
  old epoch and may revoke a candidate; a close selected after adoption reaches only the new router.
  No router lane is held with connection authority or the loaded registry. Adoption preserves every
  candidate owner without treating token liveness as connection-set membership; consuming
  candidate reauthentication alone authenticates each exact loaded-registry token and rejects one
  already revoked by closure.
- Commit begins only after every fallible reservation, start, park, join, lock acquisition, and
  repeated identity check has succeeded. While all drivers and endpoints remain fenced, commit uses
  only preallocated, ownership-moving operations: it exchanges every epoch pointer, driver worker
  admission, candidate recovery hold, and old/replacement service-registry membership, then moves
  every ingester start token and driver park token into the success output. It does not open an
  ingester or release a driver. No commit step may allocate, wait, join, lock a new boundary, call
  user code, perform backend or storage work, or return a recoverable error. The replacement service
  remains behind its startup publication fence.
- Success produces one non-cloneable adopted-but-unpublished service authority. It owns the new
  service, all adopted stable connections, the still-quarantined candidate owners and exact
  registry-token or local-disposition identities, and the complete closed old-service and old-epoch
  attachments required for later explicit retirement. Candidate execution remains unavailable until
  consuming generation reauthentication. The old master gate remains permanently closed and no old
  target, command, router, broker, scheduler, or retainer can publish through an adopted core. Its
  new endpoint is installed but its ingester remains start-blocked, every stable driver remains
  parked, and no backend observation or command can execute through the adopted core.
- The committed adopted service has one distinct post-commit terminal outcome. Recovered-home or
  adopted-service drift, stable-core retirement or mismatch, service-membership loss, or inability
  to authenticate shared loaded-registry authority makes the complete fixed connection set
  permanently unpublishable. It does not return to adoption, prune the failed connection, or turn
  shared authority loss into a retryable candidate rejection.
- The final adoption ownership move holds the old-cut coordinator and its late-owner escrow in that
  order, validates both while locked, changes the stage from adoption checkout to adopted, and runs
  the infallible commit before releasing either lock. Success owns the resulting non-cloneable
  adoption fence rather than a cloneable escrow handle. After every old publication source is
  retired, later recovery consumes that fence under the same two locks. Publisher-first returns the
  complete fence as terminal failure; retirement-first changes the stage to adoption-retired and
  issues one non-cloneable retirement witness. Any owner that nevertheless arrives afterward stays
  terminally escrowed in the adoption-retired stage and cannot enter candidate or service state.
  The process publication lock is never acquired while either old-cut lock is held; final process
  publication must consume the retirement witness together with the converged adopted output.
- Only the later recovery-publication authority may consume a candidate-set-converged adopted output
  after old-epoch retirement and startup convergence. It acquires every exact stable-connection
  authority and retirement gate in stable order and holds the complete set continuously through
  final validation, process-service installation, and startup-gate opening. Its final publication
  commit first arms every replacement ingester and stable driver against the same still-closed
  startup gate, then atomically installs the current service and opens that gate under one process
  publication authority. Publication-first installs the service before releasing converged
  retirement retention; retirement-first returns the complete terminal adopted-service owner.
  Check-then-release-then-publish is not authorizing. Worker wakes occur only after the short commit
  releases its locks. None of those activation capabilities is separately visible or droppable into
  an executable state. This stable-core race is separate from the old-cut publisher/retirement-
  witness race, whose witness remains a required input to the same publication commit.
- The process publication slot lends the current service only through counted, non-cloneable scoped
  leases. Withdrawal first removes the pointer-exact epoch, then waits for every lease to release its
  service owner. A lease must drop its service `Arc` before decrementing the count or waking the
  waiter; zero therefore proves that the withdrawn epoch is the sole remaining service owner and may
  be consumed without an ownership retry.
- The process recovery supervisor owns one repeatable scheduled-ordinary provider factory across
  service epochs. The factory, rather than an epoch provider, retains the stable admitted-session
  pool and its sole connection-session ownership. Each service receives one freshly fenced provider
  view. Retiring an old epoch shuts down that view and returns every checkout to the factory without
  dropping the stable session owner; only final supervisor shutdown closes the factory and releases
  those sessions. The supervisor factory owner keeps only weak revocation controls for issued epoch
  views, never service or connection ownership; final shutdown fences every still-reachable view
  before releasing the pool even when conservative failure ownership retains its wrapper. A
  replacement provider cannot be reused from the failed service or manufacture a second stable
  session for an adopted connection.
- If explicit supervisor shutdown wins while forced reopen is waiting, it consumes the exact
  pending-projection quarantine through a distinct nonpublishing terminal-disposition stage rather
  than dropping it back into retained-service escrow. Under the old-cut coordinator and terminal
  escrow it takes installed or conflicted authority, redirects any later old-cut publication into
  that escrow, then outside the locks settles candidate, local-disposition, connection-quarantine,
  stable-driver, ingester, old context, and old provider ownership. Only an empty terminal escrow
  yields the one-shot witness that removes the exact retained-service escrow. This path closes no
  retained home, claims no adoption, and creates no replacement execution or publication authority.
- Final publication transfers the sealed dormant accepted set into one bounded recovery lane owned
  by the replacement scheduler. The lane remains inaccessible until the shared startup gate opens.
  It then materializes each exact retained lease token and replacement worker admission into an
  executable projection, obtains only the remaining process-shell execution authority from the
  current epoch provider, and enters ordinary execution after complete input preparation. Provider
  decline or temporary unavailability dematerializes the wrapper back into the same dormant owner
  and admission hold for a later wake. The lane never calls ordinary projection acquisition, mints
  another registry token, or releases the exact token before the recovered execution settles.
- Adoption consumes both inputs even on failure. Any preflight mismatch, late owner, duplicate use,
  poison, failed park or join, or detected partial installation returns one non-executable inert
  adoption authority owning the quarantine, replacement service, prepared resources, and every old
  or new epoch attachment it reached. It exposes no connection, candidate, token, backend command,
  retry, or publication capability. Before the first park, preallocated attempt state provides a
  terminal inert fallback for every exact core. Failure or unwind marks each touched hub endpoint
  inert and each touched driver permanently parked or cancel-only while its fence is held, transfers
  the owned resource bundle to the result or pre-reserved escrow, and only then releases borrowed
  guards. No lock guard escapes the operation, and dropping the result cannot resume a driver or
  expose an endpoint. Its sole consuming explicit disposition may cancel and join workers, revoke
  local authority, and release owners outside every authority lock, but cannot authenticate another
  adoption. Implicit drop performs only bounded in-memory cancellation, escrow, revocation, and
  handle detachment; it never waits, joins, or performs backend or storage I/O. The infallible commit
  region contains no injected failure point.
- Adoption performs no recovered read, backend request, unsubscribe, durable mutation, candidate
  selection, generation rebind, service publication, history reconstruction, scheduler dequeue, or
  old-gate reopening. Same-home recovery and handle reacquisition precede replacement-authority
  construction; candidate reauthentication and startup convergence follow the adopted-unpublished
  output.
- After successful same-home recovery and connection-epoch adoption, one explicit consuming
  reauthentication may transfer each still-live quarantined candidate into dormant accepted
  provenance without a CAS request or durable mutation. It requires the same home id and admitted
  newer generation, plus bounded stable Syndic reads proving the exact pending ordinary turn, zero
  source events, pending input gate, selected path, valid binding revision and complete binding
  facts, execution binding, CAS thread, lineage, and sealed submitted input. The transaction
  authenticates the same stable connection, managed process, loaded-thread generation, Syndic
  owner, and registry token before and after those reads, then reconfirms the recovered home.
- Accepted provenance is non-executable. It retains only the recovered home and service identity,
  exact stable loaded identity, durable witness, registry token, lease owner, and replacement hold.
  Executable projection reconstruction occurs only after final service publication succeeds;
  complete input preparation then starts from the recovered generation and revalidates every text,
  marker, asset owner, sidecar, runtime path, and cancellation boundary. No prior preparation
  evidence crosses recovery. Reauthentication never resumes, forks, injects, reconstructs history,
  clears a durable failure, manufactures a second lease, or falls back to ordinary projection
  acquisition.
- Candidate-local durable mismatch, unstable durable revision, or exact token revocation on an
  otherwise authenticated live core returns the exact quarantined capability to the owning ledger
  for retry or explicit disposition. Recovered-home or adopted-service drift, stable-core
  retirement or mismatch, service-membership loss, or unavailable shared registry authentication
  instead terminalizes the complete committed adoption.
- The adopted service authority retains one exact reauthentication ledger over the complete
  candidate set. A rejected capability and its replacement recovery hold return to that ledger;
  another authorized attempt may occur only while the service remains unpublished, and any such
  retryable entry blocks publication. Sealing the ledger requires each original candidate exactly
  once as either dormant accepted provenance or a consuming explicit disposition. Acceptance
  transfers the exact replacement hold into the dormant recovered-candidate inventory. Disposition
  confirms or performs local registry revocation, consumes the lease and quarantine capability, and
  returns that hold to the replacement pool without backend or durable work. The sealing transition
  atomically transfers every exact connection-quarantine owner, including owners for zero-candidate
  connections, into private candidate-set-converged retirement retention while authenticating the
  complete accepted-token set. That retention exposes no connection operation and is released only
  by later final publication or converged-authority disposal. No quarantined, retryable,
  disposition, rejected lease, rejected registry-token, connection-owner, or rejected worker-hold
  capability can escape that authority or cross service publication. Each accepted entry's stable
  lease and registry token remain in the sealed dormant inventory.
- A terminal transition demotes every accepted, rejected, or unprocessed candidate that was not
  already disposed to the same service-terminal reason under one non-retryable whole-attempt owner.
  Retry, per-candidate disposition, successful seal, and publication are then unavailable. The
  explicit whole-attempt disposition performs every local candidate revocation and replacement-hold
  settlement before releasing connection owners and consuming inert adopted-service cleanup. That
  cleanup requests and joins every unpublished service worker, shuts down its provider, reports a
  typed shutdown failure only after attempting all joins, and releases but never closes the retained
  home. Zero-candidate sets enter and leave the same terminal owner. Implicit drop remains bounded,
  nonblocking, and free of backend or durable work.

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
  the explicit projection-lost reason. A `Delivering` fragment whose provider acceptance remains
  possible becomes terminal delivery-unknown, leaves every live route, remains durable
  accepted-input user history, and is never replayed automatically. A later exact-rejection rule
  may instead prove one named delivering request was not accepted. The projection-loss successor
  persists the stable source binding and generic-or-named disposition identity together with the
  actual gate and selected-route authority consumed inside the serialized mutation, including the
  named input and source leaf revision when present. Ambiguous completion is exact only when that
  complete witness matches; one abandonment mode cannot authenticate the other.
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
- Accepted steering records target the exact CAS thread id and expected active CAS turn id and
  preserve admission order. An absent expected turn id never authorizes a steering claim or
  dispatch.
- One steering worker claims one exact ready leaf under its immutable generation and exact
  semantic target, then reopens that accepted input as the same count-and-digest-bound replayable
  text/local-image descriptor source used by ordinary turn start. The serialized storage mutation
  consumes the current compatible gate and route-head revisions; an independently admitted sibling
  may advance those shared aggregates without invalidating the leaf operation. The worker and
  backend retain only the current descriptor and page; they never construct a whole input
  collection or another accepted payload.
- The app acquires one fixed worker slot before the ready read and one connection-wide,
  non-cloneable active-steering attempt before the Ready-to-Delivering claim. Worker-slot
  exhaustion or an already occupied connection attempt performs no route mutation and leaves the
  exact accepted input durably ready for steering. The steering attempt fences the target,
  connection, loaded generation, CAS thread and turn, and command authorization until durable
  disposition or atomic transfer to target-loss publication; no second steering request may
  overlap it on that connection.
- Long-lived connection driver/ingester pairs and scheduled ordinary executions cannot consume the
  complete process worker pool. Worker permits have closed connection, scheduled-ordinary, and
  steering-critical roles. Configuration admits at least one connection pair, one long-lived
  scheduled ordinary execution, and one protected steering-critical permit. Connection-pair and
  scheduled-ordinary admission leave the protected permit free unless a steering-critical worker
  already owns it under the same accounting lock; a scheduled ordinary worker never counts as that
  progress owner. Steering-critical work may consume the final free permit. Every admitted worker
  remains accounted until its actual worker returns.
- One process-owned level-triggered accepted-input scheduler discovers both active-steering and
  eligible next-turn work only through bounded revision-bound durable source pages. Accepted
  publication, steering-target readiness, worker and same-thread-flight release, durable terminal
  or projection-loss publication, and runtime/session readiness coalesce wake state rather than
  retaining one waiter or task per input. Steering and next-turn scans retain separate compact
  cursors and eligibility facts but share one scheduler signal and the worker pool's sole release
  waiter. Each pass reserves the applicable worker role before selecting one candidate, preserves
  accepted order, and stops without durable mutation when capacity is unavailable; durable
  accepted state remains the backlog for a later pass.
- A service that observes any next-source row at construction fences its entire next-turn lane for
  restart recovery. Later same-process admission cannot leapfrog that durable predecessor, and
  execution, worker, or flight readiness cannot implicitly claim recovery authority. The recovery
  controller must finish its bounded startup classification and explicitly hand the lane back.
- Admission-only thread revision advances are compatible only when selected tail and digest remain
  exact and the revision does not regress. Native planning may reuse the same CAS projection across
  that drift. Binding activation then advances the represented-prefix source revision to the
  compatible current proof while preserving the original CAS establishment lineage, execution,
  native count, and tool profile; mutation, reconciliation, and reopen validation apply the same
  relation.
- Public accepted-input admission is an opaque prepared capability consumed only by the owning
  projection service, and callers cannot execute its raw cross-domain command. The immutable
  accepted-input record persists the complete original admission intent; exact reconciliation
  proves that receipt plus permanent order and route-leaf identity and therefore survives later
  valid delivery, rejection, activation, terminal, promotion, asset-owner, or projection-loss
  descendants. Because only that opaque `SyncAll` command publishes the receipt, exact receipt
  authority also proves its compact asset-owner participant committed atomically. Scheduler wake
  follows exact acceptance. An unresolved read or successful command without the exact receipt
  fails the service closed, without requiring one fragile app mutex across every route publisher.
- Exact failed-home command-frontier rejection is process-local attempt settlement, not a durable
  lifecycle transition. Its typed cut-correlated nondispatch completion returns the scheduled worker
  to persistent-failure surrender without changing the accepted input, pending turn, immutable
  receipt, registry token, or retry eligibility. Scheduler shutdown waits for that surrender and
  joins the worker before recovery inventory sealing; only later successful recovery publication
  may reconstruct execution from the retained durable provenance.
- Durable `Retryable` means only that exact evidence proves the prior attempt did not dispatch and
  that another attempt is not forbidden. It does not itself authorize an immediate or timed retry.
  `Admitted` work is eligible in an ordinary scheduler pass; `Retryable` work is eligible only in a
  bounded retry pass opened by an explicit fresh cancellation lifecycle, recovery authority, or a
  future explicitly transient deadline. Ordinary ready publication, target readiness, worker
  release, and connection-attempt release may schedule admitted work, continue an already eligible
  pass, or resume a capacity-blocked pass, but they do not open another retry pass merely because
  the worker just returned the same input to `Retryable`.
- The scheduler retains only compact pass cursors and one process-global retry-eligibility state,
  never an input identity, payload, timer, waiter, or retry set. A future failure class may arm one
  capped process-global retry deadline only when its producer exposes a closed explicitly transient
  and proven-nondispatch disposition. No current backend or app failure has that disposition, so
  the current scheduler arms no speculative retry timer.
- Every claim or standalone disposition request names stable operation identity: the accepted
  input, source leaf revision, transition kind, immutable route generation, and exact semantic
  steering target. Under the serialized writer, the mutation validates that identity against the
  current compatible generation, captures the actual current gate and selected route, and persists
  those facts in one bounded successor proof. Ambiguous commit reconciliation therefore remains
  exact without a read-to-write rebase window or two identical reads of mutable shared authority:
  the immutable successor witness plus monotonic compatible descendants is sufficient.
  Same-generation sibling admission is compatible shared-authority advancement; a changed leaf,
  target, generation, or transition is a collision.
  A matching successor lifecycle without the complete durable proof never authorizes dispatch,
  replay, or terminal tracker release.
- The specialized backend request carries the exact CAS thread, expected active CAS turn, and one
  bounded protocol client-message correlation derived injectively from the original accepted-input
  identity. That correlation is not a second durable identity, does not prove content by itself,
  and is never allocated by a delivery retry.
- The canonical V1 steering correlation is the ASCII prefix
  `beryl.accepted-input.v1:` followed by the exact 16 accepted-input identity bytes as 32 lowercase
  hexadecimal digits. Any other length, prefix, character, or decoded identity is not a Beryl
  steering correlation.
- The successful steering response and matching `UserMessage` lifecycle have no guaranteed
  relative ordering: the response may precede delayed lifecycle, or both Started and Completed may
  precede the response. The response must name the expected active turn. Each lifecycle must name
  the same active CAS route and protocol correlation, then compare incrementally against a fresh
  pass over the exact accepted-input source. A missing, duplicate, cross-target, or
  content-disagreeing correlation is an incomplete-capture invariant failure; the system never
  keeps a request-sized echo or selects accepted input by content similarity.
- Successfully checking that delayed lifecycle publishes only the ordered app-owned correlation
  result and does not itself change accepted-input delivery state. An incomplete-capture invariant
  failure instead retires its unprovable production target and converges through the existing
  atomic active-abandonment transition: ready or retryable work becomes projection-lost next-turn
  work, while any delivering request whose provider acceptance remains possible becomes terminal
  delivery-unknown. The correlation boundary never keeps a retired projection active merely to
  defer that loss disposition.
- The checked Started-to-Completed tracker retains at most the two ordered results required by one
  exact lifecycle sequence, so a complete lifecycle can precede response delivery without
  backpressure becoming a false protocol failure. It remains sequence-terminal after its
  `Completed` result is consumed. Only the later owner that durably resolves the exact delivery
  route may explicitly release that terminal state; result consumption alone never admits a
  duplicate lifecycle.
- The delivery owner arms that tracker before replay preparation. Target loss closes further
  command authorization, waits for any already-selected delayed lifecycle publication, and then
  either observes the exact durable disposition or atomically replaces the steering attempt with
  generic-or-named loss-publication authority. A missing lifecycle has no elapsed-time retry
  inference: it remains pending while the exact target remains active and converges only when
  lifecycle evidence or authoritative target loss resolves it.
- Exact success, retry, or structured rejection is the final delivery-authority election. After
  that disposition commits, the delivery owner releases the checked tracker and finishes its
  attempt without replacing the disposition from later mutable target state. The atomic finish
  reports any deferred loss for ordinary convergence against the new durable state. A disposition
  that removed the last live steering route may preserve an independently proven terminal source
  outcome. Retry retains the live steering count, so the terminal gate cannot publish a proven
  terminal beside it; deferred loss instead reclassifies that input as projection lost. Only a
  path with no exact delivery disposition may transfer the attempt into loss-publication authority.
- The backend reports exact matching success, exact provider rejection, proven non-dispatch, and
  completion unknown as distinct outcomes. A matching success durably completes the live delivery
  route. Proven non-dispatch is dispatch evidence, not a transient-failure classification.
  Cancellation returns the original route to retryable state and parks it until an independent
  fresh cancellation-lifecycle or recovery wake. Every other current target-current source,
  authority, validation, serialization, lifecycle-arm, or command-authorization failure returns
  the route to safe retryable storage and then fails the exact active projection closed through
  existing atomic target-loss authority; this preserves the undispatched fragment as ordered
  next-turn work rather than fabricating delivery-unknown. Failure to complete that convergence
  fails the owning scheduler service closed. Neither path arms a timer or consumes repeated worker
  slots. Connection-invalidating pre-dispatch failure uses the same exact target-loss convergence.
  Before invalidating that connection, the sole foreground driver seals the armed no-lifecycle
  branch under the checked tracker from its exact nondispatch result. The delivery owner consumes
  that same seal after durable retry publication even if broker closure has already become visible;
  a lifecycle reservation that won first remains ambiguous and is never overwritten.
- A structured non-steerable rejection changes the named delivering record to retryable and moves
  it, with its original accepted-input identity, to the ordered next-turn queue under the explicit
  steering-rejected reason. It does not by itself retire an otherwise exact active projection.
- An exact rejection without a closed machine verdict proves that the named request was not
  accepted, but it does not prove that the expected active target remains current. Diagnostic
  message text cannot distinguish no-active-turn, expected-turn mismatch, or another target
  rejection. CAS-live therefore classifies the target as unconfirmed and performs one atomic
  active-abandonment transition: it retires the active binding and CAS thread, moves the exactly
  rejected fragment plus every other ready, retryable, or proven-undispatched fragment to ordered
  next-turn work under projection-lost authority, and terminalizes only sibling delivering work
  whose dispatch may have occurred as delivery-unknown. The named rejected fragment is never
  retried against the retired target and never becomes terminal merely because the rejection lacked
  a verdict.
- An independently proven stale steering target uses the same atomic abandonment disposition. No
  rejection or abandonment path drops, duplicates, merges, or allocates another accepted-input
  identity.
- `turn/steer` has no CAS idempotency key or authoritative delivery readback. Transport loss,
  timeout, malformed response, or response-identity failure after possible dispatch is not a
  steering rejection and never becomes retryable work. It produces the delivery-unknown outcome
  above and makes the projection's represented history unprovable.
- Steering never repeats a recovered-history prefix, branch-selection context, or per-turn
  developer instructions.
- Concurrent steering workers are bounded. Lack of worker capacity leaves admitted input durably
  scheduled for its exact steering target without accumulating resident tasks or payloads; a later
  bounded scheduler pass retries it only while that target remains steerable.

## Exact Stop Operations

- One stop operation targets one exact active provider operation. Its immutable target contains the
  Syndic thread and turn, ordinary-or-provider-operation kind, binding revision, execution
  snapshot, runtime and managed-process generation, loaded-thread generation, CAS thread, and
  one-way-published CAS turn. A missing CAS turn, changed kind, stale generation, or mismatched
  selected operation rejects admission; no field is inferred from status text or process state.
- Exact CAS 0.146.0 interruption remains on the conservative untargeted-core boundary unless
  retained release-scoped evidence proves one atomic targeted primitive after the app-server turn
  check. Beryl treats the selected parent operation as exact only because its authenticated managed
  listener is exclusive and the target-operation gate prohibits
  every successor turn or compaction start from the precheck through terminal or request
  disposition. Without that no-successor proof, interruption capability is unavailable.
- While the input gate is `Stopping`, its caller-supplied operation nonce selects one live
  stop-operation record by Syndic thread and nonce. The record contains that target, its monotonic
  revision, a fixed first-publication revision for every present member of the nonempty closed cause
  set, and at most one exact `DispatchClaimed(source_revision, attempt)` witness. Revision one is
  admission; every later revision is occupied exactly once by a new cause, the claim, or the
  consuming disposition. The gate and live record are one atomic invariant; neither is
  independently sufficient stop authority. Terminal, safe-reopen, and abandonment transitions
  retain the cause and claim provenance while consuming live authority into an inert record with
  the exact successor witness, preventing same-thread operation-nonce reuse without retaining
  backend dispatch capability.
- Stop-operation and dispatch-attempt nonces are distinct caller-owned 128-bit values allocated
  from the OS cryptographic random source. Durable operation identity combines the Syndic thread
  with its nonce, and an attempt is scoped to that operation. Neither nonce is derived from CAS ids,
  reused after rejection, or interpreted as a backend idempotency key; collision rejects the
  mutation.
- Stop causes distinguish deliberate selected-operation control, diagnostic control, window-close
  ownership, and Beryl-owned interrupting approval. A later caller targeting the identical
  operation atomically and monotonically adds its cause while joining the same record; a changed
  target is stale. Causes share one dispatch and lifecycle operation, and no caller issues another
  primary interruption merely because its cause or hard-escalation intent differs. Once
  interrupting-approval cause is present, safe reopening is prohibited. The cause's immutable
  first-publication revision lets that join reconcile exactly after any later compatible cause,
  dispatch claim, terminal, safe-reopen, or abandonment descendant.
- Cause joining is serialized with safe reopen, terminal consumption, and abandonment. It either
  publishes a compatible descendant of the still-current stop before the caller's external side
  effect, or observes the exact consumed successor and cannot authorize that side effect from stale
  ownership.
- The live target owns one non-cloneable target-operation election shared by steering claim,
  terminal handoff, and stop admission. Stop waits for an earlier steering attempt to reach its
  exact durable disposition. If target loss resolves that attempt, stop loses its target rather
  than sending to stale authority. Once stop wins, later steering claim and terminal handoff wait
  for or consume the stop cut.
- The process-local stop coordinator never retains its state mutex while waiting for that router
  election. It checks for a local owner, releases coordinator state, acquires the exact election,
  then reacquires and revalidates state: an owner installed in the interval is joined, otherwise
  admission proceeds. Terminal publication may therefore retain its source-publication permit while
  converging through coordinator state without forming a coordinator/router lock cycle.
- An unknown-terminal observation never manufactures a stop operation. If it wins while the target
  remains exact, it enters `AwaitingTerminal` without a stop nonce, cause, attempt, or interruption
  capability. A later stop request remains unavailable until exact late activation restores a
  fresh steerable target.
- Ordinary stop admission requires the exact active target and a selected steering generation with
  no `Delivering` leaf. It atomically publishes the stop record and `Stopping` gate, removes ready-
  source eligibility, and changes every `Admitted` or `Retryable` member of that generation to
  effective next-turn work under `Stop` without rewriting leaves. Provider-operation admission
  instead requires the compacting record, its published CAS turn, provider snapshot, and valid
  binding; it publishes the stop pair and marks the compaction record stopping without creating a
  steering generation or changing `NextTurn(Compaction)` work.
- Input accepted after either commit is immediately visible and enters a later durable next-turn
  generation selected by the stop target kind: `Stop` for ordinary execution and `Compaction` for
  a provider operation. It never extends a stopped steering generation, becomes a draft, or
  receives another accepted-input identity. Stop-period admission may advance thread, gate, route,
  and accepted-order revisions without changing the immutable stop target.
- A terminal observed before stop admission wins without a backend interruption. After admission,
  ordinary terminal consumes the stop and publishes `FinalizingHistory`; provider-operation
  terminal consumes the stop into the dedicated compaction-finalization successor and restores the
  compacting gate only for that bounded convergence. If ordinary terminal races ready steering
  without a prior local stop, its target-operation election performs the equivalent compact next-
  turn reclassification before publication.
- The stop coordinator claims one caller-generated dispatch-attempt identity durably before it
  authorizes any request byte. Exact reconciliation classifies an ambiguous home commit as prior,
  exact, or collision from the operation, attempt, gate, target, route successor, and stop-record
  revisions. Only the same live caller that supplied an exact claimed attempt may hand that attempt
  to the connection driver.
- Stop admission, join, claim, and `begin_dispatch` revalidate the exact live-command generation
  inside the coordinator-state mutex shared with persistent-failure freeze. Whichever side
  establishes that mutex fence first owns the transition: admitted command work may finish before
  the freeze, while a failure-first fence returns home-authority loss without durable or dispatch
  advancement. A gate sample taken before the mutex cannot authorize later work.
- The sole foreground connection driver consumes that non-cloneable attempt and issues
  `turn/interrupt` on the already authenticated loaded session. Its command authorization is
  serialized with provider polling, approval responses, target closure, and terminal handoff. A
  detached request-only client, selected UI strings, or a newly resumed session cannot perform the
  primary interruption.
- Dispatch and terminal handoff have one linearized cut. If matching terminal publication wins
  before the first request byte, it consumes the stop and revokes the undelivered attempt. For a
  pinned accepted interrupt, app-server queues the matching response before its resulting terminal
  notification. A terminal already progressing before handler admission has no fixed ordering
  against the later rejection response and may instead consume the stop first. Every response
  reconciles against whichever exact stop or terminal successor won and cannot reopen a terminal,
  recreate the stop record, or change lifecycle.
- A matching empty response proves only request acceptance. Exact provider terminal evidence still
  owns interrupted, failed, or completed lifecycle. If active projection authority is lost first,
  the ordinary abandonment and source-less incomplete path owns convergence; interruption
  acknowledgement never advances represented history.
- A local failure proven to precede every request byte is safe nondispatch. Without an attached
  hard escalation and without interrupting-approval cause, it atomically consumes the record's live
  authority into its target-kind-specific safe-reopen receipt. An ordinary target restores a fresh
  empty steering generation. A provider-operation target restores its exact compacting gate and
  compaction record without any steering route. All work already classified next-turn during the
  stop interval stays next-turn in accepted order and is never retroactively steered. A local
  nondispatch after permission denial instead retires the projection through stop abandonment
  because that safety obligation cannot be waived.
- Under exact CAS 0.146.0, matching `-32600` interruption rejection with absent error data or the
  handler-local `-32603` submission-failure response with absent error data proves that app-server
  did not enqueue the core interrupt only when retained release-scoped source evidence establishes
  that handler behavior; it supplies no machine-readable cause or proof that the
  requested target remains current. Beryl never parses its diagnostic text or safely reopens from
  that response. Terminal publication may win if already observed; otherwise the target is
  unconfirmed and the stop operation atomically retires the projection, preserves next-turn work,
  and converges source-less incomplete history.
- When hard escalation is already attached, safe primary nondispatch keeps the durable stop cut
  closed while the frozen escalation snapshot runs once. After that bounded run, the same proof may
  safely reopen only if interrupting-approval cause is absent and terminal, target loss, and another
  exact disposition have not won. A crash before reopening loses the process-local escalation state
  and follows conservative startup abandonment; it never repeats the primary request or hard
  targets.
- Once any request byte may have crossed the transport, timeout, cancellation, malformed response,
  response-identity failure, transport loss, or lost caller observation leaves
  `DispatchClaimed(source_revision, attempt)` and `Stopping` durable. No automatic, visible-control,
  diagnostic, window-close, approval, or restart path repeats the primary interruption. Exact
  terminal or projection-loss evidence is the only forward convergence.
- Restart never reconstructs a live dispatch capability from a stop record. Startup classifies the
  exact active `Stopping` target, atomically retires its projection through the stored target,
  publishes source-less authority-lost convergence, resumes the target-kind-specific ordinary or
  provider-operation finalization, and opens accepted scheduling only after the existing startup
  fence reaches its fixed point.
- Every Beryl-owned interrupt while the home is healthy uses this operation. Interrupting approval
  denial admits or joins stop and adds its durable cause before the denial write. If no primary
  request byte has crossed, the same attempt dispatches once after denial; if that attempt is
  already awaiting its response, the approval joins its cut and creates no second request. Failure
  to establish durable stop ownership retires the exact connection without fabricating a denial or
  interruption outcome.
- Diagnostic stop differs only in deliberate activation, and window close differs only by waiting
  for terminal-history convergence before releasing its claim. Both join an existing exact stop.
  Persistent home failure is separate: after durable admission is unavailable, Beryl closes new
  live-command authorization and may make one volatile best-effort request from a last coherent
  exact target only when retained dispatch evidence proves that no earlier primary interruption
  may have crossed. A fixed failure-generation guard prevents another volatile request. This path
  cannot create, confirm, duplicate, retry, or release durable stop state.
- Exact failed-health observation synchronously fences live-command admission before its
  nonblocking one-shot worker signal. Existing permits become stale; store-error paths observe the
  typed health fence before ordinary router, stop, loss, settlement, or retirement cleanup. The
  dedicated worker alone allocates the failure generation, drains admitted commands, freezes exact
  evidence, and spends any eligible volatile authorization.
- A fail-closed HomeStore writer panic in the complete live-source publication transaction,
  including pending-turn activation and later source-event publication, is contained before it can
  unwind the capacity-one provider ingester or strand its sole acknowledgement. While it still
  owns the exact operation, the ingester elects the typed cut, settles the nested source-publication
  permit onto the failure side, releases that permit and its outer operation permit from the drain
  count, and only then installs the terminal acknowledgement. The failure worker retains its
  strict full drain with no timeout or provider-originated self-exemption.
- Failure observation and ordinary service close are one gate election. A close that wins first
  performs ordinary retirement. A failure that wins first makes close return a consuming retained-
  cut handoff instead: the exact home, connections, router evidence, bounded results, and returned
  pre-activation loaded projections remain owned without connection shutdown or home close.
  Quarantine sealing, connection-epoch adoption, generation reauthentication, old-service
  retirement, and recovery publication are later stages and cannot occur while constructing this
  handoff.
- Every router admission, dispatch authorization, publication, finish, and abandonment mutation
  holds its router lane and commits through the exact scoped live-command permit. Long-lived or
  destructor-owned capabilities settle under that lane and the master gate, choosing bounded
  ordinary cleanup or exact failure preservation once. A router wait never retains a drain-counted
  permit while depending on failure freeze; it releases admission before waiting and leaves on the
  bounded failure-fence wake path.
- Each mounted router admits at most 64 exact live targets. The failure worker sorts and freezes
  every admitted CAS-thread key from each router. A candidate is dispatchable only when its home,
  service, failure, connection, runtime, process, loaded generation, Syndic owner and pending turn,
  CAS thread and turn, ordinary activation, timeout, and unspent guard all agree. Active-only,
  awaiting-activation, context-compaction, terminal, closing, lost, publication-in-flight, active-
  operation, identity-mismatched, generation-mismatched, and prior-primary-ambiguous candidates
  produce closed no-dispatch reasons. Spending one guard is irreversible and cannot fall through to
  another target.
- Worker-derived pre-activation capabilities, frozen live targets, cleanup owners, and scheduled-
  promotion barriers that already crossed admission have no failure-time capacity decision. A
  worker-derived surrender child, router-bounded target, or exact retained token moves into the
  coordinator, so failure cannot discard authority, leak it through `forget`, or reconstruct it
  from counts. The lowest loaded-registry lease layer performs this transfer when no public
  projection wrapper exists. Retained projection authority is bounded by admitted workers plus at
  most 64 targets per mounted router and cannot grow with failure-signal repetition. An API that
  lacks a live bounded worker admission cannot create a recovery-eligible pre-activation
  projection. The service reserves its unique failure-escrow identity before mounting the cut;
  failure fills that exact cell, while constructor unwind or ordinary close releases only the
  pointer-identical still-empty reservation.
- Consuming a finished handoff checks out that exact filled escrow slot and creates one non-cloneable
  recovery inventory without releasing its identity reservation. Before the retained vectors are
  sealed, the old accepted-input scheduler is cancelled and joined so no admitted worker can
  publish a later surrender. A joined scheduler exit caused only by the exact failed home generation
  is valid quiescence; unrelated scheduler failure or a scheduler panic is not. The inventory
  preserves every complete pre-activation wrapper,
  retained barrier, and exact connection needed by that bounded set; it never selects a single
  wrapper merely because several survived. Inventory drop re-escrows those owners in memory. Only a
  stable inventory may be consumed into the grouped quarantine. An incomplete cut remains inert
  retained failure authority and cannot become a recovery inventory.
- Public loaded-projection and same-native-anchor metadata crosses the cut in the same lowest
  connection-authority settlement as its raw lease. Failure-first reconstructs and retains the
  complete wrapper, including exact binding, execution, lineage, home, and Syndic facts; it cannot
  degrade to a raw loaded or quarantine token because a wrapper-level failure sample raced the
  lower settlement. Callbacks already under the master gate use identity-only retainer entry points
  and never reacquire the gate.
- Ordinary close detaches store-bearing connection internals before a stale public session shell
  can survive it. Broker cancellation and join, driver join, and bounded page-diagnostic capture
  consume those owners; the remaining shell carries identity and retired authority but no
  `HomeStore` owner. Persistent-failure close does not run that destructive detachment and instead
  preserves the exact mounted connection for recovery.
- Implicit drop performs only bounded in-memory settlement: it may close admission, preserve
  conservative orphan authority, request cancellation or retirement, wake workers, and detach
  handles. It never waits, joins, invokes provider shutdown, performs backend or durable-store I/O,
  or closes the home. Explicit consuming close owns those effects. A failure-winning implicit
  service drop fills its pre-reserved escrow without waiting, while an ordinary last-owner drop
  propagates a nonblocking retirement signal after releasing every authority lock.
- Stop admission cancels any process-owned automatic lifecycle continuation attached to the target
  turn. It does not delete or reorder separately accepted input, and it never starts compaction or
  a replacement continuation turn.

### Hard-Stop Escalation

- Hard stop attaches one deliberate escalation to the same primary stop operation. It never creates
  a second stop record or a second selected-operation `turn/interrupt`.
- One fixed process-local escalation slot is keyed by the stop-operation identity. Duplicate hard
  callers join its running or finished bounded result; they cannot refresh the snapshot or run any
  target again.
- The slot lives only while that stop remains current or its terminal successor retains the
  finalization-release hold. After the hard result has reached every already joined caller, the
  bounded status-operation owner has accepted its feedback, and the stop successor no longer needs
  the hold, the app releases the slot and snapshot. A later stale caller observes the consumed stop
  identity and cannot recreate the escalation. Resident slots are therefore bounded by live
  foreground stop registrations rather than historical stop count.
- Escalation attachment is serialized with primary-outcome publication. It either joins the still-
  current stop before a safe outcome reopens it, or observes that operation as finished and must
  begin a fresh deliberate stop against the still-exact target. It cannot retain a snapshot against
  a consumed stop record.
- A first attachment may occur after a prior soft-stop response was confirmed accepted. Confirmed
  acceptance remains distinct from completion unknown in process-local ownership: neither permits
  another primary interruption, but only confirmed acceptance may reserve the sole late hard
  continuation while the exact foreground connection remains authoritative.
- Attachment also reserves one non-cloneable hard-run continuation on the foreground driver and
  one finalization-release hold keyed by the stop identity. After a primary outcome authorizes
  escalation, the driver services that continuation before unrelated polling or commands. No
  detached task may race terminal handling to reacquire the session.
- A continuation attached before primary settlement inherits the original target-operation election
  across that settlement, retains it through fresh backend no-successor authorization, and releases
  it before waiting for cleanup response. A continuation attached after confirmed acceptance uses a
  fresh exact router election on that same driver because the primary election has already settled.
  Completion-unknown primary interruption retires both foreground session authority and app-side
  router/registry authority unconditionally rather than consulting a generic error predicate.
- After durable stop admission and while the exact live target registration remains, the app
  freezes one at-most-64-entry deduplicated snapshot of backend-exposed active handles associated
  with that CAS thread, CAS turn, loaded generation, and provider item. Eligible entries are exact
  child or subagent thread-and-turn pairs only when the release supplies a truly targeted
  interruption primitive and Beryl owns its target fence, turn-owned process-instance handles only
  when the provider supplies a lifetime-stable identity and atomic targeted termination, and at
  most one supported thread-scoped background-terminal cleanup target.
- Exact CAS 0.146.0 cannot admit child or subagent turn pairs unless retained release-scoped evidence
  proves an atomic targeted child primitive and Beryl can prohibit internally scheduled child
  successors across the required cut. The frozen result records that closed unsupported
  limitation.
- Exact CAS 0.146.0 supplies no eligible individual turn-process handle unless retained release-
  scoped evidence proves an ABA-safe identity.
  `command/exec/terminate` uses an unrelated standalone connection-owned namespace, while
  `thread/backgroundTerminals/terminate` compares only a reusable numeric process id and cannot
  atomically compare the provider item id. A frozen id can address a later process after ABA reuse,
  so Beryl records an identity-unsafe unsupported limitation and never dispatches the individual
  method. A prior background-terminal list read is a TOCTOU check, not a repair.
- The optional `thread/backgroundTerminals/clean` target is explicitly coarse. It names only the
  exact loaded thread and is admitted only when normalized selected-operation activity shows at
  least one still-active turn command at snapshot time. It executes after all individually
  addressable frozen targets and may drain every unified-exec process present in that thread when
  core handles the request. Its `{}` response proves request acceptance only, never per-process
  termination or cleanup completion. Beryl never represents it as a frozen per-process set or
  selected-turn-only effect.
- Pinned same-loaded-session ordering nevertheless makes accepted cleanup a safe successor barrier:
  the handler enqueues cleanup before returning `{}`, and the sole core submission loop fully
  handles that queued op before receiving a later Beryl op. Combined with the no-successor fence,
  any later Beryl start or steering request submitted only after the response cannot overtake
  cleanup. A lost or replaced session supplies no completion claim; its old authority is retired
  rather than transferred as a cleanup proof.
- After experimental capability admission and local thread validation, a pinned coarse-cleanup
  JSON-RPC error cannot distinguish unloaded-thread, capability, or core-channel authority without
  diagnostic text. Beryl retires the parent projection and marks later frozen targets unavailable;
  it never parses the message to continue. An exact parent terminal already published while the
  request was pending remains authoritative and is never downgraded to source-less incomplete.
- Handle membership comes only from normalized provider activity admitted for that target.
  Completion, target closure, generation change, missing child-turn identity, capacity overflow,
  or unsupported capability becomes a bounded limitation; no command text, process lookup,
  working directory, name, or historical scan may synthesize a target.
- The frozen snapshot is process-local best-effort authority and is never recovered or refreshed
  after primary stop admission. Capacity overflow retains checked omitted-active counts and closed
  limitation flags rather than another handle or one record per omission. Logical operational
  history may be arbitrary, but resident active handles, reported limitations, request workers, and
  result records remain fixed-capacity.
- Escalation begins only after matching primary response acceptance or local proven nondispatch
  while the same exact foreground session and parent target remain authorized. Each frozen target
  is attempted at most once in frozen normalized provider-observation order with stable kind and
  handle tie-breaks; the single coarse cleanup target is always last. Every target preserves its
  own dispatch and response result and cannot fabricate terminal state for the selected parent
  operation. One target failure does not suppress later targets unless exact connection loss makes
  them unavailable.
- `RejectedBeforeCoreInterrupt` invalidates current parent-target proof, while completion unknown
  retires the sole foreground session. Either outcome marks every unattempted frozen target
  unavailable without a hard-target request. Escalation never reconnects or uses a detached client
  to work around lost dispatch authority.
- Escalation requests use the same exact foreground driver and loaded connection generation as the
  primary stop. They never reconnect, resume, or create a detached request-only session merely to
  reach another frozen handle. A continuation attached after primary acceptance queues only its
  hard targets, freshly binds and authorizes the same exact foreground turn, and never repeats
  `turn/interrupt`.
- A terminal notification interleaved while a hard-target response is pending is ingested and may
  durably consume the stop into ordinary `FinalizingHistory` or the dedicated provider-operation
  terminal successor; provider ingress never waits for the hard runner. The terminal successor
  retains the stop identity, and the attached process-local hold prevents finalization release to
  idle or accepted-next promotion until the hard run settles.
  Process loss drops the non-recoverable hold and hard snapshot, after which startup may finish the
  durable terminal fixed point without replaying a hard request.
- Safe primary nondispatch holds the stop gate until the escalation run finishes, then invokes the
  target-kind-specific exact safe-reopen mutation if the blocked operation remains live and
  interrupting-approval cause is absent. Matching primary acceptance, possible dispatch, or any
  interrupting-approval cause leaves the stop durable for terminal or authority-loss convergence
  regardless of hard-target results.
- A provider-operation target such as context compaction owns no ordinary parent-turn command or
  subagent handles. Pinned coarse cleanup is ineligible because no selected-operation active turn
  command authenticates its thread-wide effect. Pinned compaction hard escalation therefore has no
  target beyond its primary interruption; a future additional provider-operation target requires
  its own exact release-proven primitive.

## Context Compaction

- CAS compaction is a provider operation targeted at one exact exclusive CAS thread. Manual
  admission requires the selected idle Syndic thread, an exact valid binding, the authoritative
  loaded foreground connection generation, no same-thread target operation, and zero effective
  accepted-next input. Automatic lifecycle admission has the same durable requirements plus one
  process-local continuation intent for the exact terminal yielding turn.
- Admission allocates distinct cryptographically random 128-bit operation and request-attempt
  nonces. The provider-operation `SyndicTurnId` is the operation nonce's exact 16-byte payload under
  the turn-id type; a collision with any existing turn rejects the whole admission. The app derives
  the `SyndicExecutionSnapshotId` by truncating SHA-256 over the domain
  `beryl.syndic.compaction-snapshot.v1` plus exact home id, thread, operation and
  turn, source gate and binding revisions, represented-prefix proof, CAS thread, and loaded process
  and thread generations. Storage allocates none of these identities and rejects any disagreement
  or collision.
- A random operation-nonce collision may be replaced only after the admission classifier proves
  that the gate and every derived turn, snapshot, and operation identity remained absent. An
  ambiguous admission reuses the original tuple for reconciliation and never rotates any identity.
- One mutation atomically creates that parentless `ContextCompaction` provider-operation turn and
  provider-operation execution snapshot, the durable compaction record, and the compacting input
  gate. The record retains the exact `BerylHomeId` from the authenticated healthy-home admission
  and snapshots the valid binding revision, represented-prefix proof, runtime and managed-process
  generation, loaded-thread generation, and CAS thread. The app snapshots the feature-owned
  timeout process-locally; it is not durable operation authority.
- Pinned CAS does not reject compact-start against an active thread; its core replaces the current
  task. The idle precondition, durable compacting gate, and no-successor election are therefore
  required safety authority. Beryl never uses provider replacement as stop, collision recovery, or
  serialization.
- The provider-operation turn is canonical ownership for the streamed context-compaction item and
  lifecycle. It does not advance the Syndic committed tail, selected path, current draft,
  represented prefix, or native CAS model-turn count. The valid binding remains the CAS lineage
  authority while the compaction record exclusively prevents another same-thread operation.
- Before any request byte may be issued, one exact mutation changes the record from `Admitted` to
  `DispatchClaimed(source_revision, attempt)`. That attempt owns the sole non-cloneable dispatch
  capability. It is never replayed or used as a provider idempotency key.
- The authenticated foreground connection driver performs the sole `thread/compact/start` request
  under the same target-operation election used by stop and terminal handoff. It passes only the
  exact CAS thread id. No detached connector, resumed substitute session, shell worker, or
  successor operation may dispatch it. Admission also proves that this foreground client already
  owns the exact thread subscription; compact-start does not subscribe its request connection.
- Compatibility admission requires retained exact-0.146.0 evidence that CAS acknowledges
  `thread/compact/start` with an empty successful result only after its core submission channel
  accepts the generated compaction task. It reports progress through
  the standard turn and item stream. Request acknowledgement supplies no CAS turn identity, has no
  enforced order relative to subscriber lifecycle notifications, and is not compaction completion.
  The app records request disposition independently from ordered provider observations so
  `turn/started`, item activity, or terminal evidence may arrive before the matching
  acknowledgement without losing exact correlation.
- Matching `turn/started` one-way publishes the CAS turn against the provider-operation snapshot.
  Before that publication, the ordered broker may correlate only the pinned thread-scoped `active`
  status to the exact CAS thread under the exclusive compaction target. A provider item whose
  begin-known kind is `ContextCompaction` enters a fixed-resident exact-schema marker parser with a
  bounded 256-byte external identity and provider timestamp; it never creates an unpublished
  provider-observation build or chunk. Its authenticated seal publishes only the dedicated marker
  mutation. The broker retains the status frontier required to distinguish later idle from
  `systemError`. An item before matching turn publication, a second CAS turn, wrong identity,
  conflicting replay, unsupported provider item, or source-order gap is fail-closed provider
  authority loss, not a guessed compaction result.
- Exact success requires a durably completed `ContextCompaction` item followed in stream order by a
  matching successful `turn/completed`. In the pinned task lifecycle, successful completion emits
  exact thread idle before that terminal event, so the terminal proves the named compaction turn
  already crossed idle; no independent `thread/read` poll may race or replace it. Item completion
  alone, empty request acknowledgement alone, timeout, or an unmatched idle status is insufficient.
- The compaction record's exact matching successful terminal witness is canonical terminal source
  authority for the provider-operation turn. It records the terminal status and the exact
  turn-state revision established by that observation; storage does not duplicate it as an
  ordinary `TurnEnded` source event. Durable validation accepts the resulting complete
  zero-source-event turn only when one record's target, terminal, and recorded revision agree with
  it exactly. A missing record, another terminal source, or any disagreement is corruption.
- The provider-operation turn retains exact interrupted or failed terminal status when observed.
  A clean pinned interruption normally publishes idle, but interrupted terminal alone is not
  unconditional idle proof because a prior system-error state can survive task clearing. It may
  preserve the valid binding only when exact ordered status evidence separately proves idle;
  otherwise it retires authority. It never counts as compaction success and retains the release-
  pinned `ForcedAbortOrderingUnproven` history-incomplete reason; any later same-target event
  retires the connection rather than reopening the provider-operation turn. Pinned failure follows
  `systemError` rather than idle and
  therefore consumes failure while retiring the connection and binding. A successful terminal
  without the required completed marker is protocol incompatibility and likewise retires uncertain
  authority. Captured exact item evidence remains durable and bounded in every case.
- Proven local nondispatch consumes the admitted operation as failed, preserves the still-exact
  valid binding, and releases its queue-only gate. A source-pinned request rejection proves no core
  compaction was accepted but does not prove that the requested loaded target remains current, so
  it retires the connection and binding and converges the provider-operation turn incomplete.
  Timeout, transport loss, malformed response, or any other result after possible dispatch is
  completion unknown: Beryl retires authority, never retries compaction, and converges the turn
  incomplete while preserving accepted input.
- Request disposition reconciles against either the live record or its immutable consumed
  successor plus the independently keyed settlement receipt that fixes the exact historical gate
  transition and is fully committed by the consumed operation. The reconciliation read also
  authenticates the concrete settlement-specific lifecycle, binding, accepted-work, or
  continuation successor rather than relying on receipt self-consistency. If exact provider
  terminal settled first, a later matching empty acknowledgement is
  a compatible no-op; it does not reopen the gate or start a timeout. A same-attempt completion-
  unknown result after that successor likewise preserves the exact terminal lifecycle, gate, and
  terminal-chosen valid-or-stale binding disposition, but retires the unusable foreground
  connection and never retries. A later rejection, proven nondispatch, conflicting attempt, or
  operation/receipt mismatch contradicts the terminal and is an invariant failure. Exact terminal
  evidence may therefore win even when response delivery is lost without transferring connection
  authority or making the request retryable.
- User input accepted while the gate is compacting is durably ordered as ordinary next-turn work
  and is never sent as active-turn steering. Compaction terminalization uses its dedicated
  provider-operation settlement and bounded item finalization; generic ordinary terminal handling
  cannot expose the provider-operation turn as recovered pending work or rebuild the selected
  conversation transcript from it.
- Before CAS publishes the exact compaction turn id, ordinary stop admission is ineligible and no
  backend interruption is sent. An operation still in `Admitted` may be cancelled locally by its
  owning lifecycle before dispatch; after dispatch claim, a caller without the CAS turn can only
  retire its own session or wait for exact evidence. Once the CAS turn is durably published, stop
  admission targets the provider-operation snapshot and moves the gate to stopping while retaining
  the compaction record as the blocked operation. Matching terminal or authority-loss convergence
  consumes both authorities exactly once.
- Compaction-stop admission does not rewrite existing or later accepted input from
  `NextTurn(Compaction)` to ordinary stop routing. If the primary interrupt is proven locally
  undispatched and the exact compaction target remains live, the provider-operation safe-reopen
  mutation consumes the stop record and restores `Compacting(operation, provider turn)` plus the
  same compaction record. It creates no steering generation and does not repeat compact-start. A
  matching provider terminal instead consumes the stop into the dedicated compaction finalization
  successor; rejection, possible dispatch with authority loss, or restart abandons both live
  authorities and never reopens. Each provider-specific stop successor retains the exact
  `Stopping` compaction source revision and its immediate safe-reopened, finalizing, or consumed
  successor revision, reauthenticates the ordered stopping ancestry, and admits later provider
  descendants only through their retained request/event frontier. Abandonment also matches that cut
  exactly to the immutable settlement receipt.
- Pinned hard escalation for a compaction stop owns no child, command, individual process, or
  coarse thread-cleanup target. It may perform only the same exact primary interruption, as defined
  by the stop contract, and reports the closed unsupported target set without borrowing ordinary-
  turn activity.
- The feature-owned completion timeout is a process-local observation deadline whose timer starts
  at exact request acceptance. Expiry publishes bounded feedback but performs no durable lifecycle
  mutation, request retry, interruption, session retirement, gate release, or continuation
  cancellation. Exact later terminal or authority-loss evidence remains authoritative.
- CAS compacted internal history is not Syndic history and cannot become Beryl's durable-history
  authority. After exact successful settlement, the still-valid CAS thread continues through its
  native compacted lineage against the same represented Syndic prefix without recovery injection.

## Automatic Lifecycle Continuation

- The lifecycle intent is process-local and keyed to the exact yielding Syndic and CAS turn. Yield
  acknowledgement creates no durable work by itself. Exact stop admission cancels the intent, and
  process loss drops it without reconstruction from transcript or provider-operation records.
- A healthy-home window-close barrier synchronously cancels the matching intent in the process
  coordinator before it classifies stop eligibility or waits for finalization. This cancellation
  applies while the yielding turn is active, terminal-history finalization is pending, automatic
  compaction is awaiting CAS turn publication, or compaction is already live. It does not require
  process exit and cannot be undone by a later terminal, timeout, or compaction success. An already
  admitted compaction settles without creating continuation, while the close barrier separately
  stops or retires that operation through its exact available authority.
- After the yielding turn reaches terminal-history fixed point, compaction admission atomically
  checks the current idle gate and accepted-next aggregate. If user input is already effective, the
  admission returns that closed outcome without creating a compaction record; the app consumes the
  lifecycle intent and wakes ordinary accepted-next scheduling.
- A surviving intent attaches process-locally to the admitted compaction operation. Input accepted
  later remains durable queue-only work. Compaction timeout does not detach the intent, while stop,
  exact compaction failure, non-success terminal, authority loss, or connection loss consumes it.
- Successful settlement performs one serialized durable choice under the exact compaction record,
  gate, accepted-route, thread, tail, current-draft reverse binding, and valid CAS binding
  revisions. Existing accepted-next work wins: settlement consumes the compaction record, returns
  the gate to idle, consumes the process-local intent, and emits accepted-next readiness without
  changing the draft.
- Before attempting the no-user-work branch, the app stages and seals the fixed continuation as one
  ownerless content-addressed manifest through the existing bounded content builder. Its canonical
  input is exactly one UTF-8 text atom containing `Continue from the root doc/plan.md.`, with zero
  markers and the exact empty asset-set proof. The `SyndicContentId` is the ordinary full content
  digest. A staged manifest left unreachable because user work wins or the process exits remains
  inert under the existing no-deletion rule. The coordinator revalidates the non-cloneable
  lifecycle intent after staging and before settlement; close or stop cancellation leaves the
  candidate unreachable and cannot admit a continuation.
- Candidate staging reconciles only by the derived content identity, exact frontier, digest, and
  fixed shape. Exact sealed state continues; a collision fails the coordinator closed. A
  definitive preparation failure consumes the lifecycle intent and uses ordinary successful
  compaction settlement without a continuation so accepted work can proceed; Beryl reports the
  bounded continuation failure. Beryl-home failure instead follows the home failure barrier and
  cannot fabricate either settlement or a different input.
- If no accepted-next work exists at that serialization cut, the same mutation consumes the
  compaction record and creates one pending conversation turn parented to the current committed
  tail plus one canonical user-role item referencing that sealed content and empty asset proof.
  The app derives the turn and item ids by truncating separate SHA-256 hashes over domain tags
  `beryl.syndic.lifecycle-continuation.turn.v1` and
  `beryl.syndic.lifecycle-continuation.item.v1` plus the admission home id, Syndic thread,
  compaction-operation nonce, and fixed-content digest. Settlement accepts no home-id input;
  storage derives its verification domain from the durable compaction record and verifies both
  identities, exact content shape and digest, absence of markers/assets, and identity
  noncollision. The turn origin is
  `BerylLifecycleContinuation`; the mutation advances the Syndic tail and gate without reading,
  replacing, or clearing the current composer draft.
- The accepted-input scheduler never fabricates this Beryl-origin turn from a user receipt. After
  atomic admission, the ordinary protected execution lane dispatches it as a normal conversation
  turn and uses the latest applied non-empty developer instructions at dispatch. CAS-turn
  publication records the checked next native count, while only exact active-to-valid terminal
  convergence increments that count once under the ordinary binding invariant. A concurrent later
  user submission observes the pending or active continuation and enters permanent accepted-next
  order behind it.
- An ambiguous local result from the settlement mutation is reconciled by the exact compaction-
  operation, turn, item, content, gate, and user-work-won successor identities. If the continuation
  committed, Beryl resumes that turn; if user work won, it wakes that work. It never creates a
  compensating second continuation.

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
- One process-owned startup recovery fence precedes accepted-input scheduling and new input
  admission for the opened Beryl home. Recovery discovers work by bounded forward pages over the
  existing one-row-per-thread input-gate family; it does not maintain a second durable recovery
  queue or mirror accepted-input identities in process memory. The home-identity-bound cursor
  advances by stable thread identity while the startup owner is exclusive, so recovery's own
  same-key mutations cannot hide an unvisited thread.
- Each discovered non-idle gate is reclassified from one stabilized, bounded set of current gate,
  blocking-turn state, binding head and binding, execution snapshot, active CAS-turn identity, and
  selected-route evidence. A disagreement between the source row and the stabilized facts is
  retried from durable authority; a coherent but unsupported combination is corruption and fails
  the service closed.
- Accepted-input delivery recovery retries only work proven not dispatched or exactly rejected. It
  never converts an ambiguous dispatched `turn/start` or `turn/steer` into retryable work. After authority-lost
  convergence, fresh projection recovery for an already admitted pending successor may include the
  exact eligible interrupted predecessor as the authority-lost tail context defined by fresh
  projection recovery. Projection establishment itself starts no replacement model turn, and only
  the distinct pending successor may proceed through its existing ordinary `turn/start`.
  Cancellation, exact connection retirement, or expected obsolete-service drift may park that
  attempt; every other projection refusal fails the owning scheduler closed rather than
  presenting an unstarted durable turn as successfully handled.
- A `PendingTurn` whose blocking turn is the current incomplete committed tail, with pending,
  source-free turn state, no selected active route, and a non-active binding is proven
  undispatched. Restart rediscovers it through a separate revision-bound pending-turn source view
  over the same input-gate family and executes that existing turn through the ordinary projection
  path. It never promotes another accepted input or creates replacement turn identity. A valid
  activation cancellation has this shape.
- The recovered-pending scheduler advances its physical thread cursor before an attempted worker
  can invalidate the Syndic revision. Work-neutral drift and that worker's own completion may
  rebind the advanced floor for the remainder of the current sweep, preventing automatic retry of
  the same safe-pending source. Every independent owner that can make work eligible behind the
  floor publishes a fresh execution-readiness wake, which discards the floor and begins a complete
  scan. The coalesced signal is consumed only by the scheduler: readiness observed before a read
  resets first, and readiness arriving during stale handling remains pending for the next loop.
- Durable binding activation is the restart dispatch boundary. An active binding is possible-
  dispatch evidence even when no active CAS-turn identity or activation source event exists,
  because the process may have failed after a request byte crossed the transport. Startup
  reconstructs the exact active authority, atomically abandons it with generic projection-loss
  disposition, and then publishes one source-less incomplete terminal. It then runs the ordinary
  bounded terminal-history convergence over only the already captured item evidence, so completed
  items become finalized before scheduler handoff. It never invents a missing provider item,
  replays that `turn/start`, or replays any `Delivering` steering fragment.
- An `AwaitingTerminal` gate at startup is active possible-dispatch authority, not a resumable
  terminal wait. Its selected route retains the exact pre-uncertainty steering target, allowing the
  same generic active-abandonment transition to retire that projection without scanning members.
  Work already effective under `UnknownTerminal` remains ordered next-turn work; no interrupt,
  steering request, late-activation synthesis, or session resume is attempted.
- A stale binding, `PendingTurn` gate, and selected projection-loss route for the same blocking turn
  prove that active abandonment committed but source-less terminal publication did not. Recovery
  completes that terminal publication and the same bounded terminal-history convergence. Admitted
  or retryable steering fragments remain ordered projection-lost next-turn work, while a fragment
  that may have been delivered remains terminal delivery-unknown.
- Every ordinary proven-terminal publication atomically moves the input gate to
  `FinalizingHistory(turn)` while closing source admission, advancing binding authority, and
  marking affected projections stale. It does not make the gate idle before the bounded
  terminal-history pipeline has durably settled.
- Terminal-history finalization remains the sole durable convergence obligation. The same-thread
  owner resumably freezes and finalizes immediately eligible captured items and rebuilds the
  selected transcript. One exact completion command then proves the gate still owns that
  proven-terminal turn, the selected transcript is current, and the item frontier is either fully
  finalized or stopped at an explicitly non-finalizable captured item or pending-resource
  disposition. Path-neutral queued admission may advance the broad thread revision and the
  finalizing gate without superseding an active or completed transcript build. At writer
  serialization the completion command consumes the current compatible finalizing-gate descendant
  and atomically changes only that gate to idle, preserving every concurrently admitted route,
  high-water mark, counter, and byte total.
- A parallel active-steering worker may win target-loss publication, but it owns no projection
  flight and therefore never consumes the resulting history obligation. Router invalidation wakes
  the ordinary capture owner, which retains the same-thread flight and performs the sole live
  convergence; startup is the only fallback if that owner does not survive.
- An abandoned binding is historical after that exact release. Reopen validation re-proves the
  terminal fixed point when the abandoned turn is still the committed tail and the current gate is
  idle; after a successor advances the tail, ordinary current-gate ordering controls the successor.
  Historical binding evidence never requires the replaceable current gate to remain in an obsolete
  finalization phase.
- Startup pages include `FinalizingHistory` gates. Recovery resumes their existing convergence
  obligation without publishing another terminal event. Repeating restart before abandonment,
  between abandonment and terminal publication, during item or transcript convergence, after
  convergence but before gate release, after release, or after accepted-input promotion therefore
  converges to the same durable result.
- Startup classifies a compacting gate only with its exact current compaction record, provider-
  operation turn and snapshot, binding head, CAS reverse authority, request claim/disposition,
  optional published CAS turn, marker, terminal state, and accepted-route aggregates. A missing or
  disagreeing half, nonce reuse, impossible transition order, or provider observation outside the
  exact target is coherent corruption rather than repair authority.
- An `Admitted` compaction record proves that no request attempt was authorized. Startup consumes
  it as cancelled-before-dispatch, preserves the exact valid binding, finalizes the source-free
  provider-operation turn as failed, releases the gate, and retains all accepted-next work. It
  does not allocate or dispatch an attempt.
- `DispatchClaimed` with no request disposition, exact acceptance, or completion unknown is
  possible-dispatch evidence even when no CAS turn, marker, or provider source event exists. If no
  exact terminal successor was already durably observed, startup retires the old connection and
  binding authority, consumes the compaction operation as authority lost, publishes the provider-
  operation turn incomplete, finalizes only captured exact items, and releases queued work after
  the same-thread fixed point. Marker or CAS-turn publication, completion-wait timeout, and
  response loss do not make this state replayable. Pinned rejection separately proves no core
  admission but still retires the no-longer-proven loaded target and is never retried.
- A claimed record with durably proven local nondispatch is the sole safe claimed restart case.
  Startup finishes the same failed-operation consumption as live reconciliation, preserves the
  exact valid binding, releases the gate after bounded provider-operation finalization, and never
  reuses or redispatches the claimed attempt.
- A durably completed marker followed by matching successful terminal is a complete remote success
  even if the empty request acknowledgement was not recorded before process loss. Startup resumes
  bounded provider-operation item finalization, consumes the compaction record as success, keeps
  the exact valid binding, and releases the gate. A recorded matching interrupted terminal may
  retain that binding only with separately durable exact idle-status evidence; terminal alone does
  not prove that a prior system-error state cleared. Failed terminal, interrupted terminal without
  idle proof, or successful terminal without the marker consumes failure and retires the uncertain
  binding.
- A compacting operation already transitioned to stopping is recovered by the exact paired stop-
  operation contract. Startup sends neither compact-start nor interrupt, consumes both live
  authorities through one projection-loss successor, preserves all accepted input, and converges
  the provider-operation turn incomplete unless matching terminal evidence had already won the
  serialized cut.
- The process-local lifecycle continuation never participates in compaction restart. Recovery may
  settle successful compaction and wake accepted input, but it cannot synthesize the fixed
  continuation turn. If the live settlement already atomically consumed the compaction record and
  admitted the derived continuation turn before process loss, its `PendingTurn`, canonical item,
  and sealed content are ordinary durable authority; recovered-pending scheduling executes that
  existing identity without consulting or recreating lifecycle intent. Repeated restart before
  consumption, during bounded item finalization, after user-work settlement, or after queue wake
  converges without replaying a provider request or synthesizing a continuation. Restart after
  continuation settlement converges through ordinary pending/active-turn recovery and never
  creates a second turn.
- Startup classifies a `Stopping` gate only together with its exact current stop-operation record.
  `Admitted` and `DispatchClaimed` both lack a recoverable foreground connection generation after
  process restart, so neither state authorizes an interrupt replay. Under the same target-operation
  election used during live control, recovery atomically abandons the ordinary active projection
  or provider-operation valid binding through the stop target, preserves all effective next-turn
  input, consumes the stop and any paired provider-operation live authority into its startup-
  abandonment receipt, and enters source-less incomplete convergence for the blocked operation. A
  claimed attempt remains possible-dispatch provenance; an admitted stop remains proven unissued
  by Beryl, but neither distinction permits reactivation of the old CAS target.
- A stop record without its matching `Stopping` gate, a `Stopping` gate without its record, or a
  record whose exact target no longer matches the named operation is coherent corruption. Startup
  never repairs the pair by deleting one side, synthesizing an attempt, or sending a backend
  request.
- Only after the complete bounded startup scan has converged every active or post-abandonment
  thread does the recovery owner open both scheduler lanes and emit one typed recovery wake. That
  wake opens one retry-eligible steering pass, one recovered-pending scan, and the ordinary
  accepted-next lane. Provider, worker, or flight readiness cannot bypass the startup fence.
- Exact CAS resume may restore live control only when runtime, root, CAS thread id, Syndic thread
  id, reverse uniqueness, selected-path proof, tool profile, and lineage mode all permit it. Losing
  a recovered loaded session revokes its execution capability. Only an already-established
  same-process quarantine anchor may hand it to a fresh connection without rebuilding the prefix.
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
  cannot dispatch duplicate same-thread start, fork, or injection work. Scheduled next-turn work
  acquires that same flight after global source discovery and retains it across candidate
  validation, promotion reconciliation, projection establishment, and ordinary dispatch rather
  than reacquiring it inside each step. Releasing the flight physically removes its process-local
  entry and coalesces one accepted-input scheduler wake.
- Different threads share process-wide runtime and account projections without sharing active-turn ownership or stop targets.
- Resident live-event pages, delivery scheduler pages, active steering tasks, retry pages, proof
  records, diagnostic payloads, recovery cursor pages, and branch-context pages have deterministic
  count and byte bounds. Durable accepted-input and retry domains remain logical paged collections.
- Active-slot exhaustion applies backpressure or leaves already admitted durable work scheduled; it
  does not reject a user input merely because a process queue would otherwise grow.
- Quiet live streams remain active until a terminal event, transport failure, protocol error, or managed-process exit is observed.

## Security And Policy

- CAS remains responsible for authentication, ChatGPT workspace selection, managed configuration, enterprise policy, sandboxing, approvals, skills, MCP, tools, subagents, and rate limits.
- Recovery injection and branch-selection context never contain authentication material, loopback capability tokens, hidden developer instructions, policy-private fields, or raw approval payloads.
- Stored protocol errors and diagnostic evidence are bounded and redact secret-like values before durable commit.
- Syndic capture and recovery projection do not broaden CAS permissions or bypass policy decisions.
