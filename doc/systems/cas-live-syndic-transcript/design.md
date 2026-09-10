# Goals

Capture normal Codex App Server turns exactly into Syndic durable history and execute new Syndic work through one exclusive CAS projection per Syndic thread.

Keep Syndic as the normal canonical and read authority while permitting one narrow, release-pinned
historical repair when an exact correlated terminal turn has a proven or conservatively suspected
live-capture gap and an exact terminal outcome.

Preserve CAS as the live execution, authentication, sandbox, approval, skill, MCP, subagent, and enterprise-policy authority without retaining failed service internals across Beryl-home recovery.

## Non-goals

- Importing, cataloging, or backfilling CAS history.
- Reading CAS history for ordinary transcript rendering, thread lists, titles, search, restore, replay, or unrelated turns.
- Treating bounded repair as notification replay or reconstructing a synthetic live event sequence.
- Summarizing, truncating, guessing, or splicing canonical history to make an incomplete turn appear complete.
- Replacing CAS execution or policy behavior.
- Mutating submitted Syndic turn parentage to match a CAS operation.
- Retaining or adopting failed connection, service-epoch, broker, projection, or loaded-session internals after durable-store recovery.
- Defining a Beryl-side CAS memory budget or managing CAS process memory.
- Providing hard-stop escalation, experimental process termination, or background-terminal cleanup.

# Decisions

## Documentation Set

- `doc/systems/bounded-resource-dataflow/design.md` owns fixed-page, payload, queue, backpressure, concurrency, and transactional-streaming limits.
- `doc/systems/syndic-conversation-history/design.md` owns canonical history, repair-snapshot
  storage and selection, `FinalizingHistory`, transcript projections, and durable conversation
  state.
- This document owns CAS/Syndic correlation, normal live capture, terminal-repair eligibility,
  the one-request authorization and pinned adapter, exclusive execution projection, delivery,
  interruption, compaction, and recovery behavior.

## Ownership Split

- Syndic owns threads, current drafts, submitted turns, immutable parentage, accepted input, canonical history, transcript projections, resource metadata, and CAS projection-binding records.
- CAS owns live execution and provider policy for CAS-backed turns.
- Beryl coordinates typed durable commands and normalized CAS requests without becoming another durable history owner.
- Ordinary transcript, catalog, title, search, branch-context, and replay reads use Syndic only.
- CAS historical access exists solely inside the repair boundary defined here.
- The process owns execution independently of main-window selection. A window owns its durable
  GUI claim, editor, view subscriptions, and presentation; that claim neither owns nor authorizes
  a CAS session. The process owns exact execution sessions, capture, scheduling, pending request
  routing, and lifecycle continuation for viewed and unviewed threads alike.

## Process-Owned Execution Lifetime

- One healthy-home service generation owns one bounded execution registry keyed by Syndic thread.
  Every reserved entry names the exact healthy home/service generation, Syndic thread, and full
  runtime/root execution binding before projection establishment. It retains managed-process,
  connection, loaded-session, projection-binding, tool-profile, and operation authority as each is
  established and validates every applicable identity before use. A reservation or admitted
  unprojected session does not imply loaded projection or dispatch authority. The entry owns or
  checks out the actual admitted execution lease; copied ids or a shared connection handle cannot
  preserve authority after the owning lease has revoked it.
- A checked-out session remains owned by its exact execution flight through preparation,
  dispatch classification, approval, streaming, stop, compaction, and terminal-history settlement.
  Its process owner retains the flight, worker reservations, required runtime interest, sole driver
  and ordered ingester independently of view subscriptions. A return to the registry transfers
  custody only within that same healthy generation and never publishes simultaneous usable leases.
- Switching threads or closing a nonfinal main window flushes and detaches the window's view.
  It does not revoke execution authority, unsubscribe the execution stream, stop the target,
  suppress accepted-input promotion, or cancel lifecycle continuation. Already accepted work and
  eligible automatic work follow their ordinary gates while no window selects the thread.
- A window may attach to a running unclaimed thread through ordinary GUI claim acquisition and
  bounded current presentation. Attachment does not establish another CAS projection, replay
  work, or wait for the execution lease to become idle. Existing GUI occupancy remains exclusive.
- Registry capacity is reserved before inserting an already-admitted execution session; connection
  admission retains its existing worker-permit gate. The slot bound is
  `ProjectionServiceConfig.worker_capacity / CONNECTION_WORKER_PERMITS` using checked nonzero
  configuration and the existing two-worker connection reservation; the protected steering
  reserve remains intact. Available, checked-out, and retiring entries share this same bound,
  and retain their slot reservation until exact release even if connection workers have ended.
  There is at most one compact entry per admitted thread and no separate waiting list.
  Saturation leaves durable pending
  work intact and waits for the existing typed capacity wake. It does not impose a new durable
  thread or queued-input quota or evict required active authority.
- Durable queues remain revision-bound paged routes. Only eligible admitted work acquires an
  execution session; catalog enumeration, window restoration, or an activity query cannot start
  arbitrary catalog threads or allocate a session per durable thread.
- At terminal-history fixed point the owner may serve eligible same-thread work under the normal
  operation gate. When no admitted work, pending request, continuation, finalization, or other
  required execution interest remains, it releases expensive session and runtime resources through
  exact retirement. Idle catalog membership retains no connection or worker. All entries and
  capabilities are revoked and joined on generation retirement; none survives process restart.
- Required-work admission and idle-retirement election share the home store's mutation-observation
  boundary. The process owner captures a current token before its bounded required-work read and
  consumes it in the final in-memory retirement election, together with exact service, session,
  connection and view authority. A mutation that starts first invalidates retirement's old facts;
  retirement that wins first excludes reuse of its old session, while later admitted work follows
  ordinary preparation and dispatch. No independently sampled revision check closes this boundary.
  Writer settlement requests a coalesced idle recheck after publication, including when an earlier
  inspection encountered an active mutation. Storage reads, provider effects and disposal remain
  outside the final ownership gates; writer observation alone proves neither idle work nor health.

## Request Custody And Running-Thread Projection

- Admitted protocol requests and installed approval-denial/stop obligations belong to the exact
  process execution session, ordered route, and request identity. Existing supported request and
  approval policy applies identically while viewed or unviewed. Detachment cannot change policy,
  duplicate or retarget a response, discard an obligation, or invent user-response authority.
- The process retains bounded normalized request state and its exact non-cloneable capability
  until ordinary ordered settlement or invalidation. Existing per-connection broker, route,
  payload, and request-concurrency limits bound it; no unbounded secondary queue exists.
- Request state is ephemeral and retains existing privacy exclusions. No raw approval payload,
  command body, authentication material, permission body, or secret is added to durable records,
  running-thread pages, notices, or logs. Restart never reconstructs a response capability.
- The app publishes revision-bound bounded pages of running and needs-attention facts from exact
  process execution state, typed durable work/status pages, and the transient attention pool.
  Pages cover preparing and pending work, executing and stopping targets, approval handling,
  compaction, and terminal-history work. Durable facts do not manufacture live authority.
- One process-owned transient attention pool uses the same fixed pending-notice count capacity
  and per-notice byte ceiling as the [Notifications feature](../../features/notifications/design.md).
  This is one shared pool, not a pool per historical thread. Each record contains only compact
  exact thread, yield/outcome identity, and bounded notice facts; completed attention retains no
  execution lease and creates no durable attention or acknowledgement record.
- The pool applies Notifications' existing priority, replacement, omission, and diagnostic rules
  at capacity. View absence alone does not discard an admitted attention record. Its exact record
  clears on notice-close acknowledgement or successful explicit Running threads activation to
  that thread. A stale acknowledgement cannot clear a successor record. Neither action resumes
  work, and no attention or acknowledgement survives process restart by contract.
- Running-thread projection and subscriptions contain compact identity, state, and bounded display
  facts, never transcript bodies, editor state, raw request payloads, or GUI entities. Its resident
  pages and subscribers have the existing configured page and window bounds. Querying or updating
  it creates no hidden window, transcript mount, focus transfer, or extra capture consumer.

## Application Shutdown Coordination

- Final-main-window close and explicit application Exit use one process-owned shutdown
  coordinator. Nonfinal ordinary close is view detachment and never enters this execution barrier.
  Feature authority owns confirmation and visible failures; storage authority owns restore-set
  publication, with an empty set for final ordinary close and preserved layout for explicit Exit.
- Confirmation reads a versioned process-wide work snapshot, including hidden, pending, preparing,
  approval-blocked, stopping, compaction, continuation, and terminal-history work. Merely opening
  or cancelling confirmation mutates no execution, queue, continuation, claim, or durable state.
- Barrier admission linearizes under a shared process gate with main-window acquisition/removal,
  execution admission, and every dispatch or same-thread successor cut. It revalidates final-window
  identity and current work at admission. Positive confirmation authorizes all current work
  captured by that atomic barrier; ordinary progress or newly admitted work before the fence does
  not require repeated confirmation. If an initially idle close skipped confirmation and work
  appears before admission, confirmation is required before effects. An ordinary close that is no
  longer final returns to view-detach semantics. Concurrent close and Exit requests join one
  current attempt and cannot duplicate stop
  dispatch or publish competing shutdown modes; an admitted explicit Exit retains layout mode.
- The admitted barrier fences direct submission, accepted-input promotion, pending dispatch,
  steering, compaction launch, and automatic-continuation admission process-wide. It cancels
  volatile continuation intents at their exact same-thread cut and preserves all already accepted
  input and durably admitted pending turns. A candidate that won before the fence remains part of
  the barrier; a later candidate cannot dispatch. It never drains the queue by starting more turns.
- The coordinator visits the exact registered targets with bounded pages and joins their retained
  execution flights. It requests or joins each target's existing sole exact graceful soft stop when
  authorized, and waits for initially noninterruptible work to reach exact eligibility, terminal
  outcome, or authoritative loss. Approval waits remain exact target work; invisibility or shutdown
  cannot synthesize approval consent. Responses, coarse idle state, and missing stop eligibility
  are not terminal evidence.
- Terminal capture, repair eligibility/convergence, finalization, and admitted reconciliation
  remain permitted behind the dispatch fence. The barrier completes only after each captured
  target's terminal-history fixed point or the existing durable authority-loss convergence and
  all required draft/session durability obligations. Only then may claims be released, windows
  disposed, and the process service graph and managed runtimes retired and joined.
- Any failed or unproven obligation retains coherent windows, claims, resident editors, and exact
  operation/reconciliation custody. Failed shutdown does not undo a stop already sent, resurrect a
  cancelled continuation, repeat uncertain dispatch, or fabricate rollback. The interaction and
  dispatch gates reopen only after a coherent failure outcome; a retry is a new explicit attempt.

## Pinned CAS Contract

- Beryl targets one exact supported CAS release at a time. Runtime admission checks that exact release and rejects an incompatible runtime; it does not run capability probes against user or synthetic threads.
- Required ordinary operations, notifications, item variants, ordering rules, and exact soft interruption are part of the pinned release contract rather than dynamically negotiated capabilities.
- Repair uses the experimental `thread/turns/list` adapter pinned to the same release. The adapter
  exposes only an exact correlated terminal-turn snapshot with hard response, item, content, and
  media bounds.
- A release change requires source-backed review and focused tests for both normal capture and the repair adapter before the supported release changes. Runtime traffic is not used to rediscover those semantics.
- Schema presence, diagnostic text, thread enumeration, or a generic history response never broadens the admitted contract.

## Exclusive CAS Projection

- One executing Syndic thread has at most one current CAS projection binding, and one CAS thread id is bound to at most one Syndic thread.
- Each usable binding records the exact runtime/root execution binding, CAS thread id, represented Syndic prefix, lineage mode, pinned tool-profile identity, and current process/loaded-session generation.
- A binding is valid only for the exact selected Syndic prefix it represents. Missing or mismatched proof makes it stale or unbound; it never causes parentage or history to be guessed.
- Thread activation and history browsing never resume, enumerate, or read CAS.
- Different Syndic threads may execute concurrently through distinct exclusive CAS projections. Same-thread execution, steering, stop, compaction, fork, rollback, and repair are serialized.

## Submission And Turn-Start Admission

- Before durable admission of an ordinary turn that can require `turn/start`, Beryl queries the
  Beryl-home free-space reserve through the storage boundary with the one service-configured turn-
  start requirement. Direct submission and queued accepted-input promotion use that same
  requirement; callers cannot substitute a path-specific or pre-aggregated byte total.
- The requirement is the checked sum of the immutable 256-MiB
  `DURABLE_START_ADMISSION_BUDGET_BYTES` product policy and a separately configured nonzero minimum
  turn-capture reserve. Service configuration is invalid before service publication or any query
  when that sum overflows or the capture reserve is zero.
- The fixed budget conservatively covers the greater owner-derived bounded append envelope for the
  direct and queued durable-start commands. The Beryl-home storage system owns the derivation and
  its cross-package responsibility split; it is not a claim that the physical filesystem will
  consume exactly or at most that many bytes.
- A low-space, unavailable, or indeterminate result rejects admission before consuming the draft;
  the input and image markers remain intact.
- The reserve check is an admission fence, not a promise that later provider output will fit. A later store failure enters the bounded outage path.
- Durable admission precedes composer clearing, transcript-visible publication, accepted-input scheduling, or any CAS request.
- Admission atomically freezes exact text and image atoms, creates the submitted Syndic turn and canonical user-input item, advances the selected tail, creates the replacement draft, and records the delivery identity.
- Input admitted while an exact turn is active becomes ordered accepted input for steering when steering remains exact; otherwise it becomes ordered next-turn work. It is never represented in a second CAS-owned queue.
- Validation, reserve, binding, image-path preparation, or storage failure leaves the current draft unchanged.

## Dispatch And Delivery

- A pending turn dispatches only through ordinary `turn/start` after exact projection authority for its parent prefix is proven.
- A matching CAS response and live identity bind the pending Syndic turn to one exact CAS thread and turn. Missing identities remain absent.
- A request proven not to have dispatched may be retried under the same durable turn identity. Once any byte may have crossed the transport, Beryl never repeats `turn/start` automatically.
- Steering targets one exact active CAS thread and turn and retains the accepted-input identity and order. Possible dispatch without an authoritative result becomes delivery-unknown and is never replayed automatically.
- Provider rejection, transport loss, and target loss never fabricate delivery success or discard admitted input.

## Normal Live Capture

- Every admitted execution projection owns one ordered consumer for the complete pinned notification
  stream regardless of whether a window selects its thread. The protocol's foreground connection
  profile denotes full live capture capability, not GUI visibility. Responses, server requests,
  item observations, and terminal controls cross one ordering boundary.
- Normal capture retains every Beryl-relevant public field of every admitted pinned item variant, including exact operational content. Operational records remain outside parent transcript narrative but are not normally sampled, summarized, or discarded.
- Provider observations are normalized into exact typed item start, delta, completion, and terminal facts with CAS thread, turn, and item identity.
- Arbitrarily large admitted values cross bounded durable staging and content pages. Page, transport, or coalescing boundaries do not become semantic event boundaries.
- Assistant commentary, final answers, plans, user-message correlation, generated-media metadata, and supported operational records preserve exact field identity, order, status, and provenance.
- Raw reasoning text and standalone image-generation base64 are deliberate ingress exclusions. Their values are structurally consumed and discarded before any retained normalized value, diagnostic payload, or log record is created.
- Each admitted source fact and its canonical effects publish atomically. Exact replay is recognized by stable identity; different data at an occupied identity is a collision, and gaps are capture failures.
- `ProviderObservationStager` sends its exact home-store outcome through the ordered `beryl-app`
  `Ingester`. Any `Indeterminate` custody value reaches the per-home reconciliation registry before
  `BrokerReply`, `AckSlot`, or `ActiveObservation` can be completed, released, cancelled, or
  retired, as required by `doc/systems/beryl-home-storage/design.md`. The acknowledgement carries
  neither a receipt nor a descriptor, publishes no source fact, and authorizes no retry, rollback,
  publication, or reconciliation execution.
- For consuming seal, Syndic returns one seal-specific custody guard that retains both the sole
  home-store custody and the inert consumed stager. The `Ingester` immediately consumes the guard;
  its terminal installation puts home custody in the registry before releasing the stager. The
  guard exposes no stager or successor and is never stored in a broker reply, acknowledgement slot,
  active observation, connection, or service. Ordinary destruction cannot discard the result: the
  guard's non-discardable home custody performs fallback installation before its stager is dropped.
- After registry handoff, the old process-local observation stager and operation holder are
  disposable and never cross connection or service retirement. A later `ExactNew` successor is
  reconstructed only from durable natural records; `ExactOld` exposes a continuation only to the
  same still-live owner when the direct operation contract permits it; and `Collision` exposes no
  continuation or publication authority. Recovery never depends on an old stager.
- A normal terminal event audits only observations already admitted in order. It cannot prove or invent a missing item.
- Completion/live mismatch, interruption or stream loss, and any other proven or conservatively
  suspected missing canonical fact enter repair-required once exact CAS/Syndic turn correlation and
  terminal outcome make the bounded repair eligible. Before then, unresolved terminal state remains
  explicit and does not authorize repair or history completion.
- Transcript-visible live fragments may be shown process-locally before durable takeover. They have no canonical or repair authority and are relinquished only after exact Syndic prefix agreement.

## Provider Narrative And Item Authority

- Provider item lifecycle and transcript narrative are distinct. Exact provider completion remains durable even when a narrative or resource cannot yet be finalized.
- For live `AgentMessage` and `Plan` items, the ordered live append is the narrative source and completion is an equality fence. A disagreement is a capture gap for the whole turn, not permission to choose one text opportunistically.
- A repair snapshot does not replay item-start or delta lifecycles. Its terminal item views are semantic final-item authority with explicit historical-repair provenance.
- Normal operational items remain exact canonical history. Repair may publish their complete terminal item views only when the snapshot contains them within the pinned bounds; it never reduces normal capture to narrative-only storage.
- Unknown variants, malformed required fields, impossible lifecycle, identity disagreement, missing required content, or unrepresentable media make the affected turn repair-required or incomplete. No event or item is ignored while the turn is published complete.

## Durable-Store Outage Buffer

- When a Beryl-home write fails after a CAS turn may be active, Beryl immediately fences new durable admissions and same-thread successor operations.
- One process-local outage buffer may retain only Beryl-relevant normalized facts for already active exact targets. Its item count, encoded bytes, per-field bytes, and target count have hard local limits independent of CAS memory.
- Retention priority is exact identity and correlation, terminal outcome, assistant final output, transcript-visible narrative, user-message correlation, generated-media handoff metadata, then operational content. Operational content may be evicted first when the hard limit is reached.
- Priority affects only outage survival. Normal capture before the outage remains exact, and a lower-priority fact that fits is retained exactly.
- Any evicted, rejected, partially received, structurally unrepresentable, or otherwise dropped
  canonical fact marks the entire owning turn as a repair candidate and moves it to repair-required
  once exact correlation and terminal outcome are known. A retained suffix or terminal fact never
  makes a gapped turn canonical.
- Buffered narrative may remain visible as explicitly transient UI during the outage. It is never committed as a canonical prefix to be spliced with historical repair.
- On store recovery Beryl closes the failed service and every connection, broker, projection,
  loaded session, and process-local authority derived from it, then follows the ordered
  fresh-service recovery and post-publication projection reacquisition defined below.
- No stable-core adoption, service-epoch transfer, retained connection, lease handoff, or old-generation projection promotion exists.

## Exact Terminal-Turn Historical Repair

- A repair candidate is one exact Syndic turn whose CAS thread and turn identities were durably correlated before or can be authenticated by the sealed outage facts, whose terminal outcome is exact, and whose capture gap is known or conservatively suspected.
- Repair requires the caller to hold the same-thread no-successor fence. Under that fence the exact
  correlated target must be the latest CAS turn; without both facts, no historical request is
  authorized.
- Each repair-required turn has one durable target-scoped request disposition, initially
  `Available`. Before any backend call, one atomic Syndic mutation changes it to
  `Consumed(request-attempt nonce, source revision, successor revision)` and returns the sole
  non-cloneable dispatch capability. `NotCommitted` returns no capability; `Indeterminate` returns
  no capability until targeted reconciliation proves `ExactNew` to the same still-current
  coordinator. If that coordinator no longer exists, no capability is reconstructed and recovery
  uses only a complete durable staged candidate or converges incomplete. Only `ExactOld` may
  authorize the same claim command again. A consumed disposition is never reset by cancellation,
  response loss, process loss, recovery, or incomplete convergence.
- Consuming that non-cloneable capability sends exactly one experimental `thread/turns/list` request using the generated
  `ThreadTurnsListParams`: the authenticated `threadId`, no request cursor,
  `limit=1`, `sortDirection=desc`, and `itemsView=full`. It accepts only the generated
  `ThreadTurnsListResponse`. A bounded continuation cursor for older turns is structurally consumed
  and discarded; Beryl never follows it, requests an adjacent turn, falls back to whole-thread
  `thread/read`, fills unrelated history, supports catalog or transcript browsing, or runs as
  background backfill.
- The adapter accepts exactly one matching terminal target regardless of bounded cursor-field
  presence. Another returned turn, nonterminal status, incomplete full-item semantics, or content
  outside fixed response, item, content, and media limits makes the repair incomplete.
- Exact 0.146.0 generated-schema and pinned-release source evidence must prove that this one-request
  route yields the required latest correlated terminal turn and complete semantic item view. If it
  cannot, the repair adapter remains unavailable; no history fallback is substituted.
- The result must be one complete terminal snapshot containing the full ordered public item view, exact thread/turn/item identities, terminal outcome, and every required narrative, operational, and resource field admitted by the pinned contract.
- The complete adapter result is staged behind a compact snapshot head and remains noncanonical.
  Syndic either seal-and-selects that staged head as the whole turn's sole canonical item authority
  or selects none of it. It never splices buffered facts, a durable live prefix, selected GUI
  text, or a partial repair response into canonical repaired history.
- The snapshot records repair adapter version, pinned CAS release, request and response digest, exact CAS and Syndic identities, repair time, and the original capture-gap reason.
- Snapshot items are semantically final provider items. They do not fabricate live timestamps, deltas, approvals, or source-event sequence positions that the repair read did not prove.
- Missing thread, turn, item, full-item view, required identity, terminal status, required field, or required media makes repair incomplete. Similar content, item order, text equality, or a path guess never substitutes for identity.
- Atomic seal-and-selection of the complete staged snapshot head also enters `FinalizingHistory`; that
  mutation does not rebuild or publish projections. Explicit incomplete convergence likewise enters
  `FinalizingHistory`, fixes the closed incomplete authority and provenance, and selects or claims no
  staged snapshot. For either outcome, bounded durable work rebuilds every affected projection to a
  fixed point, publishes one coherent transcript presentation generation, and only afterward
  releases the same-thread gate atomically.
- A bounded rejection, timeout, process or connection loss, possible request dispatch without one
  complete response, incompatible or incomplete response, out-of-bounds response, or inability to
  complete the sole authorized request converges the turn to explicit incomplete history. No
  restart, recovery path, user action, or later fence acquisition may authorize another historical
  request for that turn.
- Until repair finalization or explicit incomplete convergence releases the gate, the same Syndic
  thread cannot start a successor, fork from the affected prefix, perform rollback/replacement
  execution across it, or compact it. Other threads remain independent.
- After successful atomic repair, Syndic remains the sole normal read authority. Later ordinary transcript reads never consult the repair adapter or CAS.

## Generated Media During Repair

- Standalone image-generation base64 remains discarded on both normal and repair ingress.
- The CAS-live repair coordinator in `beryl-app` owns the whole-turn media preparation lifetime. When
  a repair snapshot names a nonempty `savedPath`, it promptly reads that exact path through the
  snapshot's authenticated runtime boundary and prepares the bytes in the Beryl-home image sidecar
  before the source can disappear.
- Each bounded repair-media stage home command atomically records one immutable Asset page and its
  matching noncanonical Syndic media witness under the existing target turn/item identities. It
  consumes current-generation sidecar admission evidence after byte verification and durability
  finish outside the writer, and advances exact ordered membership, direct entry locators, counts,
  and matching commitments. Staging publishes no canonical metadata, reference, or disposition.
- The [image-assets system](../image-assets/design.md) owns paged Asset sealing, sidecar evidence,
  visibility, and owner-qualified direct lookup. Complete durable page chains seal without another
  full traversal. One final cross-domain home command validates only compact exact sealed heads,
  revisions, media commitments, and the consumed repair gate; Beryl-state publishes one Asset
  visibility selector while Syndic selects the whole snapshot and enters `FinalizingHistory`.
  Final read/write work, memory, and reconciliation are independent of item/media count and perform
  no sidecar I/O, page-set scan, or bulk metadata promotion. Either both selections commit or neither.
- Selected Syndic resource metadata carries exact repair-owner/page/entry authority for bounded
  Asset reads. AssetId-only lookup cannot expose staging. Resource dispositions derive from the
  selected snapshot, and subsequent bounded projection work retains those references without
  requiring a whole-set copy or separate per-resource publication before selection.
- Failure or explicit incomplete convergence leaves prepared sidecars and unselected staging inert
  for future home-wide garbage collection. Fresh recovery may seal or select only an already fully
  staged candidate from compact durable completion evidence and fresh handles. It neither rehashes
  all admitted sidecars nor fills missing stages, rereads CAS, or accesses the old runtime path.
  Exact selector reconciliation distinguishes nonpublication, complete publication, and corruption;
  no old capability or mixed-domain selection authorizes success.
- The runtime path itself is never durable media authority.
- Missing, empty, changed, unreadable, unsupported, oversized, or unauthenticated `savedPath` makes the repaired turn incomplete. Beryl never falls back to inline base64, a similar file, a URL, or prior transient bytes.

## Native Lineage And Fresh Projection

- An exact valid binding continues on its existing CAS thread and sends no reconstructed Syndic history.
- A branch or replacement uses exact CAS-native lineage when immutable Syndic parentage and binding proof map to the required terminal CAS prefix. An empty prefix starts a fresh native thread.
- The ordinary path requires the pinned CAS-native continuation and fork primitives. Missing, stale,
  unavailable, or unprovable native lineage may instead establish a fresh CAS projection through
  one bounded, one-time `thread/inject_items` injection of the complete eligible Syndic prefix.
- Injection supports only the closed lossless item subset proven by the pinned release. It excludes the current submitted input, hidden developer instructions, raw reasoning, activity-only summaries, diagnostics, and resource bytes.
- If any required history item lacks a lossless supported representation or the complete prefix exceeds its approved byte, item, or model-context budget, execution remains unavailable. Beryl does not summarize, omit, truncate, or replay only a suffix.
- Recovered Syndic history is never an ordinary per-turn payload. A successful injection starts no
  model turn and is recorded once in the new binding. The injected prefix is never resent on later
  `turn/start` or `turn/steer` requests.
- Recovery injection uses one domain-separated source identity derived from the exact home,
  Syndic thread, selected path, represented prefix, source revision, totals, and sequence digest.
  That complete identity, not a page handle or caller-chosen proof, binds the bounded injected
  sequence across the storage and provider boundary.
- After exact CAS recovery injection succeeds, Beryl observes Unix completion time immediately
  then. The earlier request timestamp is never substituted or relabeled as injection completion;
  only this post-success completion observation may feed durable recovered-injection publication
  and its ordering authority.
- Failed or ambiguous fresh-target publication returns no projection capability and uses targeted
  natural-record reconciliation of that exact opaque binding-publication scope. Only `ExactNew`
  followed by a reread of the named complete eligible binding may establish another fresh
  projection. An ambiguous, uncommitted, or otherwise stale target remains non-authorizing
  provenance and cannot be promoted through resume or reinjection.
- Repair-required history is not eligible for fork, rollback, compaction, or injection. A turn that has converged explicitly incomplete is eligible only where the owning feature and pinned CAS lineage contract explicitly permit that incomplete boundary; it is never presented as repaired.

## Branch Selection Context

- Branch-discussion selected context derives from one immutable Syndic envelope and is projected once before the first branch-local user turn.
- The exact selected assistant passage remains untrusted assistant-role context. It never becomes ordinary user input, developer instructions, or hidden application authority.
- The projection sends one bounded provenance-framed assistant/output-text item carrying that exact
  accepted passage. It does not use `additionalContext`, developer instructions, ordinary user
  input, or an application-private CAS wrapper.
- Rendering the synthetic discussion-context item performs no CAS request, and CAS projection never reads rendered text.
- Missing provenance, unsupported shape, or exceeded context limit makes branch execution unavailable rather than guessed or truncated.

## Exact Soft Stop

- Beryl provides exact CAS soft interruption as the sole stop mechanism for one exact active
  ordinary or provider operation.
- Durable stop authority names the Syndic thread and turn, operation kind, binding and execution snapshot, runtime/process/loaded-session generation, CAS thread and turn, cause set, and one request-attempt identity.
- Stop admission and the same-thread operation gate publish atomically before `turn/interrupt` may be sent. Later callers targeting the same exact operation join that one stop; they do not create another interruption.
- The foreground connection that owns the exact active target sends the sole interruption request. A detached, resumed, or request-only connection cannot substitute.
- Command-execution and file-change approval denials already interrupt their provider operations.
  Their ordered denial records that no separate interruption is required and must not emit a second
  `turn/interrupt`.
- Permission-expansion denial first admits or joins the exact durable stop with the
  interrupting-approval cause, then sends the denial, and only afterward dispatches the stop's sole
  interruption attempt when it has not already crossed a request byte. An already-dispatched
  attempt is joined; permission handling never creates a second interruption.
- A response proves request acceptance or rejection, not terminal history. Exact terminal evidence or authoritative target loss owns convergence.
- Proven local nondispatch may safely reopen an ordinary still-exact target only under the same
  operation fence. An interrupting-approval cause permanently forbids reopening that target even
  when nondispatch is proven. Once any byte may have crossed, Beryl never repeats the interruption
  automatically or after restart.
- Input accepted while stopping remains ordered next-turn work and is never retroactively steered.
- Store failure after durable stop admission preserves the durable stop record but creates no
  retained dispatch capability.
- The only pre-admission exception is one narrow volatile soft interruption after exact proof that
  durable stop admission did not commit: admission failed before reaching a writer, or the writer
  returned the typed `NotCommitted` outcome. `Committed`, `Indeterminate`, transport or store loss,
  and any state in which durable authority may exist are ineligible.
- Volatile authorization is a process-local, single-use typed value for the same existing
  authenticated foreground target and its sole driver. It cannot be used by a detached,
  replacement, or resumed session and cannot authorize target selection after the exact binding is
  lost.
- For GUI consumers this system projects one opaque exact-soft-stop eligibility fact only while the
  exact foreground target, sole driver, generations, and operation fence remain valid. The fact is
  revoked on drift and cannot be reconstructed from displayed ids, activity, or backend
  availability. Each durable or volatile request also projects one opaque stable feedback identity;
  volatile feedback remains valid without implying a durable operation, retry, or terminal claim.
- Before the sole driver consumes volatile authorization or dispatches `turn/interrupt`, the
  same-thread cut cancels any process-local lifecycle continuation intent for that exact target and
  preserves accepted input as ordered next-turn work. No volatile request outcome or process restart
  reconstructs, reschedules, or silently restarts the cancelled continuation.
- A volatile attempt has no durable stop operation, join, retry, restart recovery, or durable
  success claim. Its matching acceptance is not terminal evidence; terminal convergence still
  requires the ordered live stream or authoritative target loss.
- Hard stop, diagnostic hard stop, child or subagent termination, command-process termination,
  process shutdown as turn control, and thread-wide coarse cleanup are outside the product contract.

## Context Compaction

- Context compaction is a CAS provider operation on one exact idle exclusive projection. It is not an ordinary conversation turn and does not change Syndic parentage or represented-prefix identity.
- Admission requires a healthy store, an idle same-thread gate, no accepted-next work, no repair-required turn, and an exact valid foreground projection.
- One durable operation and one request-attempt identity authorize at most one `thread/compact/start` dispatch. Possible dispatch is never retried automatically.
- The random durable operation identity is retyped as the provider-operation turn identity. The
  snapshot identity is the system-owned domain-separated hash of the complete admission target;
  callers cannot choose, shorten, or substitute either identity.
- Exact success requires the pinned completed compaction item and matching successful terminal evidence in order. Acknowledgement, timeout, an idle observation, or item completion alone is insufficient.
- Accepted input during compaction is durable ordered next-turn work.
- Soft stop may target compaction only after its exact CAS turn identity is durably known and the
  same-thread gate still owns that exact compaction operation. A guessed current turn, thread-wide
  target, replacement operation, or stale observation is ineligible.
- If the compaction interruption is proven locally nondispatched, reconciliation may reopen only
  that same compaction operation under its unchanged operation fence and compact-start disposition.
  Possible interrupt dispatch, an interrupting-approval cause, target drift, or authority loss
  forbids reopen; none authorizes a replacement compact-start or another interruption.
- There is no hard-stop or background-cleanup escalation.
- Successful compaction changes only CAS internal context. Syndic remains canonical history and ordinary transcript reads remain independent of compacted CAS state.

## Automatic Lifecycle Continuation

- Lifecycle continuation intent is process-local and keyed to the exact yielding turn. Process loss,
  exact stop admission, admitted process shutdown, compaction failure, or authority loss discards
  it. Thread switching and nonfinal window close leave it eligible under the ordinary gates.
- Process shutdown cancels that volatile intent at the same-thread admission cut before a
  continuation can win. A continuation already durably admitted before the cut remains an ordinary
  pending turn. Neither failed shutdown nor restart reconstructs the cancelled intent.
- After the yielding turn reaches terminal-history fixed point, Beryl compacts only when the same-thread gate is idle and no accepted-next work already wins.
- Syndic owns atomic publication of the fixed continuation content through its
  [fixed-content operation](../../../crates/syndic-storage/doc/design-history-storage.md#fixed-lifecycle-content-publication).
  The app consumes only the sealed result. Existing incomplete or conflicting content produces
  bounded continuation failure without changing that content or discarding accepted input.
- If accepted input exists when compaction settles, that work wins. Otherwise Beryl may atomically admit the exact fixed continuation text defined by the lifecycle feature while leaving the operator's current draft unchanged.
- Restart never reconstructs continuation intent. An already durably admitted continuation turn recovers as an ordinary pending turn and is not duplicated.

## Startup And Runtime Recovery

- Startup reads only durable Beryl-home and Syndic authority. It never uses CAS catalog or history reads to discover conversation work.
- Pending work proven not dispatched may resume through a fresh exact projection. Possible-dispatch work is never replayed and converges through explicit incomplete or delivery-unknown state.
- Any turn already marked repair-required remains a same-thread barrier. Recovery may consume its
  still-unconsumed sole request authorization when the exact pinned source remains available. If
  that authorization was already consumed, recovery may finish only an already complete durable
  staged response and matching repair-media evidence; otherwise it converges the turn incomplete.
  An unavailable source or a sole request that cannot complete likewise converges incomplete, and
  no case issues another historical request.
- The [backend-runtime system](../backend-runtime/design.md) owns whole-service recovery ordering
  through atomic replacement publication. Behind that startup fence this system contributes
  convergence of durable pending, stop, compaction, and repair obligations; only after publication
  does it reacquire CAS projections from durable binding authority.
- No old connection, driver, broker, router, projection, loaded session, lease, candidate,
  scheduler, worker, or process-local request capability crosses that recovery boundary.
- A lost connection or process cannot recreate soft-stop dispatch authority, automatic continuation intent, steering authority, or an in-flight repair response.
- Startup opens same-thread scheduling only after pending terminal, repair, stop, and compaction obligations reach their durable fixed point.

## Concurrency And Bounds

- Same-thread submission, steering, repair, fork, replacement execution, rollback, compaction, stop, binding transition, and terminal finalization pass through one revisioned operation gate.
- Projection establishment uses one bounded process-wide flight per Beryl-home generation and Syndic thread.
- Normal live buffers, outage buffers, repair requests and responses, staging pages, schedulers, and active workers have explicit count and byte bounds.
- Durable accepted-input and canonical history remain paged logical collections; their total size does not determine resident memory.
- Quiet live streams remain active until terminal evidence, protocol failure, transport loss, or managed-process exit.

## Security And Policy

- CAS remains responsible for authentication, workspace selection, managed configuration, enterprise policy, sandboxing, approvals, skills, MCP, tools, subagents, and rate limits.
- Live capture, repair, recovery injection, and branch context never retain authentication material, loopback tokens, hidden developer instructions, policy-private fields, or raw approval payloads.
- Repair authorization is derived only from exact durable correlation and the pinned adapter. User-supplied ids, filesystem paths, diagnostic text, or visible transcript content cannot authorize a repair read.
- Stored errors and repair provenance are bounded and redact secret-like values before durable commit.
- Syndic capture and repair do not broaden CAS permissions or bypass policy decisions.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers:

- `privileged-access/v1`
- `external-side-effects/v2`

Execution/view separation protects exact provider-effect custody, durable accepted work, and capture
through nonfinal detachment. Verification must cover detach and reattach during preparation,
streaming, in-flight approval handling, compaction, continuation, and terminal-history work; bounded registry
saturation and idle reclamation; and shutdown admission racing new work, final-window changes,
duplicate requests, authority loss, and durability failure. Independent semantic review must examine
the execution lease, request-routing privilege boundary, and all dispatch/successor cuts. Background
execution does not weaken privacy, recovery, same-thread serialization, or resource guarantees.
