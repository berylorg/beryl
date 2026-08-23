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

# Phase 165: Select Historical Roots From Compact Syndic Intents (finished)

Added bounded authenticated undo/redo selection from one compact active-session intent and reused
the canonical historical-root adoption and reconciliation path without caller-cached history or
current-draft publication. All 38 locked edit-history, retention, and historical-adoption tests,
locked all-targets checking, formatting, and independent persistence/concurrency review passed.

# Phase 166: Route Beryl Host History Through Syndic Historical Roots (pending)

Add a dedicated Beryl composer-host history coordinator that translates one exact active widget
history intent into the retained Syndic transition and direct historical-root adoption. Atomically
rebind the live host only on committed settlement; preserve both frontiers on rejection, conflict,
cancellation, error, candidate drift, corrupt or missing authority, and terminal collision; retain
indeterminate custody through exact reconciliation; and drain admitted work across release or
rebind without accepting late completion. Prove sequential undo/redo, repeated branching and redo
clearing, retention eviction, restart, all five settlements, cancellation cuts, every
reconciliation classification, candidate drift, corrupt or missing roots, rebind, release, late
completion, and one atomic live-view update without inverse text, marker-registry residency, or
current-draft publication.

# Phase 167: Enforce Credit-Gated Composer Realization (pending)

Enforce configured composer retained-memory and per-frame work budgets independently of viewport
dimensions. Prioritize caret, IME, selection, interaction and scroll anchors; admit nearby pages
only while credits remain; coalesce unrealized nominally visible content into bounded filler; keep
logical scroll extent and re-anchor on interaction; expose capacity saturation; and leave
unrepresentable drawable-surface rejection to the shell/renderer. Prove pathological viewport,
rapid scrolling, focus, hit-testing, marker, release, and recovery behavior without an unbounded
demand queue.

# Phase 168: Publish Dirty Candidates And Satisfy Flush Barriers (pending)

Add exact candidate publication and disposal receipts, atomic current-draft and Asset-owner
composition, dirty autosave deadlines and settings rearm, ambiguous-outcome reconciliation,
repeated flush-to-clean barriers, and lifecycle release. Editing may advance beyond a captured save;
only the captured frontier becomes published, and close, switch, Exit, and submission barriers stay
unsatisfied until the newest eligible candidate is durably published or terminally unavailable.

# Phase 169: Materialize Exact Draft Roots For Submission (pending)

Replace whole-payload submission preparation with the exact-root ComposerV1 materializer and
atomic idle or accepted-input admission over the matching draft root and asset proof. Prove later-
autosave independence, rejection without clear, exact send-and-clear publication, replay,
collision, queueing, and submission-only materialization ownership. Restore the deferred Phase
65/68/72 default queued-input and accepted-next cases and the Phase 75 submitted-content catalog
case through this production command, remove the 37 obsolete `beryl-app` whole-payload fixture uses,
and rename the three obsolete `beryl-home-store` submission-footprint labels without promoting raw
fault fixtures into ordinary evidence or removing the intentional negative-residue assertion.

# Phase 170: Provide The Live Main-Window Composer Slot (pending)

Establish the live app/shell ownership required to host `main-window.user-input-panel` without
reviving path-mounted prototypes or mounting unrelated multi-window behavior. Publish the exact
selected-thread composer host boundary and activation-pending handoff needed by the accepted GUI
contracts.

# Phase 171: Mount Conversation Composer And Editable Image Markers (pending)

Mount the canonical range-backed conversation composer and editable image-marker presentation,
activation, insertion, removal, clipboard, and bounded overlay behavior through accepted widget and
asset contracts. Use no fabricated source bytes, custom marker overlay, resident compatibility
path, or whole-value draft projection.

# Phase 172: Accept Bounded Composer Integration At Large Scale (pending)

Verify atomic activation, large-draft traversal and editing, logical marker scale and same-anchor
ordering, bounded editor and marker residency, paste and clipboard limits, autosave, undo/redo,
submission, lifecycle release, and failure/reconciliation states. Obtain fresh independent
acceptance before native-lineage recovery mounts restoration.

# Phase 173: Mount Native-Lineage Recovery And Editor Restoration (pending)

Mount the native-lineage recovery prompt and its accepted loading, unavailable, failure, and ready
states. Unmount the composer coherently, retain only the Beryl-owned compact restoration seed, and
validate revision, logical extent, caret, selection, scroll anchor, and target identity before
rebind. Prove stale recovery, cancellation, window close, failed rebind, and whole-value release.

# Phase 174: Build The Multi-Window Shell And Runtime Bootstrap (pending)

Mount window claims and restoration, close versus Exit, progressive bootstrap, runtime/root and
zero-runtime flows, exact activation, and the practical process window-count limit. Prove ordinary
close and Exit across idle, active, compacting, unavailable, and failure states. Running-session
same-home recovery remains explicitly unavailable rather than using retained-service authority.

# Phase 175: Mount Paged Navigation And Settings (pending)

Mount revision-bound paged catalog, search, lineage, activity, model, composer-history, and settings
sources with virtualized presentation. Mount each window's fixed-capacity navigation-history ring
directly into its Back and Forward controls without paging or virtualization. Prove stale-page
rejection, focus and popover preservation, loading and failure states, bounded row residency, and
page release.

# Phase 176: Mount Main-Window Notices (pending)

Mount the bounded notice queue, warned best-effort-home startup notice, exact soft-stop feedback,
lifecycle-yield outcomes, and accepted disabled/error states. Repair and running-session recovery
remain explicit unavailable states; mount no fabricated progress or retained-service fallback.
Prove admission, priority, preemption, replacement, dismissal, overflow, close/Exit, and stale timers.

# Phase 177: Implement Notification Audio Ownership (pending)

Add bounded notification-audio admission, encoded and decoded capacity transfer, the single
process-wide active/latest-waiting playback lane, cancellation, shutdown, and exact release. Prove
replacement, decode and device failure, settings change, close/Exit, and fail-closed runtime behavior
without changing turn semantics.

# Phase 178: Mount Transcript Presentation (pending)

Move the transcript prototype onto immutable shared pages without deep snapshot clones, then mount
realized-frame rendering, semantic anchors, selection, nested widgets, resource demand, explicit
repair-required provenance, and local page/cache release proofs. Prove atomic authority selection
without whole-turn residency and release superseded resident state after handoff. Deferred repair
dispatch remains unmounted.

# Phase 179: Verify And Close The Useful GUI Checkpoint (pending)

Verify multi-window lifecycle, activation, restoration, large drafts, logical marker scale, paged
navigation and settings, notices and audio, long transcript traversal, local cache/page release,
configured working-set behavior, and explicit fail-closed repair/recovery states. Reconcile GUI,
feature, system, package, and dependency authority, then close one fresh independent review before
Checkpoint 5.

# Phase 180: Prove The Exact 0.146.0 Terminal-Repair Surface (pending)

Inspect exact commit-scoped 0.146.0 processor, reducer, and generated-schema evidence for
`thread/turns/list`, both item-list spellings, the complete item union, history identity synthesis,
terminal-status materialization, cursor semantics, and generated-image fields. Update the focused
commit-scoped memory note with reproducible sources. Proceed only if the one descending
`thread/turns/list` request with `limit=1` and `itemsView=full` proves the latest correlated terminal
turn and complete semantic item view under the no-successor gate; otherwise stop and revise
authority without implementing a fallback.

# Phase 181: Implement The Private Terminal-Repair Adapter (pending)

Implement the private release-pinned adapter proven by Phase 179: one no-successor-gated
`thread/turns/list` request, one matching terminal turn, and one bounded semantic-final-item stream
with exact provenance and historical user-input correlation. Prove complete item coverage,
backpressure, typed incomplete outcomes, cursor discard without traversal, and no adjacent-turn,
item-history, whole-thread, general-history, or fabricated-live-event path.

# Phase 182: Add Snapshot-Specific Syndic Repair Storage (pending)

Add snapshot-specific paged Syndic records, codecs, reads, and one atomic repair mutation. It
validates exact CAS/Syndic correlation, terminal outcome, complete ordered item identities and
fields, digests, adapter/release provenance, and finalized media admission; then replaces the whole
canonical selection with the sealed snapshot and enters `FinalizingHistory`. It does not rebuild or
publish transcript projections inside that atomic selection. It never requires live-prefix equality
and never splices live, buffered, GUI, or partial snapshot content. Prove bounded paging/codecs,
identity and digest rejection, scoped reconciliation, zero partial publication, reopen behavior,
and ordinary Syndic-only reads; image-bearing snapshots remain inadmissible until Checkpoint 7
admits authenticated `savedPath` bytes.

# Phase 183: Enforce Repair-Required Successor Gates (pending)

Keep same-thread successor, fork, replacement, rollback, and compaction gated from the first exact
repair-required transition until coherent repaired or explicitly incomplete finalization releases
the gate. Other threads and structurally healthy unrelated work remain independent. Prove every
gated command family, concurrent joins, restart persistence, unrelated progress, and rejection of
live-prefix, buffered-content, or GUI-state bypasses before any repair dispatch can be mounted.

# Phase 184: Claim Terminal Repair Durably (pending)

Implement the durable target-scoped Syndic repair-request claim and derive the only private backend
capability from its consumed disposition. A consumed but unsettled claim survives process loss as
terminal incomplete authority and can never authorize a second repair request. Keep runtime claim
consumption and backend dispatch unmounted until Phase 193 installs cross-domain repair-media
admission; every repair-required target remains gated with its claim unconsumed. Prove the unmounted
boundary, concurrent admission, every dispatch crash cut, backend refusal, store loss, restart
recovery, unrelated-thread progress, and permanent rejection of duplicate dispatch.

# Phase 185: Integrate Atomic Terminal-Turn Repair (pending)

Connect the private backend sink to snapshot staging and the one atomic Syndic replacement behind
the unmounted runtime boundary. Once later mounted, a repair-required target consumes the
no-successor proof and durable Phase 183 claim, then converges to exactly repaired or explicitly
incomplete. Both dispositions enter
`FinalizingHistory`; repaired selects the complete snapshot while incomplete selects no replacement.
Bounded durable work reaches a fixed point, publishes one coherent transcript generation, and only
then atomically releases the gate. Prove request/store-loss convergence, bounded whole-turn rebuild,
generation-atomic presentation, same-thread exclusion, unrelated progress, and exactly-once
repaired-or-incomplete release.

# Phase 186: Add Bounded Outage Capture (pending)

Add the fixed-capacity prioritized outage buffer for already active exact targets. Identity,
terminal outcome, final answer, narrative, user correlation, and generated-media handoff metadata
precede operational content. Any rejected, evicted, partial, or unrepresentable canonical fact marks
the whole turn repair-required; buffered content is transient presentation only. Prove priority and
hard limits, complete versus dropped capture, loss behavior, repair classification, and no replay.

# Phase 187: Publish Entirely Fresh Same-Home Recovery (pending)

Rebuild running-session recovery as: fence new durable commands; close and dispose the failed
service; recover the same home into a newer healthy generation with fresh writer and handles;
construct a fresh backend/app service and connections; converge durable pending, stop, compaction,
and repair obligations behind the startup fence; attach the supervisor; publish atomically; then
reacquire CAS projections from durable binding authority. No old connection, broker, router,
projection, loaded session, lease, candidate, scheduler, or worker crosses the boundary. Prove the
ordered fence, complete disposal, zero old-authority reuse, durable obligation convergence, atomic
publication, post-publication reacquisition, and failure before publication.

# Phase 188: Verify And Close The Repair And Recovery Checkpoint (pending)

Run the complete functional storage, protocol, concurrency, restart, configured-limit, static-
boundary, and source-residue gates. Factor proofs by Fjall, home-store, domain, sidecar, app, backend,
and CAS-repair ownership, with only representative end-to-end compositions. Verify named Beryl-owned
queues, pages, caches, pools, and workers release or evict after repetition; treat RSS and renderer
counters as observational evidence only.
Reconcile API docs, memory/failure notes, and the tracker, then close a fresh independent review
before Checkpoint 6. Sustained stress still requires the Operator's AC-power gate and proves
configured correctness bounds rather than performance targets. Verify the declared runtime repair
gap remains fail-closed: no target dispatches and no durable claim is consumed before Phase 193.

# Phase 189: Implement Branch Discussion Creation (pending)

Implement immutable branch selection provenance, readonly context, durable child conversation
creation, first submission, ordinary child conversation, inherited image-label authority, and exact
branch-local label allocation. Prove creation failure leaves no runnable child or premature CAS work.

# Phase 190: Implement Resolution Handoff (pending)

Implement resolution admission, queued-input deferral, durable parent handoff, busy-parent ordering,
restart recovery, idempotency, retry, successful archive, and post-archive navigation outcomes.
Prove no lost input, duplicate handoff, early archive, or parent-order violation.

# Phase 191: Verify And Close Checkpoint 6 (pending)

Verify child creation, inherited label authority, first submission, resolution ordering, restart,
retry, archive, navigation, and failure states. Reconcile feature, system, package, GUI, and storage
authority, then close a fresh independent review before Checkpoint 7.

# Phase 192: Implement Asset Admission And Durable Ownership (pending)

Implement Beryl-home image admission for paste and generated output, content-addressed sidecars,
byte-for-byte collision verification, labels, references, Host/WSL projection, and generated-output
ownership. Prove arbitrary-size streaming, cancellation, collision, and durable reopen behavior.

# Phase 193: Implement Media Rendition Resources (pending)

Implement bounded file reads, header parsing, on-demand thumbnail and tile decode workers, CPU
surfaces, upload staging, shared media identity, GPU residency, eviction, and device-loss recovery.
Prove adversarial dimensions, concurrent windows, cancellation, and exact capacity release.

# Phase 194: Implement Generated-Image Repair Media (pending)

Authenticate `savedPath`, stream admitted bytes into inert sidecar and asset staging, then publish
the repaired Syndic snapshot and Beryl asset metadata through one atomic cross-domain cut. Missing,
unusable, incomplete, or failed media remains inert and finalizes the owning turn incomplete; never
retain inline base64. After this admission path is installed, mount the Phase 183 claim-consumption
and Phase 184 repair-dispatch path for all eligible targets. Prove crash cuts, collision, orphan
invisibility, coherent publication, and no pre-admission claim consumption.

# Phase 195: Mount Maintenance Presentation And Deferred Cleanup (pending)

Mount generated-title maintenance and successful branch-archive presentation through established
Syndic authority and bounded Beryl projections. Preserve unreachable turns and resources until a
separately designed future garbage-collection operation; add no graph-dependent semantic search.

# Phase 196: Verify And Close Checkpoint 7 (pending)

Verify arbitrary asset size, adversarial dimensions, concurrent windows, cancellation, collisions,
generated-image repair, atomic cross-domain publication, device loss, cache eviction, and deferred
cleanup boundaries. Reconcile authority and close a fresh independent review before Checkpoint 8.

# Phase 197: Reconcile Final Live Authority (pending)

Reconcile root, feature, system, package, GUI, settings, hotkey, diagnostics, source, dependency,
memory, failure, plan, and tracker authority against the implemented target state.

# Phase 198: Remove Obsolete Surfaces (pending)

Remove every remaining shim, obsolete export, test, key, diagnostic, role, archived-source
membership edge, and forbidden API reference. Prove the live source graph exposes only target-state
authority and no compatibility path.

# Phase 199: Verify Named Resource Boundaries (pending)

Verify every named Beryl-owned queue, cache, pool, page set, editor window, transcript window, media
decoder, CPU surface cache, GPU texture cache, and worker set obeys its configured limit and releases
or evicts after repetition. Treat RSS and renderer counters as observational diagnostics only.

# Phase 200: Run End-To-End Functional Verification (pending)

Run storage, runtime/CAS, multi-window, conversation, branch, asset, recovery, and Windows functional
verification. Coordinate with the Operator before sustained stress or performance measurement so
the laptop can remain on AC power.

# Phase 201: Close The Rework (pending)

Obtain fresh independent architectural completion review, close every finding, compact and archive
the rework tracker under the project convention, and leave no unresolved target-state authority.
