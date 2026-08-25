# Scope

Resume Checkpoint 4 of the Beryl-home architectural rework tracked by
`doc/rework/beryl-home/REWORK.md` under the simplified authority now recorded in the feature,
system, package, and root designs.

The Operator GUI gate is approved. The immediate work first adds app-neutral composite-position
inline objects to the owned GPUI streaming-layout boundary, then completes source-zero-width
inline-object editing in the external range-backed `gpui-text-input` widget, persistent copy-on-
write composer draft storage, and bounded exact-root ComposerV1 materialization in
`syndic-storage` before Beryl mounts any large editor or other new GUI surface.
Deferred terminal repair, outage capture, and completely fresh
running-session same-home recovery remain explicitly unavailable until Checkpoint 5; no
retained-service compatibility path may substitute.

Hard resource rules apply to named Beryl-owned queues, pages, caches, workers, editor and transcript
windows, image decoders, CPU surfaces, and GPU textures. The plan does not require a universal
resource governor, exact allocator accounting, CAS-process memory accounting, GPU-driver residency,
or a global RSS theorem.

Canonical Cargo dependency graphs use each repository's ordinary manifest and tracked lockfile.
Local sibling-fork testing is an explicit opt-in through ignored `.cargo/local.toml` configuration
and an ignored alternate lockfile, so it cannot rewrite or masquerade as canonical exact-git
resolution.

Before GUI implementation begins, stop and obtain the existing Operator gate. Functional checks run
normally. Before sustained stress, benchmarks, or performance measurement, stop and coordinate with
the Operator so the laptop can remain connected to AC power. Use one independent completion review
per phase; add another review only after a concrete high-risk finding. Time-box delegated audits and
keep one implementation worker per disjoint ownership boundary.

# Phase 173: Coordinate Autosave, Flush Barriers, And Lifecycle Release (finished)

The composer host now owns one generation-qualified autosave scheduler and one joined lifecycle
flush barrier with constant-size process-local custody. Disposing barriers freeze new edit and
history admission while draining already-admitted work; publication, ambiguous reconciliation,
marker noncommit, cancellation, stale callbacks, supersession, and service disposal converge through
typed lifecycle cuts without retaining waiter collections, payloads, marker sets, inverse edits,
root graphs, or history-sized RAM. Exact-clean disposal releases every session, publication, marker-
flight, timer, barrier, pending operation, and last-outcome identity.

Locked verification passed 28 Phase 173 cases, 10 Phase 166 ordinary-stack cases, 43 Phase 171/172
regressions, 34 affected Phase 141/143 cases, and the lower HomeStore ExactOld reconciliation proof.
Package checks, formatting, scoped hygiene, and the fresh independent adversarial review passed with
no findings.

# Phase 174: Reconcile Authorized Durable Successors (finished)

HomeStore now owns one optional statically typed successor protocol inside its existing targeted-
reconciliation descriptor, registry, snapshot, worker, and single-flight lifecycle. One source and
zero or more real derived-read witnesses authenticate a fixed inline correlation; complete
pre-writer charging covers resolver state, four simultaneous correlation representations, typed
read peaks, collision facts, and existing custody. Ordinary unanimous sides retain precedence,
ineligible mixed sides fail before hooks, typed equality controls agreement, invalid derived proof
material seals collision before current reads, and `ExactSuccessor` atomically vacates and releases
the complete scope. No proof, acknowledgement, reset, release, scan, or app-reread API was added.

Locked verification passed 21 focused ordinary and successor reconciliation cases plus the all-
targets fault-feature package check, package formatting, and scoped hygiene. The fresh independent
adversarial review closed its memory-accounting, eligibility, equality, derived-read, witness-shape,
and module-size findings and accepted the phase with no remaining finding. A broader package run
separately exposed one unrelated pre-existing theme-watcher coalescing failure after all successor
cases passed; it did not affect this acceptance boundary.

# Phase 175: Materialize Exact Draft Roots For Submission (pending)

Replace whole-payload submission preparation with one production command that joins the Phase 173
submission flush, captures the exact published draft root and asset proof, streams that root through
the accepted `ComposerV1` materializer, and admits idle or accepted-next input only when the atomic
Syndic mutation still matches that exact proof. Keep materializer cursors, pages, staged records,
command custody, and reconciliation evidence explicitly bounded; neither the command nor its tests
may retain a whole draft, marker collection, inverse history, root graph, or history-sized queue.

On exact success, publish the submitted input and fresh empty draft together and release all
submission-only materialization state. Rejection or proven noncommit must preserve the exact draft
without clearing it or starting model work; indeterminate outcomes must reconcile to exact success,
exact noncommit, or terminal collision without repeat external submission. Prove that autosaves and
edits admitted after root capture cannot change submitted content, that replay is idempotent, and
that busy-thread queueing and accepted-next ordering use the same exact-root command boundary.

Restore the deferred Phase 65/68/72 default queued-input and accepted-next cases and the Phase 75
submitted-content catalog case through this production command. Remove the 37 direct ordinary
`beryl-app` whole-payload fixture call sites, excluding imports, type annotations, raw fault fixtures,
and the intentional negative-residue assertion; rename the three obsolete `beryl-home-store`
submission-footprint labels without promoting raw fault fixtures into ordinary evidence. Run
focused locked nextest coverage for the new command and affected
submission, queueing, lifecycle, catalog, materializer, and HomeStore contracts; run package checks,
formatting, scoped hygiene, and a fresh independent adversarial completion review.

Completion review corrections: preserve exact stage and custody across every fallible flush,
materializer, and pre/post-attempt acceptance edge; reconcile image-bearing accepted-next success
through a valid later promotion descendant; carry and query the same opaque turn-start free-space
requirement immediately before direct idle acceptance, preserving the exact draft for every non-
sufficient result; replace validation-only replay commands with an explicit already-accepted
outcome; and split the oversized submission state machine without changing its public boundary.
Add focused fault, restart, promotion, free-space, retry, and bounded-custody proofs for these cuts.

Repeated-review corrections: retain and explicitly reconcile every indeterminate HomeStore writer
custody for materializer and acceptance commands before any reread, retry, publication, or release;
join live submission custody into composer-service disposal and fence disposed-service tickets;
honor the caller cancellation through capture, materialization, free-space admission, and the final
HomeCommand; and require the permanent exact accepted route leaf in ExactNew reconciliation.
Prove zero retained reconciliation scopes, no post-disposal or post-cancellation send, exact release,
and route-leaf collision behavior.

Resumable milestone: custody, disposal, cancellation, and route-leaf corrections compile. The
fault-enabled exact-root submission target remains at 10 passing and 3 failing until the Phase 174
successor protocol is integrated into first acceptance and promotion; no application reread or
closed-scope acknowledgement may substitute.

# Phase 176: Provide The Live Main-Window Composer Slot (pending)

Establish the live app/shell ownership required to host `main-window.user-input-panel` without
reviving path-mounted prototypes or mounting unrelated multi-window behavior. Publish the exact
selected-thread composer host boundary and activation-pending handoff needed by the accepted GUI
contracts.

# Phase 177: Mount Conversation Composer And Editable Image Markers (pending)

Mount the canonical range-backed conversation composer and editable image-marker presentation,
activation, insertion, removal, clipboard, and bounded overlay behavior through accepted widget and
asset contracts. Use no fabricated source bytes, custom marker overlay, resident compatibility
path, or whole-value draft projection.

# Phase 178: Enforce And Accept Bounded Composer Integration At Large Scale (pending)

Connect the mounted composer to explicit nonzero retained-memory and per-frame work budgets that
are independent of viewport dimensions; enforce caret, IME, selection, interaction/scroll-anchor,
then nearby-content priority; coalesce unrealized regions into bounded filler; preserve logical
scroll extent and re-anchor on interaction; expose exact saturation states; and leave drawable-
surface rejection to the shell/renderer. Verify atomic activation, pathological viewport and rapid
scrolling, large-draft traversal and editing, logical marker scale and same-anchor ordering, exact
hit testing, bounded editor/marker/demand residency, paste and clipboard limits, autosave,
undo/redo, submission, lifecycle release, recovery, and failure/reconciliation states before
native-lineage recovery mounts restoration.

# Phase 179: Mount Native-Lineage Recovery And Editor Restoration (pending)

Mount the native-lineage recovery prompt and its accepted loading, unavailable, failure, and ready
states. Unmount the composer coherently, retain only the Beryl-owned compact restoration seed, and
validate revision, logical extent, caret, selection, scroll anchor, and target identity before
rebind. Prove stale recovery, cancellation, window close, failed rebind, and whole-value release.

# Phase 180: Build The Multi-Window Shell And Runtime Bootstrap (pending)

Mount window claims and restoration, close versus Exit, progressive bootstrap, runtime/root and
zero-runtime flows, exact activation, and the practical process window-count limit. Prove ordinary
close and Exit across idle, active, compacting, unavailable, and failure states. Running-session
same-home recovery remains explicitly unavailable rather than using retained-service authority.

# Phase 181: Mount Paged Navigation And Settings (pending)

Mount revision-bound paged catalog, search, lineage, activity, model, composer-history, and settings
sources with virtualized presentation. Mount each window's fixed-capacity navigation-history ring
directly into its Back and Forward controls without paging or virtualization. Prove stale-page
rejection, focus and popover preservation, loading and failure states, bounded row residency, and
page release.

# Phase 182: Mount Main-Window Notices (pending)

Mount the bounded notice queue, warned best-effort-home startup notice, exact soft-stop feedback,
lifecycle-yield outcomes, and accepted disabled/error states. Repair and running-session recovery
remain explicit unavailable states; mount no fabricated progress or retained-service fallback.
Prove admission, priority, preemption, replacement, dismissal, overflow, close/Exit, and stale timers.

# Phase 183: Implement Notification Audio Ownership (pending)

Add bounded notification-audio admission, encoded and decoded capacity transfer, the single
process-wide active/latest-waiting playback lane, cancellation, shutdown, and exact release. Prove
replacement, decode and device failure, settings change, close/Exit, and fail-closed runtime behavior
without changing turn semantics.

# Phase 184: Mount Transcript Presentation (pending)

Move the transcript prototype onto immutable shared pages without deep snapshot clones, then mount
realized-frame rendering, semantic anchors, selection, nested widgets, resource demand, explicit
repair-required provenance, and local page/cache release proofs. Prove atomic authority selection
without whole-turn residency and release superseded resident state after handoff. Deferred repair
dispatch remains unmounted.

# Phase 185: Verify And Close The Useful GUI Checkpoint (pending)

Verify multi-window lifecycle, activation, restoration, large drafts, logical marker scale, paged
navigation and settings, notices and audio, long transcript traversal, local cache/page release,
configured working-set behavior, and explicit fail-closed repair/recovery states. Reconcile GUI,
feature, system, package, and dependency authority, then close one fresh independent review before
Checkpoint 5.

# Phase 186: Prove The Exact 0.146.0 Terminal-Repair Surface (pending)

Inspect exact commit-scoped 0.146.0 processor, reducer, and generated-schema evidence for
`thread/turns/list`, both item-list spellings, the complete item union, history identity synthesis,
terminal-status materialization, cursor semantics, and generated-image fields. Update the focused
commit-scoped memory note with reproducible sources. Proceed only if the one descending
`thread/turns/list` request with `limit=1` and `itemsView=full` proves the latest correlated terminal
turn and complete semantic item view under the no-successor gate; otherwise stop and revise
authority without implementing a fallback.

# Phase 187: Implement The Private Terminal-Repair Adapter (pending)

Implement the private release-pinned adapter proven by Phase 186: one no-successor-gated
`thread/turns/list` request, one matching terminal turn, and one bounded semantic-final-item stream
with exact provenance and historical user-input correlation. Prove complete item coverage,
backpressure, typed incomplete outcomes, cursor discard without traversal, and no adjacent-turn,
item-history, whole-thread, general-history, or fabricated-live-event path.

# Phase 188: Add Snapshot-Specific Syndic Repair Storage (pending)

Add snapshot-specific paged Syndic records, codecs, reads, and one atomic repair mutation. It
validates exact CAS/Syndic correlation, terminal outcome, complete ordered item identities and
fields, digests, adapter/release provenance, and finalized media admission; then replaces the whole
canonical selection with the sealed snapshot and enters `FinalizingHistory`. It does not rebuild or
publish transcript projections inside that atomic selection. It never requires live-prefix equality
and never splices live, buffered, GUI, or partial snapshot content. Prove bounded paging/codecs,
identity and digest rejection, scoped reconciliation, zero partial publication, reopen behavior,
and ordinary Syndic-only reads; image-bearing snapshots remain inadmissible until Checkpoint 7
admits authenticated `savedPath` bytes.

# Phase 189: Enforce Repair-Required Successor Gates (pending)

Keep same-thread successor, fork, replacement, rollback, and compaction gated from the first exact
repair-required transition until coherent repaired or explicitly incomplete finalization releases
the gate. Other threads and structurally healthy unrelated work remain independent. Prove every
gated command family, concurrent joins, restart persistence, unrelated progress, and rejection of
live-prefix, buffered-content, or GUI-state bypasses before any repair dispatch can be mounted.

# Phase 190: Claim Terminal Repair Durably (pending)

Implement the durable target-scoped Syndic repair-request claim and derive the only private backend
capability from its consumed disposition. A consumed but unsettled claim survives process loss as
terminal incomplete authority and can never authorize a second repair request. Keep runtime claim
consumption and backend dispatch unmounted until Phase 200 installs cross-domain repair-media
admission; every repair-required target remains gated with its claim unconsumed. Prove the unmounted
boundary, concurrent admission, every dispatch crash cut, backend refusal, store loss, restart
recovery, unrelated-thread progress, and permanent rejection of duplicate dispatch.

# Phase 191: Integrate Atomic Terminal-Turn Repair (pending)

Connect the private backend sink to snapshot staging and the one atomic Syndic replacement behind
the unmounted runtime boundary. Once later mounted, a repair-required target consumes the
no-successor proof and durable Phase 190 claim, then converges to exactly repaired or explicitly
incomplete. Both dispositions enter
`FinalizingHistory`; repaired selects the complete snapshot while incomplete selects no replacement.
Bounded durable work reaches a fixed point, publishes one coherent transcript generation, and only
then atomically releases the gate. Prove request/store-loss convergence, bounded whole-turn rebuild,
generation-atomic presentation, same-thread exclusion, unrelated progress, and exactly-once
repaired-or-incomplete release.

# Phase 192: Add Bounded Outage Capture (pending)

Add the fixed-capacity prioritized outage buffer for already active exact targets. Identity,
terminal outcome, final answer, narrative, user correlation, and generated-media handoff metadata
precede operational content. Any rejected, evicted, partial, or unrepresentable canonical fact marks
the whole turn repair-required; buffered content is transient presentation only. Prove priority and
hard limits, complete versus dropped capture, loss behavior, repair classification, and no replay.

# Phase 193: Publish Entirely Fresh Same-Home Recovery (pending)

Rebuild running-session recovery as: fence new durable commands; close and dispose the failed
service; recover the same home into a newer healthy generation with fresh writer and handles;
construct a fresh backend/app service and connections; converge durable pending, stop, compaction,
and repair obligations behind the startup fence; attach the supervisor; publish atomically; then
reacquire CAS projections from durable binding authority. No old connection, broker, router,
projection, loaded session, lease, candidate, scheduler, or worker crosses the boundary. Prove the
ordered fence, complete disposal, zero old-authority reuse, durable obligation convergence, atomic
publication, post-publication reacquisition, and failure before publication.

# Phase 194: Verify And Close The Repair And Recovery Checkpoint (pending)

Run the complete functional storage, protocol, concurrency, restart, configured-limit, static-
boundary, and source-residue gates. Factor proofs by Fjall, home-store, domain, sidecar, app, backend,
and CAS-repair ownership, with only representative end-to-end compositions. Verify named Beryl-owned
queues, pages, caches, pools, and workers release or evict after repetition; treat RSS and renderer
counters as observational evidence only.
Reconcile API docs, memory/failure notes, and the tracker, then close a fresh independent review
before Checkpoint 6. Sustained stress still requires the Operator's AC-power gate and proves
configured correctness bounds rather than performance targets. Verify the declared runtime repair
gap remains fail-closed: no target dispatches and no durable claim is consumed before Phase 200.

# Phase 195: Implement Branch Discussion Creation (pending)

Implement immutable branch selection provenance, readonly context, durable child conversation
creation, first submission, ordinary child conversation, inherited image-label authority, and exact
branch-local label allocation. Prove creation failure leaves no runnable child or premature CAS work.

# Phase 196: Implement Resolution Handoff (pending)

Implement resolution admission, queued-input deferral, durable parent handoff, busy-parent ordering,
restart recovery, idempotency, retry, successful archive, and post-archive navigation outcomes.
Prove no lost input, duplicate handoff, early archive, or parent-order violation.

# Phase 197: Verify And Close Checkpoint 6 (pending)

Verify child creation, inherited label authority, first submission, resolution ordering, restart,
retry, archive, navigation, and failure states. Reconcile feature, system, package, GUI, and storage
authority, then close a fresh independent review before Checkpoint 7.

# Phase 198: Implement Asset Admission And Durable Ownership (pending)

Implement Beryl-home image admission for paste and generated output, content-addressed sidecars,
byte-for-byte collision verification, labels, references, Host/WSL projection, and generated-output
ownership. Prove arbitrary-size streaming, cancellation, collision, and durable reopen behavior.

# Phase 199: Implement Media Rendition Resources (pending)

Implement bounded file reads, header parsing, on-demand thumbnail and tile decode workers, CPU
surfaces, upload staging, shared media identity, GPU residency, eviction, and device-loss recovery.
Prove adversarial dimensions, concurrent windows, cancellation, and exact capacity release.

# Phase 200: Implement Generated-Image Repair Media (pending)

Authenticate `savedPath`, stream admitted bytes into inert sidecar and asset staging, then publish
the repaired Syndic snapshot and Beryl asset metadata through one atomic cross-domain cut. Missing,
unusable, incomplete, or failed media remains inert and finalizes the owning turn incomplete; never
retain inline base64. After this admission path is installed, mount the Phase 190 claim-consumption
and Phase 191 repair-dispatch path for all eligible targets. Prove crash cuts, collision, orphan
invisibility, coherent publication, and no pre-admission claim consumption.

# Phase 201: Mount Maintenance Presentation And Deferred Cleanup (pending)

Mount generated-title maintenance and successful branch-archive presentation through established
Syndic authority and bounded Beryl projections. Preserve unreachable turns and resources until a
separately designed future garbage-collection operation; add no graph-dependent semantic search.

# Phase 202: Verify And Close Checkpoint 7 (pending)

Verify arbitrary asset size, adversarial dimensions, concurrent windows, cancellation, collisions,
generated-image repair, atomic cross-domain publication, device loss, cache eviction, and deferred
cleanup boundaries. Reconcile authority and close a fresh independent review before Checkpoint 8.

# Phase 203: Reconcile Final Live Authority (pending)

Reconcile root, feature, system, package, GUI, settings, hotkey, diagnostics, source, dependency,
memory, failure, plan, and tracker authority against the implemented target state.

# Phase 204: Remove Obsolete Surfaces (pending)

Remove every remaining shim, obsolete export, test, key, diagnostic, role, archived-source
membership edge, and forbidden API reference. Prove the live source graph exposes only target-state
authority and no compatibility path.

# Phase 205: Verify Named Resource Boundaries (pending)

Verify every named Beryl-owned queue, cache, pool, page set, editor window, transcript window, media
decoder, CPU surface cache, GPU texture cache, and worker set obeys its configured limit and releases
or evicts after repetition. Treat RSS and renderer counters as observational diagnostics only.

# Phase 206: Run End-To-End Functional Verification (pending)

Run storage, runtime/CAS, multi-window, conversation, branch, asset, recovery, and Windows functional
verification. Coordinate with the Operator before sustained stress or performance measurement so
the laptop can remain on AC power.

# Phase 207: Close The Rework (pending)

Obtain fresh independent architectural completion review, close every finding, compact and archive
the rework tracker under the project convention, and leave no unresolved target-state authority.
