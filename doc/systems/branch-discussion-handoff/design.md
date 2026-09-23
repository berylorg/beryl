# Goals

Define durable branch-discussion context, resolution-tool admission, parent-handoff ordering,
recovery, and archive coordination across Syndic, Beryl-home durable jobs, and CAS execution.

Guarantee that queued user input is never discarded, one live accepted resolution attempt gates later discussion input, retries cannot duplicate a parent turn, terminal failure restores unarchived discussion editing without erasing failed evidence, and archive occurs only after successful handoff.

## Non-goals

- Defining synthetic discussion-context presentation, discussion-status layout, or user-facing copy.
- Allowing the model to choose authoritative parent, child, job, thread, or archive identities.
- Automatically retrying a resolution tool call that was deferred because discussion input was queued.
- Automatically creating a replacement parent turn or fresh resolution attempt after terminal handoff failure.
- Redirecting a handoff when its parent thread is missing or unavailable.
- Garbage-collecting archived discussion history or failed handoff turns.

# Decisions

## Durable Discussion Binding

- A branch-discussion Syndic thread stores exact parent Syndic thread id and exact context-owner draft or submitted-turn id.
- Its first draft stores an immutable context envelope containing version, exact selected UTF-8 text, source thread id, source turn id, source item/projection id, source revision, normalized selected range, context digest, and creation time.
- Context-envelope V1 computes its context digest as SHA-256 over the exact selected UTF-8 bytes without trimming, normalization, or framing; source provenance and range remain separately versioned envelope fields.
- Branch creation accepts only a current assistant projection owned by a proven-terminal source turn.
  It validates the exact source thread, turn, item, finalized projection identity and revision,
  selected range, and selected bytes before creating the Syndic thread, inherited immutable
  execution, attributes and usage seeds, first draft, parent binding, and context-owner link in one
  `SyncAll` home-store command.
- That command creates only the durable, discoverable discussion and performs no window claim,
  session selection, or model request. After committed or reconciled `ExactNew` creation, the
  feature-owned workflow selects it through ordinary thread activation. Activation failure or retry
  never repeats discussion creation.
- A projection admitted as branch context is finalized durable history. Its identity, revision, text, resource references, and ordering cannot later be invalidated, rebuilt, or rewritten in place.
- The source turn must lie on the source thread's selected path at branch admission. That is a creation precondition, not a permanent claim about the mutable parent thread: later replacement may move the parent thread tail without invalidating the discussion, its immutable historical source, or its handoff destination.
- Context reconstruction reads the exact envelope by context-owner identity. It never searches for similar transcript text.
- The first idle submission transitions that draft identity into the first submitted discussion turn,
  derives the turn's immutable parent from the exact envelope source, and retains the envelope
  unchanged. The draft record itself has no generic parent field.

## Exact Durable Mutation Outcomes

- The generic command outcome, custody handoff, and per-home registry lifecycle are owned by
  `doc/systems/beryl-home-storage/design.md`. Each branch creation, resolution admission,
  parent-input/job transition, and success/archive command contributes its own exact operation-
  specific old and intended-new natural-record scope. A discussion-wide, thread-wide, or home-wide
  reread is never a substitute.
- Any generic `Indeterminate` result transfers its unique opaque descriptor and reserved capacity
  synchronously into the per-home reconciliation registry before a tool reply, acknowledgement,
  scheduler release, cancellation observation, broker disposal, or service disposal can erase the
  immediate outcome. No branch-owned caller or worker retains a second descriptor.
- While that exact scope is unresolved, the affected operation publishes no success, dispatches no
  dependent CAS work, and performs no mutation or delivery retry. Structurally healthy unrelated
  scopes and threads remain available.
- `ExactOld` proves that the intended mutation did not become authoritative and permits only the
  owning operation's ordinary post-reconciliation noncommit behavior. `ExactNew` proves the
  complete intended state, reconstructs the exact normal committed receipt, and alone permits
  ordinary publication or dependent work. `Collision` authorizes neither publication nor retry and
  leaves only that exact operation scope closed without guessing, merging, replaying, or widening
  recovery.

## Scoped Resolution Tool

- The branch-resolution tool definition is part of the canonical versioned Beryl conversation-tool
  registry installed at every persistent conversation lineage's initial CAS `thread/start`.
  Registration remains identical through native continuation, resume, and fork for stable provider
  prompt prefixes; it is capability discovery, not mutation authority.
- The tool accepts one nonempty resolution text payload of at most 65,536 Unicode scalar values and
  no parent, child, thread, archive, or job identity arguments. Incremental admission bounds the
  decoded UTF-8 representation before allocation or durable mutation.
- Beryl derives discussion thread, resolving Syndic turn, exact CAS thread/turn, parent thread, context owner, and active binding from the correlated tool request.
- Resolution text is model-produced content, is bounded before durable admission, and is retained exactly after admission.
- A tool request with missing correlation, an ordinary non-discussion thread, wrong thread, stale
  active turn, archived discussion, or unavailable parent is rejected without mutation. No tool
  argument can widen this handler-side scope.

## Atomic Admission Against Queued Input

- Resolution admission and accepted-input queue admission for one discussion use the same serialized thread-operation gate and expected revisions.
- Admission checks for future-turn accepted input after applying every earlier ordered input mutation in the writer.
- If future-turn input exists, resolution returns `deferred_queued_input` with bounded structured guidance, creates no job, changes no archive or composer state, and leaves the discussion fully editable.
- Active-turn steering already delivered or targeting the resolving turn is not future-turn queued input and does not by itself defer resolution.
- Beryl never schedules an automatic retry of `deferred_queued_input`; only a later model tool call can make a new admission attempt.
- Successful admission atomically creates one resolution-intent record and one handoff job for a fresh attempt, marks the discussion resolution-pending, advances its revision, and installs the composer gate before returning success.

## Resolution Identity And Idempotency

- A discussion owns an ordered durable history of resolution attempts. Each admitted attempt owns one immutable resolution-intent record and one handoff job.
- At most one attempt is live. Jobs in `waiting_resolving_turn`, `waiting_parent`, `starting_parent`, `parent_active`, or `retryable_failed` are live; `terminal_failed` and `succeeded` are terminal.
- A fresh attempt may be admitted only when the discussion is unarchived and either has no prior attempt or its latest attempt is `terminal_failed`. A queued-input deferral creates no attempt.
- The intent id is a new Beryl-owned stable id stored with the exact correlated tool-call identity and resolving turn id.
- The handoff job id is derived from that attempt's admitted intent id and remains stable across retries and restart.
- Repeated delivery of the same tool request returns that request's existing admission result, including after its attempt becomes terminal.
- A different tool request while an attempt is live returns `already_admitted` and cannot replace its payload or parent.
- A different tool request after `terminal_failed` may create a new intent and job with a new payload and identities. A succeeded attempt has archived the discussion and rejects later admission.
- Each job id is the idempotency identity for only that attempt's parent accepted-input admission and CAS delivery correlation.

## Handoff Job Lifecycle

- Job states are `waiting_resolving_turn`, `waiting_parent`, `starting_parent`, `parent_active`, `retryable_failed`, `terminal_failed`, and `succeeded`.
- Runtime unavailable, root unavailable, CAS unavailable, and delivery failure proven before dispatch may enter `retryable_failed` from any of the four non-failure checkpoints. Exact CAS rejection before acceptance may do so only from `starting_parent`. A possibly dispatched parent `turn/start` whose response is lost is not retryable delivery failure.
- Invariant violation and missing parent may enter `terminal_failed` from any non-failure checkpoint. Unrecoverable post-append may do so only from `starting_parent` or `parent_active`. Parent interruption, incomplete termination, and terminal failure may do so only from `parent_active`.
- Child input remaining after steering settlement and resolving-turn convergence enters
  `terminal_failed` from `waiting_resolving_turn`, before any parent input exists. It is a normal
  terminal disposition, not an invariant violation. Failure and discussion gate release publish
  atomically; queued child input remains durable and becomes eligible for ordinary processing.
- The exact failure disposition and checkpoint matrix is one durable schema invariant shared by transition admission and record decoding. Ordinary registration, verification, and recovery reject any persisted pair outside it rather than interpreting the evidence at a different stage.
- The handoff composer gate exists exactly while the latest attempt is live. A retryable failure retains that gate and its immutable job; a transition to `terminal_failed` removes the gate in the same durable state change.
- After tool admission, the job waits until the resolving child CAS turn is no longer active and its tool call plus resolution payload are durable. Terminal success, interruption, or explicit incomplete termination may satisfy this `waiting_resolving_turn` condition because accepted intent is already immutable; it does not remove the handoff composer gate.
- Advancement to `waiting_parent` also requires settled already-admitted steering, completed
  resolving-turn terminal convergence and zero remaining child accepted input. Delivering or
  unresolved steering keeps the job waiting. Steering rejected or reclassified to future child
  input terminally fails the attempt as described above. The writer validates these exact child
  facts with the State transition; no parent input may be prepared or admitted before that
  transition. Pending discussion gates prevent new acceptance and successors thereafter, so the
  proven empty child queue stays empty through parent execution. Recovery applies the same check.
- `waiting_parent` preserves existing parent accepted-input order. The handoff follows every
  parent input through the job's recorded accepted-input frontier, inclusive, and preserves later
  input already accepted before handoff admission through the normal revisioned admission path.
- Parent active turn, compaction, replacement, rebind, or another same-thread operation keeps the job waiting.
- A parent's pending discussion handoff also keeps delivery waiting. An exact archived parent
  rejects fresh child resolution admission; if the child attempt was already admitted and no
  parent input exists, `ParentArchived` terminally fails it from `waiting_resolving_turn` or
  `waiting_parent` and releases the child's gate atomically. The writer validates the exact parent
  archive observation inside the same Syndic release participant. This normal outcome neither
  invokes invariant failure nor makes archive depend on descendant jobs. After parent input has
  been admitted, the existing parent-turn outcome remains authoritative for the child job.
- Runtime/root/CAS unavailability or delivery failure proven before dispatch moves the job to `retryable_failed` without changing the discussion archive state or admitting another attempt.
- Retrying `retryable_failed` resumes only that exact job and any exact parent input already admitted for it. It cannot change the intent payload, allocate another attempt, or append a duplicate parent turn.
- Invariant failure, missing parent, or an unrecoverable post-append state moves the job to `terminal_failed`, leaves the discussion unarchived, and releases the discussion for later ordinary mutation.

## Parent Input And Turn

- Parent handoff is a real Syndic submitted turn admitted through the parent's normal revisioned submission path.
- The input item is typed as Beryl-generated discussion handoff rather than user-authored composer input and records discussion thread id, intent id, job id, context digest, and resolution provenance.
- Its visible text identifies that it is a discussion resolution and includes the exact admitted resolution payload.
- The canonical visible text is the UTF-8 prefix `Discussion resolution:` followed by two LF
  characters and the exact admitted resolution bytes. No trimming, rewriting, hidden suffix,
  selected-context duplication or image-marker interpretation changes that payload. The fixed
  24-byte prefix gives a maximum of 262,168 visible UTF-8 bytes. Provenance retains the child,
  intent/job, context owner/digest and resolving turn plus SHA-256 of the exact resolution bytes;
  the immutable State job supplies the original correlated model-call identity.
- A generated accepted-input identity uses the intent/job's exact 16-byte payload under the
  `SyndicAcceptedInputId` type. The new submitted turn and canonical item use distinct caller-owned
  stable identities retained through preparation and reconciliation. Identity collision rejects
  the command; it never generates replacement ids during replay. A generated receipt names that
  exact turn and item and has no source/replacement draft or steering route. The input occupies
  the next permanent accepted ordinal and is already assigned to its turn in the admitting command;
  it never appears as pending steering or next-turn work.
- Parent admission requires an exact idle, editable parent with no live accepted input, no active
  binding or unresolved same-thread lifecycle operation, and an ordinary draft submission intent.
  Initial discussion-context and replacement drafts remain waiting; generated admission cannot
  perform their special first-submission or replacement transition.
  It advances normal turn topology, pending dispatch and bounded transcript/history heads while
  preserving the current draft identity, revision, text/markers, editor session and submission
  intent. Only the current-draft reverse index's enclosing thread-revision proof advances.
  Competing ordinary input and other handoffs contend at the same revisioned writer boundary.
- CAS receives the same visible handoff text through ordinary user input because it is the new model-visible parent turn; hidden context is not used to conceal a visible handoff.
- Parent admission and the job transition to `starting_parent` occur atomically. Recovery finding the parent turn identity never creates another input for that job.
- CAS acceptance records exact CAS turn identity and moves the job to `parent_active`.
- Generated handoff turns and canonical inputs have distinct closed provenance kinds. Ordinary
  pending dispatch, execution snapshots, input replay, provider-user-message correlation, exact
  stop, terminal convergence and repair classification recognize that kind as the same ordinary
  model-visible execution path. The generated canonical input is retained exactly once when CAS
  echoes it; neither correlation nor history repair changes its authorship to composer input.
- CAS rejection before acceptance moves the job to `retryable_failed` and leaves the existing admitted parent turn pending for retry; it does not append another turn.
- If parent `turn/start` may have been dispatched but its response is unavailable, the parent turn is
  never replayed automatically. Proven loss of its execution session converges that parent turn to
  incomplete, moves the job to `terminal_failed`, leaves the discussion unarchived, and releases
  its composer gate according to the feature contract.

## Success, Archive, And Failure

- Parent terminal success atomically moves the Beryl job to `succeeded` and publishes the Syndic
  discussion thread's one-way archived attribute in the same home command.
- Archive publication occurs only after that commit's `SyncAll` barrier.
- Parent interruption, incomplete termination, or terminal failure atomically moves the job to `terminal_failed`, leaves the discussion unarchived, releases the handoff composer gate, and retains the exact parent turn identity.
- Every terminally failed intent, job, accepted parent input, and already-appended failed parent turn remains durable. Failure never rolls those records back or treats them as successful archive.
- Beryl never automatically creates a second parent turn or a fresh attempt after terminal failure.
- `Retry handoff` is valid only while the same job is `retryable_failed`. It resumes that job and any exact admitted parent turn; it is invalid after `terminal_failed`.
- Later ordinary discussion turns may proceed after terminal failure. A later correlated tool request may admit a new attempt and, through its distinct job id, one distinct parent handoff turn; this is not recovery or retry of the failed attempt.
- Ordinary discussion mutation after terminal failure does not clear or reclassify the failed attempt. It remains the latest status source until a fresh attempt is admitted.
- Only a job reaching `succeeded` archives the discussion. Neither retryable nor terminal failure changes archive metadata to archived.

## Parent Identity And Window Ownership

- Beryl exposes no parent-thread deletion command. Resolution admission and recovery still require the exact durable parent record and reject missing or invalid parent identity without inferring a replacement.
- A parent main-window claim does not block job execution. The owning window receives ordinary active-turn and transcript updates for the parent.
- If no window owns the parent, the job may execute in background through the same exclusive CAS projection and durable thread gate; it does not create a hidden replacement window.

## Restart Recovery

- One process-owned coordinator belongs to the exact home and app service generation. It composes
  typed Syndic and Beryl-state participants through the home writer; windows and tool response
  owners neither own jobs nor retain their reconciliation descriptors. Disposal fences admission,
  cancels scanning and joins admitted workers before releasing the service. Replacement reconstructs
  it from durable facts using fresh handles.
- Initial-open and same-home recovery candidates use the same sequential durable convergence
  algorithm through explicit candidate access, after CAS-live convergence has settled available
  dispatch and terminal evidence. Candidate convergence performs no CAS request and admits no
  ordinary ready work. Waiting jobs may remain durable at publication; a backlog larger than the
  ready queue cannot prevent startup merely because ordinary workers are still fenced.
- After complete graph publication, a fresh bounded live-index scan admits ordinary work. Candidate
  convergence does not transfer a retained job list, cursor, response owner or execution capability
  into that scan. The existing complete-graph publication fence controls worker release.

- Startup validates exact positive `handoff_recovery_page_items`,
  `handoff_recovery_page_encoded_bytes`, `handoff_job_record_encoded_bytes`,
  `handoff_reconcile_slots`, and `handoff_ready_job_items` configuration before scanning. It
  enumerates only the live-job index in key order through cursor pages that independently obey
  both configured page caps. The item count is the number of returned job records; encoded-byte
  accounting is the checked sum of every returned encoded key and value length, excluding only the
  cursor container's fixed bookkeeping. Job mutation rejects a record beyond
  `handoff_job_record_encoded_bytes`, so recovery never returns one oversized record as an
  exception. Historical `terminal_failed` and `succeeded` attempts remain queryable for status and
  idempotency but are never scheduled as live work.
- The scanner retains at most one cursor page, its continuation key and store revision, one current
  job identity/checkpoint, and aggregate progress counters. It never builds a live-job collection
  or parent-to-job map. Ordinary scheduling admission pauses when either the ready-job queue reaches
  `handoff_ready_job_items` or all `handoff_reconcile_slots` are occupied. Candidate convergence
  processes one job at a time without populating that queue.
- Each decoded job record is released after reconciliation or bounded scheduler admission, and the
  page plus its decoded-byte accounting is released before the next cursor request. Cancellation,
  store invalidation, and startup failure release the current page, queued task ownership, and
  reconciliation slot; restart resumes from durable job state rather than retained scan memory.
- Recovery reconciles each live job with discussion revision, composer gate, parent existence, parent accepted-input identity, parent turn identity, CAS binding, and exact active-turn records.
- Recovery requires the handoff composer gate for every live latest attempt and no handoff composer gate after `terminal_failed`; archived readonly behavior derives separately from the succeeded attempt's archive metadata.
- Recovery advances an already completed durable step instead of repeating it.
- Unknown CAS terminal state remains unresolved until exact evidence or the CAS-live recovery contract classifies the turn incomplete; it never causes duplicate delivery.
- Job workers, retry tasks, and per-parent schedulers obey the configured reconciliation-slot and
  ready-job capacities, are keyed by exact job id, release their slot and queue ownership on every
  terminal, cancellation, or supersession path, and wake only for relevant durable, explicit-retry,
  or runtime state changes.

## Coordination Participants

- The ordinary dispatcher supplies a branch-specific context containing its exact home/service
  generation and Syndic thread/turn plus the broker-validated CAS thread, turn and tool-call
  identity. It derives this context before consuming the routed call and preserves the single
  response owner. The model payload supplies no correlation identity. Admission revalidates the
  current durable binding and resolving turn; copied correlation alone grants no mutation right.
- Beryl-state owns the immutable admitted intent within the job, exact request index, ordered
  attempts and revisioned job transitions. Syndic owns the discussion's revisioned handoff gate
  and the generated parent input's immutable provenance. Admission composes both typed mutations
  in one command; a job record alone cannot close discussion input. Every ordinary discussion
  mutation that the feature forbids while resolution is pending checks that same durable gate.
- The handoff gate is independent from the execution input gate and thread title/archive revision.
  It starts open with discussion creation, names the exact intent/job/resolving turn while pending,
  and advances its own revision on admission or terminal release. A missing gate on a discussion is
  invalid state, never an implicit open gate. Retrying the same live attempt preserves its gate;
  terminal failure clears it, while success clears it with one-way archive publication. Ordinary
  threads have no handoff gate. Storage validates its local thread/turn closure; the app validates
  its agreement with the State job in the same command or exact recovery operation.
- Resolution admission authenticates the discussion's immutable parent/context binding, selected
  resolving turn, current active CAS binding/turn, open archive state, exact gate revisions and
  zero future-turn accepted inputs at the serialized writer. The parent observation fences its
  exact identity and accepted-input frontier; no parent execution capability is retained. An
  intervening queue admission or changed binding rejects the proposal without creating either
  participant. Existing request idempotency is checked before proposing a fresh attempt.
- Each command has exactly one Syndic participant. A separately typed parent-frontier proof is
  validated inside admission's Syndic mutation. Terminal failure uses one gate-release mutation;
  success uses one combined gate-release/archive mutation. The app composes that participant with
  the State job mutation. These operations preserve one writer snapshot and atomic publication;
  they do not require repeated same-domain participants or sequential commands.
- Admission gates new composer acceptance, steering acceptance, replacement/path mutation and
  automatic lifecycle successors. It does not prevent source capture, delivery of already admitted
  steering, exact stop, resolving-turn terminal convergence, or disposal. Draft preparation may
  finish unpublished, but cannot publish a forbidden edit after the gate changes. Archived
  discussion mutation remains independently closed by its archive attribute. No generic gate check
  may strand the exact resolving turn's terminal history or prevent failure cleanup.
- Before leaving `waiting_resolving_turn`, the app joins the exact current job and Syndic child
  settlement preparation under one home command and the coordinator's existing generation and
  custody fence. `Ready` contributes one Syndic validator plus State resolving completion;
  `QueuedInput` contributes one Syndic release mutation plus State terminal failure
  `ChildInputPending`. `Waiting` contributes nothing and does not reserve parent input. The app
  checks the job's discussion, intent and resolving-turn identities against the pending gate.
  Candidate convergence uses the same decision with explicit candidate access and no CAS work.
- These commands use ordinary exact outcome custody: advancing the job without a Syndic write
  reconciles the State transition closure; failing it with gate release reconciles both mutation
  closures. Uncertain outcome suppresses advancement and parent scheduling until resolved. No
  cached child observation substitutes for writer validation or mutation outcome evidence.
- The stored parent queue ordinal records the parent's accepted-input high-water mark observed
  under the resolution-admission command's exact parent gate revision. It is an ordering frontier,
  not an accepted-input reservation. Parent handoff waits for an idle eligible parent with no live
  earlier accepted input, and normal admission also preserves any later input already accepted
  before handoff admission. There is no priority bypass, speculative parent turn or separate
  volatile parent queue. Multiple jobs for one parent contend through ordinary revisioned writer
  admission; live-index key order does not become accepted-input order.
- Parent handoff preparation preserves the parent's editable draft. It constructs bounded sealed
  visible content through the normal content boundary and contributes a generated accepted input,
  its submitted turn and exact typed provenance together with `starting_parent` in one command.
  A failed preparation grants no parent input identity. A committed input is permanent even if
  subsequent dispatch fails. Exact operation custody covers all participants and releases unused
  unpublished preparation resources without rolling back committed input.
- Parent execution uses the ordinary exclusive projection, dispatch provenance and turn lifecycle.
  The handoff coordinator observes exact durable outcomes and never implements an alternate CAS
  sender. Unknown dispatch remains fenced until the CAS-live owner settles it. Job transition and
  discussion gate release or archive compose atomically; unresolved command custody suppresses
  both tool success and dependent scheduling.
- The runtime queue retains only bounded exact job identities and observed revisions. A worker
  rereads current typed state before admission, releases its payload and slot when the job must
  wait, and relies on relevant durable/runtime wakes. Bounded duplicate suppression covers only
  queued and running identities. A coalesced rescan request, not one retained wake per durable job,
  handles changes while scanning. Capacity release resumes a paused scan; waiting state alone does
  not create a polling or automatic retry loop.
- A processed live-index key is the continuation after the coordinator's own mutation. Before
  reusing an unprocessed page, the scanner checks its revision and drops a stale page. Relevant
  changes behind the cursor request another bounded pass; no scan accumulates a home-wide job set.
- Typed point and live-index reads support both ordinary home access and explicit unpublished
  candidate access with identical validation and byte accounting. Candidate access does not expose
  a healthy home or bypass stale-handle checks. Record limits accommodate the full tool contract:
  at most 65,536 scalar values and 262,144 UTF-8 bytes of exact resolution, plus bounded metadata.
  Configuration must admit a largest valid record within a page; lowering a transport/page tuning
  value cannot silently redefine the product's resolution limit.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers:

- `external-side-effects/v2`
