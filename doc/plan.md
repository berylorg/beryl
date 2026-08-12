# Scope

Resume Checkpoint 4 of the Beryl-home architectural rework tracked by
`doc/rework/beryl-home/REWORK.md` under the simplified authority now recorded in the feature,
system, package, and root designs.

The immediate work implements the pre-GUI theme runtime, then stops at the existing Operator GUI
gate before mounting the external range-backed text input and subsequent shell surfaces. Deferred
terminal repair, outage capture, and completely fresh running-session same-home recovery remain
explicitly unavailable until Checkpoint 5; no retained-service compatibility path may substitute.

Hard resource rules apply to named Beryl-owned queues, pages, caches, workers, editor and transcript
windows, image decoders, CPU surfaces, and GPU textures. The plan does not require a universal
resource governor, exact allocator accounting, CAS-process memory accounting, GPU-driver residency,
or a global RSS theorem.

Before GUI implementation begins, stop and obtain the existing Operator gate. Functional checks run
normally. Before sustained stress, benchmarks, or performance measurement, stop and coordinate with
the Operator so the laptop can remain connected to AC power. Use one independent completion review
per phase; add another review only after a concrete high-risk finding. Time-box delegated audits and
keep one implementation worker per disjoint ownership boundary.

# Phase 107: Implement The Theme Runtime (finished)

Implemented the stable editable theme repository, bounded typed parser/resolver and exact retained
mutation reconciliation in `beryl-home-store` and `beryl-state`, plus the process-wide immutable
appearance publisher, coherent external hot reload, Settings-durable retry, bounded diagnostics,
and single preview arbiter in `beryl-app`. Invalid startup content selects the built-in fallback;
invalid or rejected live refresh retains the complete last coherent running snapshot. No GUI was
mounted.

Factored non-stress gates passed: home-store theme tests 13/13, full state tests 104/104, app theme
runtime tests 28/28, locked all-target checks, API docs, metadata, formatting, diff and static
removal/resource boundaries. The independent completion re-review passed with no high- or
medium-severity findings.

# Phase 108: Implement The External Range-Backed Text Input (pending)

After the Operator's GUI gate, implement and verify the missing revision, range/page source,
resident-window, compact restoration seed, stale-result, geometry, and staged edit-sink API in the
exact `gpui-text-input` dependency. Prove editor unmount and rebind retain only the bounded seed and
never a whole value. Do not create a Beryl whole-string adapter.

# Phase 109: Mount Range-Backed Draft And Marker Editing (pending)

Replace whole-payload draft activation with revision-bound text and marker ranges, bounded editor
windows, incremental autosave/undo/submission, and very-large-draft plus logical-marker-scale proofs.

# Phase 110: Mount Native-Lineage Recovery And Editor Restoration (pending)

Mount the native-lineage recovery prompt and its accepted loading, unavailable, failure, and ready
states. Unmount the composer coherently, retain only the Beryl-owned compact restoration seed, and
validate revision, logical extent, caret, selection, scroll anchor, and target identity before
rebind. Prove stale recovery, cancellation, window close, failed rebind, and whole-value release.

# Phase 111: Build The Multi-Window Shell And Runtime Bootstrap (pending)

Mount window claims and restoration, close versus Exit, progressive bootstrap, runtime/root and
zero-runtime flows, exact activation, and the practical process window-count limit. Prove ordinary
close and Exit across idle, active, compacting, unavailable, and failure states. Running-session
same-home recovery remains explicitly unavailable rather than using retained-service authority.

# Phase 112: Mount Paged Navigation And Settings (pending)

Mount revision-bound paged catalog, search, lineage, activity, model, navigation-history,
composer-history, and settings sources with virtualized presentation. Prove stale-page rejection,
focus and popover preservation, loading and failure states, bounded row residency, and page release.

# Phase 113: Mount Main-Window Notices (pending)

Mount the bounded notice queue, warned best-effort-home startup notice, exact soft-stop feedback,
lifecycle-yield outcomes, and accepted disabled/error states. Repair and running-session recovery
remain explicit unavailable states; mount no fabricated progress or retained-service fallback.
Prove admission, priority, preemption, replacement, dismissal, overflow, close/Exit, and stale timers.

# Phase 114: Implement Notification Audio Ownership (pending)

Add bounded notification-audio admission, encoded and decoded capacity transfer, the single
process-wide active/latest-waiting playback lane, cancellation, shutdown, and exact release. Prove
replacement, decode and device failure, settings change, close/Exit, and fail-closed runtime behavior
without changing turn semantics.

# Phase 115: Mount Transcript Presentation (pending)

Move the transcript prototype onto immutable shared pages without deep snapshot clones, then mount
realized-frame rendering, semantic anchors, selection, nested widgets, resource demand, explicit
repair-required provenance, and local page/cache release proofs. Prove atomic authority selection
without whole-turn residency and release superseded resident state after handoff. Deferred repair
dispatch remains unmounted.

# Phase 116: Verify And Close The Useful GUI Checkpoint (pending)

Verify multi-window lifecycle, activation, restoration, large drafts, logical marker scale, paged
navigation and settings, notices and audio, long transcript traversal, local cache/page release,
configured working-set behavior, and explicit fail-closed repair/recovery states. Reconcile GUI,
feature, system, package, and dependency authority, then close one fresh independent review before
Checkpoint 5.

# Phase 117: Prove The Exact 0.146.0 Terminal-Repair Surface (pending)

Inspect exact commit-scoped 0.146.0 processor, reducer, and generated-schema evidence for
`thread/turns/list`, both item-list spellings, the complete item union, history identity synthesis,
terminal-status materialization, cursor semantics, and generated-image fields. Update the focused
commit-scoped memory note with reproducible sources. Proceed only if the one descending
`thread/turns/list` request with `limit=1` and `itemsView=full` proves the latest correlated terminal
turn and complete semantic item view under the no-successor gate; otherwise stop and revise
authority without implementing a fallback.

# Phase 118: Implement The Private Terminal-Repair Adapter (pending)

Implement the private release-pinned adapter proven by Phase 117: one no-successor-gated
`thread/turns/list` request, one matching terminal turn, and one bounded semantic-final-item stream
with exact provenance and historical user-input correlation. Prove complete item coverage,
backpressure, typed incomplete outcomes, cursor discard without traversal, and no adjacent-turn,
item-history, whole-thread, general-history, or fabricated-live-event path.

# Phase 119: Add Snapshot-Specific Syndic Repair Storage (pending)

Add snapshot-specific paged Syndic records, codecs, reads, and one atomic repair mutation. It
validates exact CAS/Syndic correlation, terminal outcome, complete ordered item identities and
fields, digests, adapter/release provenance, and finalized media admission; then replaces the whole
canonical selection with the sealed snapshot and enters `FinalizingHistory`. It does not rebuild or
publish transcript projections inside that atomic selection. It never requires live-prefix equality
and never splices live, buffered, GUI, or partial snapshot content. Prove bounded paging/codecs,
identity and digest rejection, scoped reconciliation, zero partial publication, reopen behavior,
and ordinary Syndic-only reads; image-bearing snapshots remain inadmissible until Checkpoint 7
admits authenticated `savedPath` bytes.

# Phase 120: Enforce Repair-Required Successor Gates (pending)

Keep same-thread successor, fork, replacement, rollback, and compaction gated from the first exact
repair-required transition until coherent repaired or explicitly incomplete finalization releases
the gate. Other threads and structurally healthy unrelated work remain independent. Prove every
gated command family, concurrent joins, restart persistence, unrelated progress, and rejection of
live-prefix, buffered-content, or GUI-state bypasses before any repair dispatch can be mounted.

# Phase 121: Claim Terminal Repair Durably (pending)

Implement the durable target-scoped Syndic repair-request claim and derive the only private backend
capability from its consumed disposition. A consumed but unsettled claim survives process loss as
terminal incomplete authority and can never authorize a second repair request. Keep runtime claim
consumption and backend dispatch unmounted until Phase 131 installs cross-domain repair-media
admission; every repair-required target remains gated with its claim unconsumed. Prove the unmounted
boundary, concurrent admission, every dispatch crash cut, backend refusal, store loss, restart
recovery, unrelated-thread progress, and permanent rejection of duplicate dispatch.

# Phase 122: Integrate Atomic Terminal-Turn Repair (pending)

Connect the private backend sink to snapshot staging and the one atomic Syndic replacement behind
the unmounted runtime boundary. Once later mounted, a repair-required target consumes the
no-successor proof and durable Phase 121 claim, then converges to exactly repaired or explicitly
incomplete. Both dispositions enter
`FinalizingHistory`; repaired selects the complete snapshot while incomplete selects no replacement.
Bounded durable work reaches a fixed point, publishes one coherent transcript generation, and only
then atomically releases the gate. Prove request/store-loss convergence, bounded whole-turn rebuild,
generation-atomic presentation, same-thread exclusion, unrelated progress, and exactly-once
repaired-or-incomplete release.

# Phase 123: Add Bounded Outage Capture (pending)

Add the fixed-capacity prioritized outage buffer for already active exact targets. Identity,
terminal outcome, final answer, narrative, user correlation, and generated-media handoff metadata
precede operational content. Any rejected, evicted, partial, or unrepresentable canonical fact marks
the whole turn repair-required; buffered content is transient presentation only. Prove priority and
hard limits, complete versus dropped capture, loss behavior, repair classification, and no replay.

# Phase 124: Publish Entirely Fresh Same-Home Recovery (pending)

Rebuild running-session recovery as: fence new durable commands; close and dispose the failed
service; recover the same home into a newer healthy generation with fresh writer and handles;
construct a fresh backend/app service and connections; converge durable pending, stop, compaction,
and repair obligations behind the startup fence; attach the supervisor; publish atomically; then
reacquire CAS projections from durable binding authority. No old connection, broker, router,
projection, loaded session, lease, candidate, scheduler, or worker crosses the boundary. Prove the
ordered fence, complete disposal, zero old-authority reuse, durable obligation convergence, atomic
publication, post-publication reacquisition, and failure before publication.

# Phase 125: Verify And Close The Repair And Recovery Checkpoint (pending)

Run the complete functional storage, protocol, concurrency, restart, configured-limit, static-
boundary, and source-residue gates. Factor proofs by Fjall, home-store, domain, sidecar, app, backend,
and CAS-repair ownership, with only representative end-to-end compositions. Verify named Beryl-owned
queues, pages, caches, pools, and workers release or evict after repetition; treat RSS and renderer
counters as observational evidence only.
Reconcile API docs, memory/failure notes, and the tracker, then close a fresh independent review
before Checkpoint 6. Sustained stress still requires the Operator's AC-power gate and proves
configured correctness bounds rather than performance targets. Verify the declared runtime repair
gap remains fail-closed: no target dispatches and no durable claim is consumed before Phase 131.

# Phase 126: Implement Branch Discussion Creation (pending)

Implement immutable branch selection provenance, readonly context, durable child conversation
creation, first submission, ordinary child conversation, inherited image-label authority, and exact
branch-local label allocation. Prove creation failure leaves no runnable child or premature CAS work.

# Phase 127: Implement Resolution Handoff (pending)

Implement resolution admission, queued-input deferral, durable parent handoff, busy-parent ordering,
restart recovery, idempotency, retry, successful archive, and post-archive navigation outcomes.
Prove no lost input, duplicate handoff, early archive, or parent-order violation.

# Phase 128: Verify And Close Checkpoint 6 (pending)

Verify child creation, inherited label authority, first submission, resolution ordering, restart,
retry, archive, navigation, and failure states. Reconcile feature, system, package, GUI, and storage
authority, then close a fresh independent review before Checkpoint 7.

# Phase 129: Implement Asset Admission And Durable Ownership (pending)

Implement Beryl-home image admission for paste and generated output, content-addressed sidecars,
byte-for-byte collision verification, labels, references, Host/WSL projection, and generated-output
ownership. Prove arbitrary-size streaming, cancellation, collision, and durable reopen behavior.

# Phase 130: Implement Media Rendition Resources (pending)

Implement bounded file reads, header parsing, on-demand thumbnail and tile decode workers, CPU
surfaces, upload staging, shared media identity, GPU residency, eviction, and device-loss recovery.
Prove adversarial dimensions, concurrent windows, cancellation, and exact capacity release.

# Phase 131: Implement Generated-Image Repair Media (pending)

Authenticate `savedPath`, stream admitted bytes into inert sidecar and asset staging, then publish
the repaired Syndic snapshot and Beryl asset metadata through one atomic cross-domain cut. Missing,
unusable, incomplete, or failed media remains inert and finalizes the owning turn incomplete; never
retain inline base64. After this admission path is installed, mount the Phase 121 claim-consumption
and Phase 122 repair-dispatch path for all eligible targets. Prove crash cuts, collision, orphan
invisibility, coherent publication, and no pre-admission claim consumption.

# Phase 132: Mount Maintenance Presentation And Deferred Cleanup (pending)

Mount generated-title maintenance and successful branch-archive presentation through established
Syndic authority and bounded Beryl projections. Preserve unreachable turns and resources until a
separately designed future garbage-collection operation; add no graph-dependent semantic search.

# Phase 133: Verify And Close Checkpoint 7 (pending)

Verify arbitrary asset size, adversarial dimensions, concurrent windows, cancellation, collisions,
generated-image repair, atomic cross-domain publication, device loss, cache eviction, and deferred
cleanup boundaries. Reconcile authority and close a fresh independent review before Checkpoint 8.

# Phase 134: Reconcile Final Live Authority (pending)

Reconcile root, feature, system, package, GUI, settings, hotkey, diagnostics, source, dependency,
memory, failure, plan, and tracker authority against the implemented target state.

# Phase 135: Remove Obsolete Surfaces (pending)

Remove every remaining shim, obsolete export, test, key, diagnostic, role, archived-source
membership edge, and forbidden API reference. Prove the live source graph exposes only target-state
authority and no compatibility path.

# Phase 136: Verify Named Resource Boundaries (pending)

Verify every named Beryl-owned queue, cache, pool, page set, editor window, transcript window, media
decoder, CPU surface cache, GPU texture cache, and worker set obeys its configured limit and releases
or evicts after repetition. Treat RSS and renderer counters as observational diagnostics only.

# Phase 137: Run End-To-End Functional Verification (pending)

Run storage, runtime/CAS, multi-window, conversation, branch, asset, recovery, and Windows functional
verification. Coordinate with the Operator before sustained stress or performance measurement so
the laptop can remain on AC power.

# Phase 138: Close The Rework (pending)

Obtain fresh independent architectural completion review, close every finding, compact and archive
the rework tracker under the project convention, and leave no unresolved target-state authority.
