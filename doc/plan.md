# Scope

Resume Checkpoint 3 of the Beryl-home architectural rework tracked by
`doc/rework/beryl-home/REWORK.md` under the simplified authority now recorded in the feature,
system, package, and root designs.

The immediate work is a clean replacement, not a repair of the former retained-service adoption
architecture. Remove runtime capability probes, hard-stop and coarse-cleanup surfaces,
stable-core/service-epoch transfer, quarantine promotion, candidate reauthentication, and their
tests. Preserve exact release admission, normal exact live capture, exact soft stop, context
compaction, durable admitted work, and the reusable pieces needed to publish a completely fresh
service after same-home recovery.

Then preserve Fjall's exact mutation outcome through Beryl, replace global ambiguity recovery with
targeted natural-record reconciliation, narrow the filesystem threat model, add one free-space
query per new-turn start attempt, and implement bounded whole-turn CAS repair with explicit Syndic
provenance. CAS remains execution authority; Syndic remains the ordinary canonical/read authority.

Hard resource rules apply to named Beryl-owned queues, pages, caches, workers, editor and transcript
windows, image decoders, CPU surfaces, and GPU textures. The plan does not require a universal
resource governor, exact allocator accounting, CAS-process memory accounting, GPU-driver residency,
or a global RSS theorem.

Before GUI implementation begins, stop and obtain the existing Operator gate. Functional checks run
normally. Before sustained stress, benchmarks, or performance measurement, stop and coordinate with
the Operator so the laptop can remain connected to AC power.

# Phase 99: Delete Retained-Service Adoption (finished)

Replaced adoption-oriented persistent-failure close with terminal authority-free disposal evidence
and deleted service/connection adoption, epoch transfer, quarantine and candidate promotion,
replacement publication, recovered-signal authority, and their obsolete tests. Failed services now
join every old worker, shut down the provider, and settle the supervisor as unavailable without
publishing a replacement; nondispatch and possible-dispatch outcomes retain their exact boundaries.

The feature-enabled app library check, all 5 terminal-boundary tests, formatting, diff hygiene,
implementation-residue checks, process cleanup, and a fresh independent completion review passed.
Running-session same-home recovery remains deliberately unavailable until Phase 112.

# Phase 100: Preserve Exact Mutation Outcomes (wip)

Carry Fjall `NotCommitted`, `Committed`, and `Indeterminate` through `beryl-home-store` without
erasing them into generic health severity, preserving exact committed receipts and using an opaque
operation reconciliation descriptor only for indeterminate outcomes. Representative package and
domain proofs cover translation, cancellation cuts, and exact error retention without duplicating
Fjall's complete persistence-cut matrix.

Replace the mutation `Result` boundary with the exact three-variant command outcome, reserve one of
the 1,024 operation slots plus its conservative descriptor-byte budget before writer admission,
materialize its exact old/new record and intended-receipt facts under admission before any Fjall
mutation, release the reservation for direct outcomes, transfer it only with `Indeterminate`, and
adapt every workspace caller without a lossy compatibility helper.
Keep the descriptor opaque. For `Indeterminate`, the immediate recipient must synchronously and
infallibly consume its move-only custody value into the already-reserved per-home registry gate
before acknowledgement, cancellation observation, operation-state release, or service disposal.
Custody installation alone authorizes no reread, retry, rollback, publication, hook, worker, or
reconciliation execution; those remain Phase 101 work. Verify pre-admission cancellation as
`NotCommitted`, ignore later cancellation as outcome authority, retain exact Fjall class and commit
state, return an exact receipt with any genuinely post-commit failure, and attach neither a receipt
nor descriptor to the other outcome variants.

Resume milestone (2026-08-11): all 80 Beryl-home target docs passed fresh cross-authority review.
The home-store Buffer/journal proofs pass, including the Fjall `Committed` buffered write followed
by unsuccessful `SyncAll` remaining Beryl `Indeterminate` with its exact failure and custody.
`cargo check -p beryl-home-store --tests --features test-faults` and the 12-test
`writer_physical_faults` nextest target pass; syndic-storage production code reaches compilation
while its test callers remain partially converted. Resume by implementing custody-only registry
installation and the exact provider `Ingester` handoff, then finish app and Syndic caller/test
conversions, audit every production `Indeterminate` boundary, run serialized checks and nextest,
format, and obtain independent completion review. The resolved design failure is retained at
`doc/failures/beryl-home-provider-ingress-indeterminate-owner.md`.

# Phase 101: Add Bounded Targeted Reconciliation (pending)

Consume registry-owned custody through domain-owned natural-record old/new reread hooks in
`beryl-state` and `syndic-storage`, with bounded worker admission. Prove exact-old,
receipt-reconstructing exact-new, and collision outcomes, bounded joins and saturation, exact
charge release or collision replacement, unrelated-work progress, and no global scrub or
structural failure without separate evidence.

# Phase 102: Separate Routine Reopen From Exhaustive Scrub (pending)

Split structural reopen and typed-handle reacquisition from exhaustive registered-domain
validation. Explicit/background scrub, schema validation, or corruption evidence alone may invoke
the every-record path. Recovery constructs a fresh writer and service rather than clearing poison
or reusing authority from the old writer; proofs distinguish routine recovery from explicit or
corruption-triggered scrub and cover stale generations, bounded workers, and resource release.

# Phase 103: Simplify Filesystem Durability (pending)

Remove home/state/sidecar volume and file IDs, opened-object continuity, anti-replacement handles,
remote-protocol admission proofs, and unconditional UNC rejection. Keep canonical path identity,
the fixed reliable lifetime lock, initial type/reparse validation, crash-safe sidecar publication,
digest/length verification, metadata-after-bytes ordering, and the no-cleanup rule. Prove the native
local NTFS full tier, warned best-effort admission elsewhere under a reliable exclusive lock,
sidecar barriers, collision convergence, and removal of external-replacement-resistance promises.

# Phase 104: Add Per-Start Free-Space Admission (pending)

Add one synchronous, uncached free-space reserve query in `beryl-home-store`. Invoke it exactly once
immediately before each direct or queued new-turn start admission. Below-reserve, unavailable, or
indeterminate results preserve the current draft or queued input and start no CAS turn; active-turn
steering and ordinary later `ENOSPC` do not use this result as a durability promise. Prove exact
query cardinality, denial preservation, zero dispatch, and absence of polling, caching, or
hysteresis.

# Phase 105: Prove The Exact 0.146.0 Terminal-Repair Surface (pending)

Inspect exact commit-scoped 0.146.0 processor, reducer, and generated-schema evidence for
`thread/turns/list`, both item-list spellings, the complete item union, history identity synthesis,
terminal-status materialization, cursor semantics, and generated-image fields. Update the focused
commit-scoped memory note with reproducible sources. Proceed only if the one descending
`thread/turns/list` request with `limit=1` and `itemsView=full` proves the latest correlated terminal
turn and complete semantic item view under the no-successor gate; otherwise stop and revise
authority without implementing a fallback.

# Phase 106: Implement The Private Terminal-Repair Adapter (pending)

Implement the private release-pinned adapter proven by Phase 105: one no-successor-gated
`thread/turns/list` request, one matching terminal turn, and one bounded semantic-final-item stream
with exact provenance and historical user-input correlation. Prove complete item coverage,
backpressure, typed incomplete outcomes, cursor discard without traversal, and no adjacent-turn,
item-history, whole-thread, general-history, or fabricated-live-event path.

# Phase 107: Add Snapshot-Specific Syndic Repair Storage (pending)

Add snapshot-specific paged Syndic records, codecs, reads, and one atomic repair mutation. It
validates exact CAS/Syndic correlation, terminal outcome, complete ordered item identities and
fields, digests, adapter/release provenance, and finalized media admission; then replaces the whole
canonical selection with the sealed snapshot and enters `FinalizingHistory`. It does not rebuild or
publish transcript projections inside that atomic selection. It never requires live-prefix equality
and never splices live, buffered, GUI, or partial snapshot content. Prove bounded paging/codecs,
identity and digest rejection, scoped reconciliation, zero partial publication, reopen behavior,
and ordinary Syndic-only reads; image-bearing snapshots remain inadmissible until Checkpoint 6
admits authenticated `savedPath` bytes.

# Phase 108: Enforce Repair-Required Successor Gates (pending)

Keep same-thread successor, fork, replacement, rollback, and compaction gated from the first exact
repair-required transition until coherent repaired or explicitly incomplete finalization releases
the gate. Other threads and structurally healthy unrelated work remain independent. Prove every
gated command family, concurrent joins, restart persistence, unrelated progress, and rejection of
live-prefix, buffered-content, or GUI-state bypasses before any repair dispatch can be mounted.

# Phase 109: Claim Terminal Repair Durably (pending)

Implement the durable target-scoped Syndic repair-request claim and derive the only private backend
capability from its consumed disposition. A consumed but unsettled claim survives process loss as
terminal incomplete authority and can never authorize a second repair request. Keep runtime claim
consumption and backend dispatch unmounted until Phase 129 installs cross-domain repair-media
admission; every repair-required target remains gated with its claim unconsumed. Prove the unmounted
boundary, concurrent admission, every dispatch crash cut, backend refusal, store loss, restart
recovery, unrelated-thread progress, and permanent rejection of duplicate dispatch.

# Phase 110: Integrate Atomic Terminal-Turn Repair (pending)

Connect the private backend sink to snapshot staging and the one atomic Syndic replacement behind
the unmounted runtime boundary. Once later mounted, a repair-required target consumes the
no-successor proof and durable Phase 109 claim, then converges to exactly repaired or explicitly
incomplete. Both dispositions enter
`FinalizingHistory`; repaired selects the complete snapshot while incomplete selects no replacement.
Bounded durable work reaches a fixed point, publishes one coherent transcript generation, and only
then atomically releases the gate. Prove request/store-loss convergence, bounded whole-turn rebuild,
generation-atomic presentation, same-thread exclusion, unrelated progress, and exactly-once
repaired-or-incomplete release.

# Phase 111: Add Bounded Outage Capture (pending)

Add the fixed-capacity prioritized outage buffer for already active exact targets. Identity,
terminal outcome, final answer, narrative, user correlation, and generated-media handoff metadata
precede operational content. Any rejected, evicted, partial, or unrepresentable canonical fact marks
the whole turn repair-required; buffered content is transient presentation only. Prove priority and
hard limits, complete versus dropped capture, loss behavior, repair classification, and no replay.

# Phase 112: Publish Entirely Fresh Same-Home Recovery (pending)

Rebuild running-session recovery as: fence new durable commands; close and dispose the failed
service; recover the same home into a newer healthy generation with fresh writer and handles;
construct a fresh backend/app service and connections; converge durable pending, stop, compaction,
and repair obligations behind the startup fence; attach the supervisor; publish atomically; then
reacquire CAS projections from durable binding authority. No old connection, broker, router,
projection, loaded session, lease, candidate, scheduler, or worker crosses the boundary. Prove the
ordered fence, complete disposal, zero old-authority reuse, durable obligation convergence, atomic
publication, post-publication reacquisition, and failure before publication.

# Phase 113: Verify And Close Checkpoint 3 (pending)

Run the complete functional storage, protocol, concurrency, restart, configured-limit, static-
boundary, and source-residue gates. Factor proofs by Fjall, home-store, domain, sidecar, app, backend,
and CAS-repair ownership, with only representative end-to-end compositions. Verify named Beryl-owned
queues, pages, caches, pools, and workers release or evict after repetition; treat RSS and renderer
counters as observational evidence only.
Reconcile API docs, memory/failure notes, and the tracker, then close a fresh independent review
before Checkpoint 4. Sustained stress still requires the Operator's AC-power gate and proves
configured correctness bounds rather than performance targets. Verify the declared runtime repair
gap remains fail-closed: no target dispatches and no durable claim is consumed before Phase 129.

# Phase 114: Implement The Theme Runtime (pending)

Implement the `beryl-state` theme-domain repository, bounded parser/validator/resolver, exact
mutation reconciliation, immutable appearance-generation publisher, and single instance-wide
preview arbiter defined by `doc/systems/theme-runtime/design.md`. Keep active-theme identity in the
Settings Apply/OK draft while theme-document Save and Save As remain separate exact operations;
prove fallback, bounded repository access, reconciliation, atomic cross-window publication, preview
arbitration, stale-result rejection, and same-home recovery.

# Phase 115: Implement The External Range-Backed Text Input (pending)

After the Operator's GUI gate, implement and verify the missing revision, range/page source,
resident-window, compact restoration seed, stale-result, geometry, and staged edit-sink API in the
exact `gpui-text-input` dependency. Prove editor unmount and rebind retain only the bounded seed and
never a whole value. Do not create a Beryl whole-string adapter.

# Phase 116: Mount Range-Backed Draft And Marker Editing (pending)

Replace whole-payload draft activation with revision-bound text and marker ranges, bounded editor
windows, incremental autosave/undo/submission, and very-large-draft plus logical-marker-scale proofs.

# Phase 117: Mount Native-Lineage Recovery And Editor Restoration (pending)

Mount the native-lineage recovery prompt and its accepted loading, unavailable, failure, and ready
states. Unmount the composer coherently, retain only the Beryl-owned compact restoration seed, and
validate revision, logical extent, caret, selection, scroll anchor, and target identity before
rebind. Prove stale recovery, cancellation, window close, failed rebind, and whole-value release.

# Phase 118: Build The Multi-Window Shell And Runtime Bootstrap (pending)

Mount window claims and restoration, close versus Exit, progressive bootstrap, runtime/root and
zero-runtime flows, exact activation, and the practical process window-count limit. Prove ordinary
close and Exit across idle, active, compacting, unavailable, and failure states.

# Phase 119: Mount Paged Navigation And Settings (pending)

Mount revision-bound paged catalog, search, lineage, activity, model, navigation-history,
composer-history, and settings sources with virtualized presentation. Prove stale-page rejection,
focus and popover preservation, loading and failure states, bounded row residency, and page release.

# Phase 120: Mount Main-Window Notices (pending)

Mount the bounded notice queue, warned best-effort-home startup notice, repair provenance, backend
recovery, exact soft-stop feedback, lifecycle-yield outcomes, and accepted disabled/error states.
Prove admission, priority, preemption, replacement, dismissal, overflow, close/Exit, stale timers,
and recovery behavior.

# Phase 121: Implement Notification Audio Ownership (pending)

Add bounded notification-audio admission, encoded and decoded capacity transfer, the single
process-wide active/latest-waiting playback lane, cancellation, shutdown, and exact release. Prove
replacement, decode and device failure, settings change, close/Exit, and recovery behavior without
changing turn semantics.

# Phase 122: Mount Transcript Presentation (pending)

Move the transcript prototype onto immutable shared pages without deep snapshot clones, then mount
realized-frame rendering, semantic anchors, selection, nested widgets, resource demand, repair
provenance, and local page/cache release proofs. Prove atomic authority selection without whole-turn
residency and release superseded resident state after handoff.

# Phase 123: Verify And Close Checkpoint 4 (pending)

Verify multi-window lifecycle, activation, restoration, large drafts, logical marker scale, paged
navigation and settings, notices and audio, long transcript traversal, local cache/page release, and
configured working-set behavior. Reconcile GUI, feature, system, package, and dependency authority,
then close a fresh independent review before Checkpoint 5.

# Phase 124: Implement Branch Discussion Creation (pending)

Implement immutable branch selection provenance, readonly context, durable child conversation
creation, first submission, ordinary child conversation, inherited image-label authority, and exact
branch-local label allocation. Prove creation failure leaves no runnable child or premature CAS work.

# Phase 125: Implement Resolution Handoff (pending)

Implement resolution admission, queued-input deferral, durable parent handoff, busy-parent ordering,
restart recovery, idempotency, retry, successful archive, and post-archive navigation outcomes.
Prove no lost input, duplicate handoff, early archive, or parent-order violation.

# Phase 126: Verify And Close Checkpoint 5 (pending)

Verify child creation, inherited label authority, first submission, resolution ordering, restart,
retry, archive, navigation, and failure states. Reconcile feature, system, package, GUI, and storage
authority, then close a fresh independent review before Checkpoint 6.

# Phase 127: Implement Asset Admission And Durable Ownership (pending)

Implement Beryl-home image admission for paste and generated output, content-addressed sidecars,
byte-for-byte collision verification, labels, references, Host/WSL projection, and generated-output
ownership. Prove arbitrary-size streaming, cancellation, collision, and durable reopen behavior.

# Phase 128: Implement Media Rendition Resources (pending)

Implement bounded file reads, header parsing, on-demand thumbnail and tile decode workers, CPU
surfaces, upload staging, shared media identity, GPU residency, eviction, and device-loss recovery.
Prove adversarial dimensions, concurrent windows, cancellation, and exact capacity release.

# Phase 129: Implement Generated-Image Repair Media (pending)

Authenticate `savedPath`, stream admitted bytes into inert sidecar and asset staging, then publish
the repaired Syndic snapshot and Beryl asset metadata through one atomic cross-domain cut. Missing,
unusable, incomplete, or failed media remains inert and finalizes the owning turn incomplete; never
retain inline base64. After this admission path is installed, mount the Phase 109 claim-consumption
and Phase 110 repair-dispatch path for all eligible targets. Prove crash cuts, collision, orphan
invisibility, coherent publication, and no pre-admission claim consumption.

# Phase 130: Mount Maintenance Presentation And Deferred Cleanup (pending)

Mount generated-title maintenance and successful branch-archive presentation through established
Syndic authority and bounded Beryl projections. Preserve unreachable turns and resources until a
separately designed future garbage-collection operation; add no graph-dependent semantic search.

# Phase 131: Verify And Close Checkpoint 6 (pending)

Verify arbitrary asset size, adversarial dimensions, concurrent windows, cancellation, collisions,
generated-image repair, atomic cross-domain publication, device loss, cache eviction, and deferred
cleanup boundaries. Reconcile authority and close a fresh independent review before Checkpoint 7.

# Phase 132: Reconcile Final Live Authority (pending)

Reconcile root, feature, system, package, GUI, settings, hotkey, diagnostics, source, dependency,
memory, failure, plan, and tracker authority against the implemented target state.

# Phase 133: Remove Obsolete Surfaces (pending)

Remove every remaining shim, obsolete export, test, key, diagnostic, role, archived-source
membership edge, and forbidden API reference. Prove the live source graph exposes only target-state
authority and no compatibility path.

# Phase 134: Verify Named Resource Boundaries (pending)

Verify every named Beryl-owned queue, cache, pool, page set, editor window, transcript window, media
decoder, CPU surface cache, GPU texture cache, and worker set obeys its configured limit and releases
or evicts after repetition. Treat RSS and renderer counters as observational diagnostics only.

# Phase 135: Run End-To-End Functional Verification (pending)

Run storage, runtime/CAS, multi-window, conversation, branch, asset, recovery, and Windows functional
verification. Coordinate with the Operator before sustained stress or performance measurement so
the laptop can remain on AC power.

# Phase 136: Close The Rework (pending)

Obtain fresh independent architectural completion review, close every finding, compact and archive
the rework tracker under the project convention, and leave no unresolved target-state authority.
