# Scope

Execute only Checkpoint 3 of the Beryl-home architectural rework tracked by `doc/rework/beryl-home/REWORK.md`: reconstruct Syndic's permanent typed storage boundary inside the one Beryl-home database, implement durable threads, drafts, submitted turns, accepted input, canonical history and bounded transcript projections, add the exact normalized CAS lineage and `thread/inject_items` boundary, and implement one exclusive recoverable CAS execution projection per Syndic thread.

Checkpoint 2 is independently complete. Checkpoint 3 code may depend only on the final `beryl-model`, `beryl-home-store`, `beryl-state`, `syndic-storage`, `beryl-backend`, and app-orchestration boundaries assigned by target authority. It must not import archived source, open another database, expose raw Fjall or app-server JSON, query CAS historical transcripts, replay recovered history when native lineage is proven, repeat an injected prefix, or add a compatibility adapter.

This checkpoint does not implement the multi-window shell, runtime/root pickers, thread selector, transcript GPUI activation, branch-discussion creation or handoff, image-byte admission and runtime projection, generated-title workers, semantic search, garbage collection, or theme/editor redesign. Those remain assigned to later checkpoints. The process entry retains its explicit compile-time bootstrap gap.

# Phase 1: Establish Syndic Package And Pure Value Boundaries (finished)

- Replace the documentation-only `syndic-storage` root with its final package module structure and dependency direction through `beryl-home-store` and `beryl-model`; Fjall remains private to `beryl-home-store`.
- Complete only the pure bounded Syndic identities, typed revisions, lifecycle values, immutable parent/context descriptors, accepted-input ordering values, projection positions, and CAS proof values required by this checkpoint.
- Keep clocks, id generation, filesystem observation, provider calls, and policy outside pure model types.

Edge cases include zero or exhausted revisions, oversized provider identities and text fields, identity-type mixing, invalid lifecycle combinations, and Serde shapes accidentally becoming storage authority.

Verification must pass focused model/package tests, API documentation examples, locked metadata and dependency trees, source-size checks, and forbidden dependency/import scans.

Resumable milestone: permanent package and pure-value boundaries exist without a mounted record schema, compatibility facade, or second physical store.

# Phase 2: Register Core Syndic Schemas And Reopen Validation (finished)

- Register one exact process-local Syndic domain through `beryl-home-store` with versioned private families for threads, drafts, turns, accepted input and its queue-order indexes, source events, canonical items, transcript views, projections, resources, CAS bindings, and every required reverse or ordering index.
- Implement exact codecs, typed bounded reads, and exhaustive startup, verification, and recovery validation before publishing the Syndic handle.
- Validate exactly one current draft per thread, matching ownership, one-way draft-to-turn identity consumption without live raw-payload collisions, immutable parentage, committed-tail reachability, acyclic parent chains, monotonic ordering, binding reverse uniqueness, one pending/active/unknown-terminal turn only at its origin thread's committed tail, ordinary-user-only replacement targets, and referenced-record agreement while allowing unreachable terminal history. Every discussion-context envelope must resolve its exact selected bytes and range through one current finalized assistant projection owned by a proven-terminal source turn.

Edge cases include missing or duplicated drafts, cycles, cross-thread draft ownership, dangling tails or parents, contradictory indexes, unsupported versions, unknown physical records, duplicate CAS bindings, context sourced from live or stale projections, parent or child replacement after context admission, and orphan turns that are valid but unreachable.

Verification must inject each malformed physical and semantic shape through test-only exact-domain seams, prove normal/recovery rejection, and prove valid large histories validate with bounded memory and no CAS or GUI access.

The Operator authorized a bounded `test-faults`-only physical-envelope corruption extension. It must remain tied to an exact live domain handle and codec owner, reject codec-valid envelopes, enforce fixed byte ceilings and `SyncAll`, and expose no production or reusable raw-storage API.

Resumable milestone: an empty or populated Syndic domain can reopen authoritatively, but no public workflow mutation exists yet.

# Phase 3: Implement Thread Creation And Durable Draft Persistence (finished)

- Implement atomic ordinary thread-plus-current-draft creation, deterministic thread-from-existing-tail creation, exact revisioned draft reads and updates, and immutable draft parent/thread/context ownership.
- Add the final app-owned non-GPUI draft persistence service for dirty-only timed autosave, explicit lifecycle flush, stale-result rejection, and restart preload without mounting the shell.
- Validation rejection, stale-revision conflict, and cancellation observed before writer admission leave the durable draft unchanged. Once writer admission begins, cancellation does not retract the save; a surfaced storage or persistence failure is treated as an ambiguous durable outcome until same-home verification or recovery and an exact current-draft reread reconcile identity, revision, and payload. The editor payload remains intact and caller-visible success stays gated throughout reconciliation. Avoid writing when payload and durable revision are unchanged.

Edge cases include concurrent creation from one historical tail, autosave racing activation or submission, lifecycle close draining rather than cancelling an admitted save, a recovered dirty draft, empty payload, timer-setting changes, ambiguous post-admission failure, and a failed store generation.

Verification must prove one current draft always survives creation, close/reopen, dirty-only autosave, explicit flush, conflict, pre-admission cancellation, ambiguous post-admission failure, verification/recovery, and exact reconciliation. Crash and persistence-failure cuts may recover the whole old or whole new draft state but never a partial mutation, competing current draft, false success, or lost editor payload.

Resumable milestone: ordinary threads and mutable durable drafts are usable without creating submitted history or contacting CAS.

# Phase 4: Replace Single-Record Content With Chunked Authority (finished)

- Replace embedded whole-payload draft, accepted-input, and canonical-item values with small owner metadata plus exact ordered content manifests and bounded content chunks. A logical draft or canonical item has no schema-level whole-content byte ceiling; each physical chunk and each read/write command remains bounded.
- Implement crash-safe content construction as bounded staged chunk commits followed by one atomic publication of a sealed manifest reference. Unreachable partial generations are retained until future garbage collection and can never become the current draft or finalized canonical authority.
- Correct Phase 3 draft creation, preload, dirty-only autosave, flush, and reconciliation to use the chunked authority without changing visible draft semantics. Submission must be able to reuse the sealed draft content reference rather than copying multi-million-token text into another record.
- Make accepted-input and canonical-item metadata reference chunked content, while exact resolved image-marker facts remain independently ordered and bounded. Do not implement admission or live capture in this phase.

Edge cases include empty shared content, multi-million-token UTF-8 drafts, chunk-boundary UTF-8 and atom encodings, content-identity collision, interrupted staging, duplicate append, stale draft publication, cancellation before any admitted command, ambiguous failure during staging or publication, same-content deduplication, and restart with unreachable partial content.

Verification must prove bounded command work and bounded page reads, exact round trips above the former 262,144-byte ceiling, atomic old-or-new draft publication across every cut, immutable sealed chunks, coherent reopen validation, and no whole-content value in `drafts`, `accepted-inputs`, or `canonical-items`.

Implementation result: Syndic now owns content-addressed `content-manifests` and `content-chunks` families, exact composer and UTF-8 encodings, full-digest collision authority, bounded 16-chunk staging commands, sealed immutable references, and separately ordered marker resolutions. Draft, accepted-input, and canonical-item records retain only compact content metadata; ordinary thread creation shares canonical empty content; draft preload assembles bounded pages; and app persistence resumes unreachable staging before one atomic seal-plus-draft publication.

Verification result: 22 default-feature and 55 all-feature `syndic-storage` nextest cases pass, together with 21 `beryl-model`, five app service, and 16 focused app persistence/fault cases. The suite proves an exact draft above 10 MB with UTF-8 split across a physical chunk boundary, reopen and resume during staging, one-chunk bounded reads, same-content reuse, compact accepted/canonical owners, collision refusal, exact app preload above 2 MB, and whole-old-or-whole-new recovery after every staged-command and final-publication persistence cut. Locked default/all-feature checks, warnings-denied scoped Clippy and Rustdoc, formatting, metadata, active-file size, obsolete-source, and whitespace audits pass; the existing inactive `beryl-backend` warnings remain the declared later-checkpoint cutover gap.

Resumable milestone: drafts and all future submitted/canonical owners use one exact chunked content authority, while no submission or live capture mutation exists yet.

# Phase 5: Implement Submission, Queueing, And Replacement Mutations (finished)

- Correct the remaining invalidated Phase 2 admission foundation recorded in `doc/failures/syndic-phase4-admission-foundation.md`: typed final marker labels and resolved submitted atoms, per-marker asset-reference ownership, generation-keyed transcript entries with bounded publication, immutable replacement-draft parentage, and CAS path validity independent of draft-only thread revision changes. Image-byte admission, GUI paste, and Host/WSL runtime projection remain in their later checkpoint.

- Architecture gate resolved: implement exactly one independently revisioned input gate per thread; exact known-or-explicitly-unknown CAS steering target proofs; permanent accepted-order history distinct from live-only steering/next-turn indexes; bounded live-route counters and bytes; and execution snapshots without accepted-input vectors. V1 permits 256 simultaneously live fragments and 268,435,456 live logical UTF-8 bytes per thread, while retained accepted history and thread turns remain unbounded by that live-work ceiling.

- Implement one atomic idle submission that transitions the same draft identity into a submitted turn, advances the selected thread tail, records delivery intent, and creates the replacement current draft.
- Implement active-turn and compaction admission that freezes each draft into one ordered accepted-input record without creating a competing turn, plus an identity-preserving disposition change to the next-turn queue after non-steerable rejection.
- Implement durable replacement-edit intent, cancellation, and accepted replacement submission from the edited turn's immutable parent without rewriting the original path.

Edge cases include duplicate activation, competing draft revisions, unknown active CAS turn id, queue overflow, stop racing accepted input, first/root submission, replacement of a root turn, stale selected-path proof, and failure after durable admission but before delivery.

Verification must prove idempotency, exact ordering, one active same-thread lifecycle, immutable old paths, coherent restart state, and no composer-clear or success authority before `SyncAll`.

Implementation result: Syndic now admits one idle current draft as its deterministic pending submitted turn or one non-idle draft as a permanently ordered accepted-input identity under an exact independently revisioned input gate. Live steering and next-turn indexes have exact count and byte ceilings independent of retained history; steering rejection preserves identity while becoming retryable next-turn work. Replacement editing copies exact immutable user content and marker provenance into the current draft without changing its parent, selected path, or CAS binding; cancellation clears only intent, while accepted sibling and root replacements preserve the old path. App-owned builders combine Syndic admission with every required per-marker asset-reference move in one home command and duplicate rather than move historical ownership for replacement editing.

Reconciliation result: publication-gated reads classify an admission as absent, exact, or collided using the expected content, immutable admitted owner, exact marker set, caller-named replacement draft, and advanced input gate. Duplicate activation is resolved through that status rather than replaying a consumed draft mutation, and surfaced post-admission failures retain caller state until same-home verification or recovery establishes one coherent result across Syndic and asset domains.

Verification result: 23 default-feature and 68 all-feature `syndic-storage` nextest cases pass, together with five focused `beryl-app` cross-domain admission/replacement cases. The matrix proves stale draft and path rejection, duplicate activation, known and unknown steering targets, pending/compaction/stopping routes, exact live count/byte boundaries, steering rejection, root and sibling replacement, restart validation, historical asset-reference agreement, atomic cross-domain rejection, and before-commit, after-commit-before-persist, and after-persist outcomes. Locked checks, warnings-denied scoped Clippy and Rustdoc, formatting, metadata, dependency, active-file-size, archived-source, authority-wording, and whitespace audits pass. The inactive `beryl-backend` dead-code warnings remain the declared later-checkpoint cutover gap.

Resumable milestone: every user-input admission has one durable Syndic outcome and no operation can create competing same-thread children.

# Phase 6: Implement Canonical Live History And Turn Lifecycles (finished)

- Implement monotonic idempotent source-event admission, canonical user/assistant/operational items, exact external identity correlation, coalesced streaming updates, explicit pending, active, complete, interrupted, failed, incomplete, and unknown-terminal turn states, and one-way finalization of current projection frontiers under proven-terminal turns.
- Preserve assistant phase metadata when supplied, retain unknown phase when absent, and keep operational activity outside parent transcript narrative.
- Commit source, canonical, lifecycle, and stale-projection effects atomically when practical without buffering a complete response. A proven-terminal transition closes ordinary event admission and marks any unfinished derived frontier stale; no later event, recovery, or replacement path may rewrite a terminal-plus-current finalized canonical item or projection.

Edge cases include duplicate or out-of-order events, mismatched thread/turn/item ids, stream loss between deltas, late terminal events, process death, unsupported event types, bounded coalescing loss, events for stale bindings, attempted post-finalization updates, and terminal turns whose stale projection work still requires first-time completion.

Verification must replay deterministic event sequences through reopen and recovery, prove exact idempotency and terminal convergence, and prove no event mutates parentage or another thread's active state.

Implementation result: Syndic now admits one bounded normalized source event at a time under exact turn-state, input-gate, per-turn sequence, current-tail, and optional active-CAS correlation proofs. Turn activation, text-item start, coalesced delta, item completion, and complete, interrupted, failed, incomplete, or unknown-terminal outcomes atomically advance source records, canonical assistant or operational items, deterministic item-owned content manifests and chunks, reverse source/CAS indexes, lifecycle state, history completeness, input-gate state, and stale transcript authority. Exact replay is distinguished from same-sequence collision before stale caller revisions, proven-terminal turns reject new source events, and terminal frontier advancement can finalize only already admitted open items without manufacturing bytes or provider completion.

Reopen result: registration replays every item-local source-event index in bounded memory and requires exact agreement with external identity, kind, assistant phase, source frontier, chunk count, encoded length, chain digest, completion state, canonical revision indexes, terminal ending event, and contiguous finalized-item frontier. Fault-injected persistence cuts reconcile to the whole old or whole new canonical event effect, and a mismatched CAS turn or item cannot mutate another turn, item, gate, parent edge, or thread.

Verification result: all 20 production-feature cases in the explicit feature-independent storage matrix and all 75 all-feature `syndic-storage` nextest cases pass, including five canonical live-history and two physical-fault reconciliation tests. Locked default library/example and all-feature all-target checks, warnings-denied matching Clippy scopes and all-feature Rustdoc, the public live-event example, formatting, metadata/dependency, Phase 6 source-size, raw-Fjall/archived-source, unfinished-marker, heading, and whitespace audits pass. The active `beryl-app` library boundary compiles; its retained pre-rework theme tests and inactive `beryl-backend` dead-code warnings remain the already-declared later-checkpoint cutover gap and received no compatibility shim.

Resumable milestone: CAS-normalized events can produce durable canonical Syndic history even though transcript projections and execution orchestration are not yet mounted.

# Phase 7: Build Bounded Transcript And Resource Projections (finished)

- Derive deterministic transcript-view records and Markdown projections from canonical items with stable provenance, exact revisions, cursor positions, the accepted paragraph, code, table, page, and chunk thresholds, and immutable finalized identities after a proven-terminal current frontier is published.
- Implement bounded history summaries, transcript pages, projection-set reads, immutable context-envelope reads, resource metadata, and explicit range reads; keep renderer residency and GPUI behavior outside storage.
- Replace the invalidated Phase 2 projection skeleton recorded in `doc/failures/syndic-phase7-projection-skeleton.md`: add exact physical content-byte and logical-text span indexes, explicit item-projection generation/build records, explicit transcript path/build records, structured block/source-range projection payloads, and deterministic domain-separated derived identities.
- Project one bounded canonical chunk and undecided Markdown window per construction step. Preserve closed live block prefixes, supersede only generation-owned open suffixes, publish no partial generation, and convert malformed or bounded-out Markdown to exact source-preserving spans rather than buffering, truncating, or failing the full transcript.
- Snapshot both logical bytes and ordered render-piece count in every content reference, so
  zero-width image markers remain visible while later pieces appended under one live content id
  cannot enter an older projection generation.
- Back Phase 7 textual code/table resources with indexed canonical logical-text ranges and bounded structural metadata. Do not duplicate them into sidecars or broaden the current whole-buffer sidecar API. Image-byte ownership, sidecar-backed external resources, and Host/WSL runtime projection remain deferred to Checkpoint 6.
- Build each selected transcript generation through bounded tail-to-root path collection followed by bounded root-to-tail entry publication, then derive history-summary completeness through one shared selected-path routine.
- Bind incomplete transcript construction to the exact current thread revision, but retain an
  immutable completed generation across draft-only and accepted-input revision advances whenever
  its committed tail and selected-path digest still match exactly.
- Persist and validate one deterministic compact ancestor skip per non-root turn so finalization can
  prove selected-path membership with constant memory and a fixed 2,080-point-read ceiling across
  the full `u64` depth domain. Canonical closure of a retained off-path turn must not invalidate the
  replacement-selected transcript or change its history summary.
- Split terminal item closure into immutable canonical-source freezing followed by projection construction and final finalized-frontier publication. A visible item reaches that frontier only with one current completed projection set; do not preserve the Phase 6 conflation through a lifecycle exception.

Edge cases include huge paragraphs, degenerate or malformed Markdown, code blocks and tables at exact thresholds, UTF-8 split boundaries, zero-width trailing and marker-only composer pieces, CRLF and unfinished fences, stale unfinished projection revisions, source advance during a build, rebuild after interruption before finalization, rejected rebuild of finalized history, missing resources, sparse and deeply branched history, unknown assistant phase, deterministic identity collision, and unreachable but retained generations and resources.

Verification must prove deterministic rebuilds, exact source provenance, bounded page/range materialization, stable positions, no visible truncation, and no full-history or heavy-byte load for metadata-only reads.

Implementation result: canonical content now has exact physical-byte and logical-text indexes, and every projection source snapshot fixes both its logical-byte frontier and its ordered piece frontier so zero-width markers survive without admitting later live pieces. Item projection builds parse one bounded canonical range and undecided Markdown window per step, emit at most 64 bounded records, preserve a generation-independent closed prefix, isolate only the mutable suffix by generation, and publish code or table resources as bounded metadata over canonical logical ranges. Public transcript, projection, and range reads clamp oversized callers to 256 records and 65,536 stored bytes. Transcript builds durably collect the selected path and publish root-to-tail entries in 64-entry batches; deterministic ancestor skips prove exact selected-path membership with constant memory and at most 2,080 point reads over the full `u64` depth domain. Immutable canonical freezing, projection completion, and final visible-frontier publication are separate operations, while completed transcript generations remain current across draft-only or accepted-input revision changes when their exact tail and path digest still agree.

Reopen result: exhaustive validation replays exact piece, chunk, source, generation, stable-prefix, resource, transcript-path, entry, head, and history-summary authority without loading a whole thread or heavy resource. Interrupted projection parsing, transcript path collection, and transcript publication resume after physical close and reopen; superseded generations remain coherent but unreachable. UTF-8-adjusted chunk boundaries, marker-only and trailing zero-width pieces, malformed or unfinished Markdown, selected and off-path finalization, and draft-only thread revision advances retain their exact intended state. Before-commit, after-commit-before-persist, and after-persist cuts around both final item-projection publication and final transcript/head/summary publication reconcile to one complete old or complete new state and never expose mixed families.

Verification result: all 43 default-feature and 92 all-feature `syndic-storage` nextest cases pass. All 292 all-feature foundation cases across `beryl-model`, `beryl-home-store`, `beryl-state`, and `syndic-storage` pass in one inline-elevated run, including the two Windows symlink-privilege cases and both new Phase 7 publication fault matrices. Locked default and all-feature all-target checks, matching warnings-denied Clippy and Rustdoc scopes, formatting, metadata, forbidden-boundary, unfinished-marker, heading, whitespace, and source-organization audits pass. Independent completion review found no Phase 7 architecture defect; its only blocking finding was the missing final-publication fault evidence, now covered. This phase supplies the bounded storage and presentation-record boundary; later composer activation must avoid a second whole-draft materialization, and later GPUI transcript activation must preserve these page, realization, and GPU-residency bounds rather than flattening a complete turn or thread.

Resumable milestone: later transcript providers can read complete bounded Syndic projections without CAS history or GUI-local caches.

# Phase 8: Implement The Exact Normalized CAS Lineage Boundary (finished)

- Extend `beryl-backend` with the targeted 0.144.1 normalized `thread/inject_items` request and closed user/input-text plus assistant/output-text item subset, preserving order and distinguishing success, structured rejection, transport loss, and unknown completion.
- Complete exact continuation, resume, fork, rollback, idle-state, loaded-session, turn-start, steering, interruption, compaction, and capability-proof inputs required by the CAS-live system without exposing raw protocol JSON.
- Admit recovery injection only when live compatibility evidence proves the exact target contract; provide no older request path or contextual fallback.

Edge cases include extra or unknown item fields, wrong role/content pairs, empty or oversized vectors, active target threads, response-id mismatch, ambiguous disconnect, unloaded threads, compaction, and a server that advertises a method but violates semantics.

Verification must use exact source-backed fixtures and isolated live protocol probes for validation atomicity, ordering, later-turn visibility, no implicit model turn, resume/fork behavior, and ambiguity classification while preserving the reusable exploration memory.

Implementation result: `beryl-backend` now targets only `codex-cli 0.144.1` and exposes typed CAS thread/turn identities, exact thread start/resume/full-fork/inclusive-through-turn-fork/rollback inputs, exact thread and turn execution overrides, metadata-only lineage responses, bounded turn-start and steering results, interruption, compaction, and subscription cleanup. Resume and rollback reject the wrong returned thread, fork rejects source-thread reuse, and steering rejects a returned turn other than the expected active turn. Compatibility admission combines the exact initialize version and typed non-destructive method-shape probes with retained source-backed and focused live semantic evidence; it neither creates a synthetic model turn nor accepts an older schema.

Injection result: one consuming fresh-loaded-to-fresh-idle typestate authorizes one stable `thread/inject_items` request. Its closed canonical API permits only nonempty user/input-text and assistant/output-text messages, preserves exact order and UTF-8 bytes, caps both canonical text and the derived item count at 262,144, exposes no raw item or protocol-error-data escape hatch, and classifies exact success, normalized structured rejection, transport loss, and unknown completion. Every outcome consumes the target, so no caller can retry the same fresh CAS thread in place.

Verification result: all 200 all-feature cases across `beryl-backend` and `syndic-storage` pass, including 13 focused exact-wire, identity-mismatch, fresh-idle, closed-shape, 65,703-byte branch-context, 262,144-byte/item-boundary, rejection, disconnect, invalid-success, and wrong-response-id cases. The retained isolated live probe passes native continuation, restart/resume, inclusive fork, rollback, ordered injection, later-turn and full-fork visibility, absence of an injected public turn, 262,144-byte transport, atomic malformed-batch rejection, and ambiguity abandonment against the exact 0.144.1 executable and schema hash. Scoped warnings-denied Clippy, warnings-denied Rustdoc, formatting, metadata, dependency, archived-source, obsolete/contextual-fallback, source-size, heading, whitespace, and raw-boundary audits pass; the declared inactive-backend `dead_code` cutover gap remains. Independent completion review found no blocking defect and assigned compatibility-proof enforcement at the production coordinator to Phase 10.

Resumable milestone: callers can establish native CAS lineage or request one bounded injection through a typed exact protocol boundary, but no Syndic binding coordinator calls it yet.

# Phase 9: Implement Projection Bindings And Recovery Item Assembly (finished)

- Replace the invalidated Phase 2 path-proof conflation recorded in
  `doc/failures/syndic-phase9-binding-prefix.md`: distinguish the thread's exact current selected
  path, the committed prefix CAS already represents, and recovered-injection establishment
  provenance. Implement revisioned `unbound`, `valid`, `active`, and `stale` Syndic binding
  mutations, permanent reverse CAS-thread uniqueness, loaded-process/session generation facts,
  immutable execution snapshots, one-way active-CAS-turn publication, and stale provenance
  retention with permanent retirement from later execution.
- Assemble the complete required committed Syndic path into the closed canonical recovery item sequence, enforcing the 262,144-byte and half-context-window limits without summarization, omission, truncation, repeated fragments, developer instructions, user-input wrapping, or `additionalContext`.
- Publish a recovered-lineage proof only after exact injection success and one durable local commit; ambiguous threads remain abandoned provenance.

Edge cases include missing model context-window metadata, tool or media history without a lossless item shape, exact-budget boundaries, stale tails during assembly, duplicate reverse bindings, process/session loss after injection, and local commit failure after remote success.

Verification must prove native-lineage eligibility and recovery budgets from exact records, deterministic sequence digests, reverse uniqueness, stale-generation rejection, no repeat authorization, and abandon-instead-of-retry behavior.

Implementation result: Syndic now distinguishes the exact current selected path, the committed
prefix represented by CAS, and recovered-injection establishment provenance. Revisioned binding
mutations publish `unbound`, `valid`, `active`, and `stale` states; permanent reverse indexes retain
CAS-thread ownership and one-way retirement; immutable execution snapshots and a separately
published CAS-turn correlation preserve exact loaded-generation authority. Active projection loss
retires the external thread and preserves accepted input. Source-less local terminal convergence is
limited to non-success outcomes under `stale` or `unbound` authority, while successful completion
and represented-prefix advancement require the exact published CAS source identity.

Recovery result: the storage service assembles either the exact current selected path or the exact
parent prefix of a pending selected turn from immutable canonical records. It rejects incomplete,
unsupported, media-bearing, empty, stale, or concurrently changing history and enforces the
262,144-item, 262,144-byte, and conservative half-model-context ceilings without summarization,
truncation, omission, contextual replay, or direct Fjall access. Typed publication and
reconciliation boundaries support durable recovered proof after successful injection and
abandonment after an ambiguous or failed local publication; Phase 10 owns that remote/local
choreography.

Verification result: the 18 focused binding suites, all 25 normal-feature `syndic-storage` cases,
and all 122 all-feature cases pass. The integrated foundation matrix passed 320 ordinary cases;
the only two non-elevated failures were the declared Windows symlink-privilege cases, and those
exact cases pass under elevation, completing all 322 cases with their required permissions.
Locked checks, warnings-denied Clippy and Rustdoc, formatting, metadata, dependency, archived-source,
direct-Fjall, contextual-fallback, source-size, heading, and whitespace audits pass. A fresh
read-only completion review found no blocking defect and confirmed that compatibility admission,
native-versus-recovered selection, the one-shot injection call, proof-publication choreography, and
competing projection serialization belong to Phase 10.

Resumable milestone: Syndic can represent and prepare one exclusive execution projection without yet starting or ingesting a live turn.

# Phase 10: Establish Exclusive Native-Or-Recovered CAS Projections (finished)

- Add the final app-owned non-GPUI CAS projection coordinator over typed Syndic and backend
  services. It must prefer exact continuation or resume, use inclusive fork for a proven nonempty
  earlier prefix, use fresh native lineage for an empty prefix, and select one-time fresh injection
  only for missing, stale, unavailable, or unprovable nonempty native lineage. It never dispatches
  in-place rollback.
- Carry one cumulative native CAS turn count separately from Syndic DAG depth so exact fork
  seeding, terminal advancement, and resume remain bounded and provider-operation turns cannot
  corrupt the native position.
- Require one typed compatibility-admission proof bound to the exact runtime/process generation before the production coordinator may call injection; low-level call ordering or a report checked on another runtime/session is not sufficient authority.
- Create a fresh empty loaded CAS thread, inject once, durably bind its exact loaded generation, and abandon any ambiguous or uncommitted projection without deleting CAS threads or mutating submitted Syndic history.
- Reject a context-bearing pending discussion turn through a typed unavailable outcome in this checkpoint rather than omitting its required selected-context item. Checkpoint 5 owns the separate proven selected-context projection and its combination with native fork or recovered history.
- Establish one canonical, versioned, deterministically ordered Beryl conversation-tool registry at every persistent Beryl CAS `thread/start`. Registration is cache-stable capability discovery rather than mutation authority: exact thread, turn, call, feature, and durable-state checks still authorize every tool request. Native continuation, resume, or fork is eligible only when the durable binding proves the same registry version; Beryl never silently drops requested tools or widens handler authority.
- Emit that registry only through the exact tagged 0.144.1 namespace/function schema. Remove Beryl's
  legacy flat registration model rather than depending on CAS's compatibility normalizer or mixing
  legacy and canonical entries.
- Prove against the exact admitted CAS 0.144.1 executable that start-time dynamic tools remain provider-visible with byte-identical definitions through inclusive native fork and process restart/resume. This proof is a Phase 10 gate; failure blocks the cache-stable native-branch design instead of falling back to routine history reconstruction.
- Replace the detached, unbounded process-local loaded-thread registry with one process-owned
  connection service and explicit per-thread loaded-projection subscription leases. A recovered
  source may be used or forked only through the exact connection lease that observed its injection;
  another connection in the same runtime/process generation is not equivalent authority. The
  process-wide allocator prevents coordinator-local generation collisions, and the live registry
  contains no tombstones.
- Exact recovered-injection authority survives only while its connection, loaded lease, and process
  generation remain valid. Consuming release invalidates the local token before one exact
  connection-scoped `thread/unsubscribe`; every unsubscribe status or ambiguous transport result
  remains non-authorizing. Connection/process loss revokes that owner's bounded live entries, and
  lost recovered authority durably retires the old binding before any fresh recovery attempt,
  without inventing an arbitrary durable thread limit or waiting for delayed `thread/closed`.
- Preserve every source-preserving or unclassified native resume/fork rejection, perform bounded
  automatic retry against the exact source, and then return a revision-bound
  recovery-decision-required capability. Its explicit retry keeps the source; its explicit recover
  command establishes one fresh injected target projection, retiring the source only when it is the
  target's own binding and never invalidating another thread's fork source. This checkpoint
  implements the non-GPUI decision boundary only; Checkpoint 4 mounts its accepted composer-slot
  prompt.
- Keep runtime/root selection as caller-supplied exact execution bindings; runtime configuration, readiness UI, and main-window activation remain later work.

Resolved design gate: the Operator selected crash-safe inclusive fork for every proven nonempty
earlier prefix and fresh native lineage for an empty prefix. In-place rollback is not a production
projection path. The rejected choreography and its crash cut remain recorded in
`doc/failures/syndic-phase10-in-place-rollback-publication.md`.

Dynamic-tool proof result: the retained exact `codex-cli 0.144.1` probe now uses only the canonical
tagged namespace/function schema. The complete provider-facing tool definitions remained
byte-identical through initial persistent start, inclusive native fork, and process
restart/resume: 11,021 bytes, SHA-256
`A86607BB83A2378E7F7470985B3EAEC526E38255975AD1ABECF07F5F4FFFBD02`. A negative initialization
without `experimentalApi` was rejected as required. The ordinary native branch path is therefore
the admitted cache-stable design; recovery injection remains a lineage-loss fallback.

Resolved rejection architecture: pinned CAS 0.144.1 does not provide a machine-readable lineage
verdict, so missing rollout and source-preserving failures remain indistinguishable ordinary
JSON-RPC errors. The coordinator preserves every such unclassified source through bounded
automatic retry, then returns an exact recovery-decision-required capability. The Operator's
explicit `Recover from Syndic history` command supplies authority to create one fresh injected
target projection; `Retry` preserves the source. A cross-thread fork source remains unchanged,
while a target-owned source is retired. Human-readable message, error code, and retry count never
become lineage proof. The invalidated assumption and accepted correction are recorded in
`doc/failures/cas-native-source-rejection-classification.md`.

Native-rejection implementation result: the non-GPUI coordinator now retries only unclassified
ordinary request rejection through one bounded three-attempt schedule, preserves the exact source
through exhaustion, and returns one non-cloneable revision-bound decision capability. Explicit
Retry replans and reuses only that exact source. Explicit Recover validates the complete Syndic
projection before use, retires a target-owned source only after operator authorization, preserves a
cross-thread parent source, and then establishes one fresh native or one-time injected target as
the exact prefix requires. Stale decisions reject before backend work, and the GUI contract mounts
the eventual prompt as a mutually exclusive replacement in the existing composer slot while
retaining the hidden draft and editor state.

Loaded-connection correction result: connection registration, same-connection acquisition, and
connection-wide retirement now linearize through one bounded process-owned gate that performs no
backend or storage work. A retired connection cannot publish a later loaded-thread entry, released
entries are physically removed, and poisoned retirement still records the connection as retired.
Deterministic races cover register-versus-retire and sibling-acquire-versus-retire in both possible
orders. The invalidated race is recorded in
`doc/failures/cas-projection-connection-retirement-race.md`.

Native-target mismatch correction result: an exact-prefix target binding whose execution or
conversation-tool profile is ineligible remains a typed target-owned source and is retired before
recovery. An ineligible ancestor remains source-less and untouched. Stale provenance carries the
source execution binding and exact typed unavailable reason. The invalidated source-erasure path is
recorded in `doc/failures/cas-native-target-execution-mismatch.md`.

Post-retirement revision correction result: stale publication returns its exact next binding
revision, and every automatic or operator-authorized target retirement requires bounded replanning
to observe precisely that revision before recovery. A concurrent later binding mutation rejects
instead of widening recovery authority. The target's current binding basis remains distinct from
an older usable source revision. The invalidated widening and revision conflation are recorded in
`doc/failures/cas-recovery-decision-post-retirement-replan.md`.

Recovery-completion timestamp correction result: recovered-lineage proof records the app's local
wall-clock observation immediately after CAS confirms injection, never the earlier request time.
Pre-epoch or out-of-range clock conversion fails explicitly; because remote injection already
occurred, that failure abandons the target with exact non-authorizing provenance. The invalidated
timestamp reuse is recorded in
`doc/failures/cas-recovery-request-time-as-completion.md`.

Post-remote validation correction result: every native lineage proof is built from prepared local
facts before start, resume, or fork dispatch. Recovered proof construction necessarily waits for
the loaded generation and completion observation after injection; any construction or idle-state
conversion failure then explicitly abandons the consumed target. The invalidated post-dispatch
cuts are recorded in `doc/failures/cas-post-remote-lineage-validation.md`.

Focused verification result: all 35 `beryl-app` library and Phase 10 projection cases pass, as do
both `test-faults` publication-ambiguity cases, all 8 focused native-projection storage cases, all
25 normal-feature `syndic-storage` cases, and all 134 all-feature storage cases. Coverage proves
bounded exhaustion, same-source explicit retry, target-owned explicit recovery, cross-thread
source preservation, exact retirement receipts, target-versus-ancestor mismatch behavior,
single injection, completion-time authority, pre-dispatch native validation, explicit
post-injection abandonment, stale-decision rejection, and deterministic registry retirement races.
Locked checks, scoped warnings-denied Clippy, warnings-denied Rustdoc, formatting, metadata,
source-size, heading, and whitespace audits pass. The already-declared inactive backend and
retained lifecycle-tool dead-code warnings remain later cutover work.

Edge cases include simultaneous projection requests for one thread, one CAS thread offered to two
Syndic threads, stale binding discovered after remote work, native fork disagreement, empty-prefix
fresh selection, absence of any production rollback dispatch, absent/stale/cross-generation
compatibility proof, process loss at every injection cut, cancellation, and recovery budget
failure after durable turn admission. They also include a stale or mismatched conversation-tool
registry, a tool call outside its exact authorized feature scope, fork/resume tool-definition drift,
loaded-projection lease release, last-subscriber unsubscribe ambiguity, and process loss while a
recovered projection lease is held. They include two admitted connections to the same CAS process,
recovered-source fork attempted from the wrong connection, coordinator construction races,
connection-wide lease loss, late close events, and generation ABA after release and reacquisition.
They include speculative warm-up failure while the editor remains focused, stale recovery commands,
retry exhaustion, disabled recovery because exact history is unrepresentable, pending input already
durably admitted, and an unresolved active or unknown-terminal turn that must never be replayed.
They also include registration or same-connection acquisition racing connection-wide retirement;
no retired connection may publish a later loaded-thread entry or leave a physical registry tombstone.
They include another binding mutation between explicit target retirement and recovery replanning;
the consumed decision must reject rather than widening to the later revision.
They include a delayed explicit recovery decision and system-clock conversion failure after
injection success; request time must not masquerade as completion authority.
They include any local lineage-proof validation failure before or after remote identity creation;
native validation precedes dispatch and recovered validation failure abandons explicitly.

Verification must prove one coordinator winner, native-lineage precedence, exact runtime-generation compatibility admission before injection, exactly-once injection per accepted projection, fresh-thread abandonment after ambiguity, durable proof before use, and no CAS list/history/name authority.
It must also prove byte-identical provider-facing Beryl tool definitions across start, fork, and
restart/resume; that registry construction and projection binding expose no mutation-authority
shortcut; deterministic loaded-projection and subscription bounds; exact recovered-lease
invalidation; and absence of silently ignored dynamic-tool requests. Exact wrong-scope resolution
handler rejection remains part of the Checkpoint 5 branch-handoff verification. Thousands of
sequential acquire/release cycles must leave no registry
entries; source and fork-child leases must coexist on one connection; a second connection must not
use the first connection's recovered proof; release and every unsubscribe outcome must revoke
locally before network completion; and recovered lease loss must publish stale retirement before a
new injected target can be established.
Unclassified resume/fork errors must preserve the binding through bounded retry, return one exact
decision capability after exhaustion, reject stale or duplicate commands, and allow recovery only
through the explicit exact recover command. Retry may not stale the source, and speculative warm-up
may not acquire GUI focus or mutate the draft.

Resumable milestone: an admitted Syndic turn can obtain one exact exclusive CAS thread with native or recovered context, but live turn routing is not yet complete.

# Phase 11: Establish Exact Non-Idempotent Delivery Outcomes (finished)

- Replace generic start/steer request failure with a normalized backend outcome that distinguishes
  exact response, proven pre-dispatch failure, and remote completion unknown after possible
  dispatch. Human-readable text, silence, and retry count supply no classification authority.
- Add terminal accepted-input delivery-unknown state, exact delivery-attempt transitions, and
  projection-loss abandonment that reroutes only undispatched work while retaining ambiguous
  input in permanent accepted-input history without automatic replay or fabricated provider items.
- Preserve submitted-turn input and captured output while allowing a proven-dead execution session
  to retire its projection and converge local capture to incomplete rather than an indefinite
  unknown-terminal lock.
- Completion-review remediation: remove standalone delivery-unknown terminalization while a
  projection remains active. Any possibly dispatched steering result must converge through the
  same atomic active-binding abandonment that retires the CAS thread, publishes stale authority,
  changes the gate to pending-turn, terminalizes every delivering route, and reroutes only work
  proven undispatched.

Edge cases include failure before transport write, process death after write but before response,
structured non-steerable rejection, response-identity mismatch, crash after delivery claim, mixed
undispatched and delivering steering routes, and old-or-new persistence outcomes at every mutation.

Verification must prove no ambiguous request becomes retryable, exact rejection remains
reclassifiable, permanent accepted order survives live-route removal, reopen rejects contradictory
delivery state, and no request or storage API fabricates delivery or completion. Persistence cuts
for mixed active-binding abandonment must expose only the complete old or complete new binding,
retirement, accepted-input, route, ordering, gate, counter, and byte-accounting state.

Implemented result: `beryl-backend` returns exact response, exact rejection, proven-not-dispatched,
or completion-unknown outcomes for non-idempotent start and steering requests. `syndic-storage`
revision-fences claim, proven-pre-dispatch retry, exact success, and structured rejection. Possible
dispatch ambiguity is representable only through atomic active-binding abandonment, which retires
the old CAS thread, terminalizes delivering routes as permanent delivery-unknown history, and
reroutes only admitted or retryable work.

Verification result: 49 combined focused backend/storage tests pass, the complete feature-enabled
`syndic-storage` suite and doctests pass, and scoped formatting, checks, warnings-denied lint,
warnings-denied Rustdoc, and whitespace audits are clean apart from the already-declared inactive
backend warnings. Persistence cuts prove complete old-or-new mixed-abandonment state with nonzero
byte accounting. Completion re-review found no remaining blocking or material Phase 11 issue. The
invalid standalone terminalization and premature fresh-binding proof are recorded under
`doc/failures/cas-delivery-unknown-without-atomic-retirement.md` and
`doc/failures/cas-rebind-before-local-terminal-convergence.md`.

Resumable milestone: backend and Syndic can represent every active-input delivery cut exactly, but
no app-owned live worker dispatches a model turn yet.

# Phase 12: Build The Connection-Owned Live Event Router (finished)

- Give each projection connection one bounded sole stream poller that routes normalized events by
  exact CAS thread and turn identity and publishes runtime/account facts through their separate
  process-wide path.
- Preserve notification-before-response ordering, treat quiet polls as active, reject invalid or
  cross-generation identities, and bound target queues, diagnostics, and worker ownership.

Edge cases include two active threads sharing one connection, another connection on the same
runtime, target registration races, queue overflow, late events after retirement, malformed ids,
and account events with no thread target.

Verification must prove one poller, exact target isolation, bounded retention, quiet-stream
survival, notification-before-response delivery, and connection-generation retirement.

Implementation result: every admitted foreground projection connection now owns one bounded worker
and sole stream reader. Commands receive a request-only capability, buffered events route before a
matching response is published, provisional targets bind once to an exact CAS turn, and exact
target queues retain at most 256 events and 16 MiB. Abnormal target retirement revokes the exact
loaded generation and enters a bounded 256-thread connection fence; fence exhaustion, malformed
identity, protocol failure, or stream loss retires the connection rather than forgetting authority.
Account and connection-lifecycle facts are shared by exact runtime and managed-process generation
while thread events remain connection-local.

Approval result: normalized approvals distinguish response-required, session-auto-denied, and
caller-denied state. Every clone shares one response state bound to the exact originating backend
session; state advances only after a successful write, and foreign or repeated responses reject
before transport access. Target-local buffered failure gates the matching command result with its
exact target and close reason while leaving unrelated targets and the connection live; normal
thread closure remains non-failing.

Review result: fresh completion review found and invalidated target-local success publication,
indistinguishable already-denied approvals, and reusable response-required approval clones. The
typed corrections are recorded in
`doc/failures/cas-phase12-router-publication-and-generation.md`; final independent router and
approval-authority reviews found no remaining material issue and confirmed a clean consuming-handoff
boundary for Phase 13 sequential reuse.

Verification result: all 122 all-feature/all-target `beryl-backend` and 51 focused all-feature
`beryl-app` nextest cases pass, including seven backend stream-boundary and seven app live-router
integration cases. Four backend and two app doctests, locked checks, warnings-denied Clippy and
Rustdoc, formatting, metadata, source-size, forbidden-boundary, heading, and whitespace audits pass.
The retained pre-rework app all-target test gap remains unchanged and received no adapter.

Resumable milestone: exact live events reach bounded per-target consumers without any consumer
being able to steal another thread's event.

# Phase 13: Execute Ordinary Turns And Capture Live History (wip)

- Consume one loaded projection into an exclusive active-execution capability, activate the exact
  durable binding, start the admitted turn, publish its returned CAS turn identity, and ingest the
  routed normalized stream into Syndic canonical history and projections.
- Reconcile every local persistence cut, retire ambiguous or lost authority, preserve incomplete
  turns, and permit simultaneous execution only across different Syndic threads.
- Replace caller-sampled home/domain revision dispatch with one writer-admitted typed mutation
  boundary, and stabilize orchestration reads only against records relevant to the exact thread or
  item. Unrelated cross-thread commits must neither conflict a live event nor starve a snapshot.
- Keep live capture independent of turn item count: retain only one bounded delta in memory and use
  durable CAS-item, canonical-item, and content authority for exact prefix and completion proof
  instead of accumulating per-item proof maps.
- Replace the invalid full-item terminal-snapshot assumption with the pinned status-only
  `turn/completed` fence. On one uninterrupted full-profile stream, serially admit every preceding
  item event, flush the one pending delta, and audit only already admitted durable items before
  terminal publication. Stream/subscription loss fails closed as incomplete; reconnect, resume,
  late subscription, process restart, and CAS history reads are not notification replay.
- Replace sparse generic/ignored item handling with a closed typed disposition for every pinned
  public item variant and every admitted public field after explicit ingress exclusions. Correlate
  submitted user input without duplication,
  accept the exact completion-only `SubAgentActivity` lifecycle, preserve exact typed provider data
  independently from narrative/resource presentation policy, and block history-complete publication
  with a typed incomplete reason for every unsupported, malformed, or unresolved history-relevant
  item.
- Replace duplicated inline/generic item payloads with one chunked `ProviderItemV1` stream per
  provider-created item. Closed start, delta, and authoritative completion frames retain field
  identity, order, optionality, typed structured values, lifecycle, and exact reusable content
  ranges. Frame-specific logical-text spans select each authoritative snapshot's
  transcript/projectable bytes from that same stream, so a completion may revise an earlier live
  view without copying unchanged bytes or exposing stale deltas; source/canonical metadata stores
  only bounded sealed frame references.
- Stage arbitrarily large provider frames through bounded resumable commands while published
  authority remains unchanged, then atomically publish the sealed content frontier, source event,
  canonical item/lifecycle, indexes, and projection staleness. Advance the Syndic replacement schema
  to V2 with per-family record versions; retain no V1 compatibility decoder or migration adapter.
- Carry the expected item kind through every normalized delta and reject kind or index mismatch
  before durable mutation. Keep hosted Responses image generation outside the supported producer
  contract under CAS 0.144.1. Normalize standalone `image_gen.imagegen` through its separate typed
  generated-media path, discard its base64 `result` at bounded JSON ingress, and preserve its
  completed provider lifecycle plus `savedPath`, but keep canonical resource
  finalization and history completeness behind until the later asset checkpoint can admit the
  generated bytes into Beryl-owned asset authority; a runtime path is never durable media.

Edge cases include activation before the start response, response loss, event-before-response,
partial assistant text, completion without start for an admitted variant, typed delta mismatch,
unsupported item variants, late terminal evidence, process or subscription loss, worker shutdown,
and two windows attempting the same thread operation.

Verification must prove exact target binding, release-scoped item-before-terminal source ordering,
status-only terminal handling, no notification replay after connection loss, no replay after
ambiguous start, quiet active turns, same-thread exclusion, cross-thread concurrency under sustained
  unrelated commits, bounded streaming commits, typed delta-kind validation, exhaustive pinned-item
  dispatch and full-field round trip/reopen equality, no-duplication user-input correlation,
  completion-only item admission, structured-value and large-field chunking beyond individual record
  bounds, exact old-or-new publication across staging cuts, bounded ingress removal of standalone
  generated-media base64 without retained JSON/string copies, typed pending-resource gating through
  `savedPath` without path authority or provider-lifecycle rewrite, hosted-image producer
  non-admission, submitted-user image correlation without provider-byte duplication, fail-closed
  handling of dynamic-tool or MCP typed inline-image payloads until they have an admitted asset
  reference, no image bytes in structured Fjall values, and count-independent bounded live-capture
  state across many active and completed items.

Investigation result: exact pinned-source review proves healthy same-thread FIFO ordering before the
status-only terminal fence for normally finishing ordinary turns and proves that CAS has no
reconnect/resume notification replay. Forced-abort ordering remains assigned to the later stop and
interruption phase. A guarded installed-runtime probe passed 21 of 21 assertions and captured no
native hosted
`image_generation` declaration even with image capability and an image-capable model. Evidence is
retained under
`doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/`
and the deterministic probe is
`doc/rework/beryl-home/probes/cas-phase13-image-generation-live.ps1`.

Implementation checkpoint: `beryl-backend` now exposes the modular closed item, delta, stream, wire,
control, metadata, and approval target boundary; its focused 52-case and full 122-case nextest suites
pass. Final static checks, two request-only assertions, and completion review remain. The
`syndic-storage` production library compiles without warnings, its typed item mutation replaces the
removed text-only module, and its 25 production-feature tests pass; all-feature fixtures and final
verification remain. App capture has not yet crossed to the replacement APIs, avoiding an interim
compatibility bridge.

Architecture correction accepted by the Operator: exact provider or local execution terminal
outcome and captured-history completeness are independent durable facts. Turn-ending status retains
the exact outcome plus an optional typed history-incomplete reason, requires a reason for a locally
`Incomplete` outcome, permits exact provider `Complete` with a reason, and keeps history
summary/finalization behind without rewriting provider lifecycle. Codecs, terminal admission,
recovery validation, and app terminal handling enforce that cross-field contract.

Backend completion-review result: a known item/history notification with absent `params` was
classified as unsupported and ignored before method dispatch, allowing malformed history to vanish
before a later terminal fence. Correct the normalized wire boundary to fail closed for missing
parameters on every known method and add focused regression coverage. The review's warnings-denied
finding is the already-declared inactive-backend `dead_code` cutover gap; Phase 13 retains the scoped
`-A dead-code` lint allowance and does not activate unrelated runtime code merely to silence it.

Projection correction accepted by the Operator: exact provider correlation advances the submitted
user item's canonical revision and must atomically stale its visible item projection and selected
transcript authority. Bounded convergence rebuilds against the latest canonical revision before
finalization; consecutive correlation/lifecycle advances may supersede intermediate work, and an
unchanged content reference reuses the prior stable projection/resource identities and parser
checkpoint instead of reparsing text. Finalization remains strict and never special-cases a
mismatched canonical source revision.

Cross-checkpoint presentation decision accepted by the Operator: durable delta coalescing in this
phase does not define visible streaming cadence. The later transcript-shell checkpoint consumes
the exact routed normalized text deltas as a bounded transient live suffix, publishes every delta
available to the next GUI frame without synthetic character pacing, and hands matching bytes to
Syndic projection only through exact frontier reconciliation.

Syndic correlation implementation result: the live-item mutation invalidates item
projection/build and selected transcript authority for both submitted-user start and completion
correlation revisions. Reopen replay narrowly validates historical user generations only against
the exact unchanged content and corresponding prior correlation frontier; it does not generally
accept stale user projections. The focused Phase 7 fixture proves both stale transitions, strict
pre-rebuild finalization refusal, unchanged stable projection/resource/checkpoint/digest reuse,
latest-revision rebuild, finalization, validation, and close/reopen.

Storage verification checkpoint: focused `phase7_transcript_construction` passes 3 of 3, the
default-feature package suite passes 25 of 25, and the all-feature suite passes 151 of 151. The four
initial fixture discrepancies and one further Phase 9 helper-selection subcase were stale fixture
identity/frontier assumptions exposed by now-retained exact user correlation; each was narrowed to
its intended valid baseline without relaxing production mutation, replay, reservation, or
corruption validators. Locked all-feature/all-target check and formatting pass.

App capture implementation result: submitted `UserMessage` correlation now validates the exact
ownerless sealed canonical user record rather than the provider-created-item composite read;
provider-created items retain that stricter generic path. Status-only terminal audit preserves the
exact provider outcome while independently carrying typed history-incomplete reasons, validates
completion kind/source/lifecycle/disposition without rewriting admitted data, and leaves generated
media `PendingAsset` behind only at resource finalization and history completeness. Scripted
pre-response fixtures now include the required pinned `turn/started` ordering fact instead of
depending on response-buffer timing. The invalid generic user path is recorded in
`doc/failures/cas-phase13-submitted-user-generic-capture.md`.

App verification checkpoint: the ordinary-turn suite passes 15 of 15 twice consecutively and the
fault suite passes 12 of 12. Locked library, normal-target, and fault-target checks, warnings-denied
scoped Clippy, warnings-denied Rustdoc, formatting, forbidden-pattern, source-size, and whitespace
audits pass. Only the already-declared inactive/dependency dead-code warnings remain outside the
scoped lint boundary.

Completion-review blocker: the first closed-disposition implementation retained several public CAS
operational variants only as an untyped activity marker or empty payload. That can publish a turn as
history-complete after discarding provider-supplied command, change, tool, collaboration, media,
review, compaction, search, or sleep fields required by the pinned public-item contract. Correct the
Syndic canonical schema and app descriptor boundary with a closed typed payload for every affected
variant, route arbitrarily large public text through chunked canonical content, preserve exact
provider lifecycle and media provenance, and reject malformed required fields without adding raw
JSON, generic blobs, or an ignored-field escape hatch. Add per-variant round-trip, reopen,
history-completeness, and terminal-audit coverage, then repeat the full backend, storage, app, static,
and independent completion review gates.

Accepted correction design: one item-owned `ProviderItemV1` stream is the sole byte authority for a
provider-created admitted item. Immutable typed start, delta, and completion frames preserve every
admitted pinned field; frame-specific logical-text span indexes expose the current narrative without
copying large bytes and let completion replace a delta-derived view exactly. The upstream standalone
image-generation base64 `result` is deliberately discarded before normalization and is not an
admitted field; its typed frame retains the exact non-binary metadata and `savedPath` only.
Bounded source and canonical records retain sealed frame references, and completion publication is
atomic only after bounded staging has produced a structurally complete final frame. Submitted-user
correlation refers to already sealed composer content. MCP and dynamic-tool values use a closed
recursive value algebra. The Syndic domain advances cleanly to schema V2 with per-family record
versions and no pre-target V1 compatibility path. The invalidated implementation and correction are
recorded in `doc/failures/cas-phase13-activity-only-public-item-loss.md`.

Correction implementation order: first restore the bounded incoming JSON exclusion for standalone
image-generation `result`, remove that field from the normalized backend item, and prove that only
`savedPath` plus non-binary lifecycle metadata crosses the backend boundary. Then add the pure typed
provider values, frame grammar, streaming encoder/validator, per-family codec versions, and
round-trip/corruption tests. Then add
bounded frame staging, atomic publication, source/canonical replay, reads, finalization, reopen, and
fault proofs in `syndic-storage`. After that replace the app descriptor/capture boundary and terminal
audit, update all affected fixtures, prove every backend variant end to end, and repeat independent
completion review.

Backend exact-field proof checkpoint: six backend-only suites now cover all 18 pinned item variants,
nested branches, completion-only subagent activity, optionality, malformed wire shapes, large
fields, and every admitted delta. The proof also established the pinned enum-variant wire spellings
`text_elements` and `move_path`.

Operator resolution: Beryl intentionally depends on CAS `savedPath` for standalone generated output
and discards the base64 `result` at incoming JSON/WebSocket ingress. The normalized item and Syndic
schema must never contain that field, and missing or unreadable `savedPath` is a typed unavailable
resource with no inline fallback. The prior proposal to persist or sidecar-admit the base64 was
invalidated; Phase 13 may resume against the corrected ingress contract. Pinned-source evidence is
retained in
`doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/image-generation-result-payload.md`.

Backend downstream-exclusion checkpoint: one structural decoder removes exactly the standalone
`imageGeneration` `result` field before retained JSON-RPC values, leaves ordinary responses and
unrelated nested result fields untouched, redacts invalid-message diagnostics, and fails closed on
ambiguous image shapes. The normalized item has no result member, and item start/completion events
retain their required typed nonnegative lifecycle timestamps. The focused 75-case gate, four
dedicated ingress cases, and the full 160-case all-feature backend nextest suite pass; locked package
check, formatting, source-size, whitespace, and retained-result audits pass apart from the declared
22 inactive-backend dead-code warnings.

Operator memory correction: that checkpoint still assembled one complete raw WebSocket message
before structural exclusion. Even without a decoded or downstream copy, retaining the full base64
transport payload until message completion violates the established drop-as-soon-as-possible
behavior. Replace it with incremental JSON consumption over bounded WebSocket payload chunks so the
discarded `result` occupies only fixed transport/parser buffers and never a message-sized raw
allocation. The framing layer remains schema-agnostic, and exact non-target message semantics and
field-order handling must stay fail-closed.

Pinned-order proof: source inspection and a literal installed-runtime capture establish that
official CAS 0.144.1 emits notification `method`, lifecycle `item`, and internally tagged item
`type` before the standalone image payload. The incremental decoder may pin that producer order and
must fail closed on a reordered, duplicate, or ambiguous target shape; it must not add whole-field
buffering, spooling, decoding, or guessing. The retained proof is
`doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/image-generation-wire-order.md`.

Bounded-ingress implementation result: the existing schema-agnostic `soketto` framing remains in
place, while a `WebSocketPayloadReader` now feeds JSON parsing directly from payload chunks through
fixed 8 KiB transport and parser buffers. Once the pinned discriminants prove standalone image
generation, its base64 `result` is consumed and discarded incrementally without constructing,
decoding, spooling, or retaining it. Fragmentation and interleaved control frames remain exact;
five focused ingress cases and all 161 all-feature backend cases pass. A future compatible CAS
path-only contract should replace this containment filter rather than perpetuate it.

Provider-codec checkpoint: the pure closed grammar now covers all 18 pinned item variants and nine
deltas, closed structured values, exact start/delta/completion frames, frame-local text spans,
constant-resident validation of arbitrarily large frames, and a deliberately capped 64 KiB
materializing helper. Independent review found and invalidated four semantic gaps: unknown
Web-search history support was lost, typed image locator validation was too weak, MCP image metadata
depth differed between paths, and standalone image status was not lifecycle-checked. The corrected
codec carries monotonic typed history support, uses one full/streaming non-data absolute-URI rule,
counts metadata depth identically, and closes image status to the pinned producer values. Focused
tests pass 14 of 14 and the full all-feature storage suite passes 165 of 165. The Operator-requested
safe stop occurred before the post-remediation independent re-review; that re-review remains the
first resumption gate. No provider-frame staging or publication code has begun.

Resumable milestone: ordinary submitted turns execute and most canonical history paths are durable,
and Phase 13 is implementing exact typed preservation for every pinned operational/activity item.

# Phase 14: Deliver Steering And Ordered Next-Turn Work (pending)

- Claim and deliver accepted steering fragments in permanent order, publish exact success, move
  structured non-steerable rejection to the next-turn queue, and terminalize ambiguous dispatch as
  delivery-unknown.
- Project delivered and delivery-unknown accepted fragments into transcript and recovery history
  from their retained accepted-input authority without fabricating provider-sourced canonical items.
- Atomically promote eligible next-turn fragments into one ordinary submitted turn while
  preserving separate user blocks, accepted identities, marker ownership, draft continuity, and
  bounded queue accounting.

Edge cases include input accepted before CAS turn-id publication, worker-capacity exhaustion,
multiple queued fragments, identical text, mixed images, current-turn terminal races, and crash at
claim, response, promotion, and publication boundaries.

Verification must prove accepted order, separate fragments, identity preservation, no duplicate
delivery, exact queue draining, bounded work, and coherent reopen after every persistence cut.

Resumable milestone: active steering and deferred user input execute without loss, merge, or replay.

# Phase 15: Gate Stop And Context Compaction (pending)

- Implement exact same-thread stop gates and interruption, rerouting only undispatched input while
  preserving ambiguous delivery outcomes.
- Implement context compaction as a provider operation with queue-only admission, post-request
  target activity plus later idle completion proof, and no fabricated interruptible CAS turn id.

Edge cases include stop before CAS turn-id publication, stop during steering, process loss during
stop, stale idle before compaction activity, input during compaction, compaction rejection, and
connection retirement before completion.

Verification must prove exact stop targets, no cross-thread interruption, activity-before-idle
compaction completion, queue preservation, and same-thread exclusion throughout both operations.

Resumable milestone: all foreground same-thread execution controls operate through durable exact gates.

# Phase 16: Recover Restart State And Integrate Checkpoint 3 (pending)

- Recover admitted-but-undelivered turns, pending and queued input, active records without terminal proof, stale or lost bindings, uncommitted event suffixes, and valid native sessions from durable identities without starting a competing same-thread turn.
- Integrate exact Syndic history summaries with the existing Beryl catalog source revisions while keeping turns, drafts, transcript bodies, and CAS metadata out of compact catalog rows.
- Reconcile package API docs, target system/package docs, tracker state, and every intentional Checkpoint 4 through Checkpoint 7 gap without mounting a shell placeholder.

Edge cases include crash at every local/remote proof boundary, restart with an unavailable runtime or root, recovered-injection session loss, delivery-unknown input, unknown-terminal versus proven-dead incomplete active work, stale catalog source revisions, corrupt projections, and shared historical tails across diverged threads.

Verification must run full normal/all-feature package suites, subprocess crash matrices, recovery and concurrency tests, locked checks, warnings-denied lint/docs, dependency/public-boundary/source-size/obsolete-source scans, exact expected process-entry failure, and proofs that browsing is entirely Syndic-backed.

Resumable milestone: the complete Checkpoint 3 storage and CAS-projection system survives restart and is ready for independent completion review.

# Phase 17: Independently Review Checkpoint 3 Completion (pending)

- Obtain fresh independent architectural reviews of Syndic schema authority, draft/turn lifecycle and crash recovery, canonical/projection correctness, backend protocol normalization, native-lineage precedence, one-time injection, active-turn concurrency, and rework boundaries.
- Rerun the complete Checkpoint 3 verification matrix and address every finding through a later planned remediation phase before advancing.
- Mark Checkpoint 3 complete only when no finding requires changing its storage, protocol, execution, recovery, or package architecture.

Verification must retain Fjall issue #304 as the sole accepted lower-layer dependency gap, preserve all later GUI, branch-handoff, asset, cleanup, semantic-search, and theme gaps, and prove no compatibility reader, historical CAS import, repeated replay, or placeholder path exists.

Resumable milestone: Checkpoint 3 is independently verified complete; the durable plan may then be refreshed from Checkpoint 4.
