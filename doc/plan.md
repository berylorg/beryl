# Scope

Resume Checkpoint 4 of the Beryl-home architectural rework tracked by
`doc/rework/beryl-home/REWORK.md` under the simplified authority now recorded in the feature,
system, package, and root designs.

The Operator GUI gate is approved. The immediate work first adds app-neutral composite-position
inline objects to the owned GPUI streaming-layout boundary, then completes source-zero-width
inline-object editing in the external range-backed `gpui-text-input` widget and bounded streamed
ComposerV1 successor construction in `syndic-storage` before Beryl mounts any large editor or other
new GUI surface.
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

# Phase 132: Correct Composite Streaming-Layout Boundaries (finished)

Accepted downstream ownership for every nonterminal trailing composite boundary across text,
oversize atoms, inline objects, and logical-line boundary fragments while leaving terminal EOF
ownership explicit. Streaming continuation now carries and validates one compact line-content fact,
so zero-width objects force correct later wrapping and forged or erased occupancy rejects atomically.
Cumulative style-run endpoints must be UTF-8 scalar boundaries before shaping on every admission
route, and the configured run-count cap rejects before that scan so malformed work remains bounded.

Public design and API documentation now state revision-scoped object identity, scalar-safe run
boundaries, and exact owned-boundary behavior. The correction adds no render-time allocation,
registry, lock, source scan, synthetic width, or changed lookup asymptotics; dead ownership fields
were removed. Final evidence passed 153 locked owned-GPUI tests and 246 complete local-sibling tests,
both locked all-target checks, metadata, private documentation, formatting, diff, residue, manifest,
lockfile, and process-cleanup gates. Fresh independent completion review approved the final tree.

# Phase 133: Finish Inline-Object Interaction And Bounded Presentation (wip)

Complete the range-backed widget interaction cutover on the accepted composite geometry and the
accepted staged-publication boundary: use exact source positions for caret, selection,
composition, platform queries, pointer hit testing, keyboard movement, and rendering; traverse each
source-zero-width object as one indivisible step; and stage one-object selection, replacement,
Backspace, Delete, and cut through the ordinary mutation boundary without deleting presentation or
fallback text.

Publish realized-object presentation only from the current coherent surface. Display, opaque
semantic state, and activation eligibility are app-neutral visual and interaction facts. Remove the
accessibility-specific label and description payload and add no OS accessibility tree, platform
semantic nodes, or assistive-technology action route.
Report pointer and active-object Enter/Space activation with the exact binding, revision,
identity, order key, presentation generation, layout epoch, realized bounds, and input origin.
Reject stale or estimated geometry, report realization loss when the exact active object is removed,
replaced, superseded, or unrealized, and retain no offscreen activation anchor.

Keep IME text composition separate from inline-object activation and object editing. Rebind and
unmount must cancel or obsolete all interaction and presentation work, release realized object and
presentation capacity, and leave an admitted edit to settle only at its detached host boundary.
Cover same-anchor first, middle, and last objects, adjacent gaps, read-only behavior, stale keys,
presentation-only generation changes, viewport loss, cancellation, mutation conflict and failure,
exact-fit and one-under limits, repeated release, rejected desired-selection transitions, and
effect publication order. Preserve constant-time realized-object caret and hit lookup, existing
text-map asymptotics, and no routine hot-path scan, lock, registry, or per-lookup allocation.

Prepare text, object, and geometry delivery together for both terminal and nonterminal work before
mutating residency, LRU order, dispatched keys, or geometry. Carry successful resident-page MRU
promotion inside the admitted candidate. Make eligible Enter/Space activation attempts inert rather
than text input on rejection, and stage disable, focus loss, pointer clearing, active-object loss,
and composition clearing coherently. Select All from an off-origin bounded surface must drive exact
terminal endpoints; start and end movement must choose before-all and after-all gaps respectively.
Platform UTF-16 mapping must require exact contiguous source-origin coverage. Restoration export
must compare the complete composite scroll position, and unkeyed cross-surface presentation lookup
must not remain public. Composite realization must consume only fragment-owned object maps so valid
wrapped adjacent objects remain admissible. Charge actual candidate and destination capacities,
including temporary map storage, and ensure prepaint can only read or advance already admitted work
rather than construct a fallible terminal candidate. Rejection proofs must fingerprint ordered
resident identities and payloads, not only counts, and cover terminal and nonterminal same-key retry.

Verify focused and full locked local-sibling `cargo-nextest`, all-target check, locked metadata,
documentation, formatting, diff checks, forbidden-source residue, exact canonical manifest and
lockfile stability, and one fresh independent architectural completion review.

Resumable milestone: interaction, IME, lifecycle, bounded presentation, keyed lookup, prepaint,
public-cutover cleanup, and target-delivery staging are implemented, and the locked library check
passes. Geometry-index text/object delivery still needs the same prepared scanner-delta and
infallible-commit boundary before Phase 133 can close. Public focused tests for keyed presentation,
rejected activation, wrapped overlapping objects, off-origin document endpoints, and contiguous-
origin UTF-16 are written but not yet run; private rejection fingerprints now preserve ordered
resident identities and payloads. Resume by implementing prepared index delivery, pinning the new
terminal accounting oracle that supersedes the known-stale 15,341-byte value, adding terminal and
nonterminal identical-key retry plus prepaint rejection tests, then running the focused and full
gates above.

# Phase 134: Accept The Composite Text-Input Sibling (pending)

Complete the public API documentation and compiling examples, reconcile the sibling design and
widget spec with the accepted implementation, and run focused and full `cargo-nextest`, locked
metadata, all-target checks, documentation, formatting, and diff gates plus a fresh independent
architectural review. Keep publication and the canonical Beryl pin as an explicit entry gate for
Phase 136; neither publication nor pin mutation belongs to this phase.

# Phase 135: Establish Streamed ComposerV1 Successor Construction (pending)

Add a production `syndic-storage` boundary that streams an exact prior ComposerV1 revision plus
ordered text and marker edits into unreachable bounded staging, verifies the canonical successor
identity and summary, seals it, and returns the exact reference consumed by the draft-reference
update. Prove bounded input and output residency, revision conflict, cancellation, supersession,
crash cuts, retry, and orphan-staging behavior without publishing a partial draft or reconstructing
the complete payload.

# Phase 136: Mount Range-Backed Draft And Marker Editing (pending)

After the accepted external inline-object revision is published and canonically pinned, replace
whole-payload draft activation with revision-bound text and marker ranges, bounded editor windows,
incremental autosave/undo/submission over the streamed successor boundary, and very-large-draft plus
logical-marker-scale proofs. Mount marker presentation metadata, activation, insertion,
removal, and restoration directly through the accepted dependency contracts without fabricated
source bytes, a custom overlay, or a resident compatibility path.

# Phase 137: Mount Native-Lineage Recovery And Editor Restoration (pending)

Mount the native-lineage recovery prompt and its accepted loading, unavailable, failure, and ready
states. Unmount the composer coherently, retain only the Beryl-owned compact restoration seed, and
validate revision, logical extent, caret, selection, scroll anchor, and target identity before
rebind. Prove stale recovery, cancellation, window close, failed rebind, and whole-value release.

# Phase 138: Build The Multi-Window Shell And Runtime Bootstrap (pending)

Mount window claims and restoration, close versus Exit, progressive bootstrap, runtime/root and
zero-runtime flows, exact activation, and the practical process window-count limit. Prove ordinary
close and Exit across idle, active, compacting, unavailable, and failure states. Running-session
same-home recovery remains explicitly unavailable rather than using retained-service authority.

# Phase 139: Mount Paged Navigation And Settings (pending)

Mount revision-bound paged catalog, search, lineage, activity, model, composer-history, and settings
sources with virtualized presentation. Mount each window's fixed-capacity navigation-history ring
directly into its Back and Forward controls without paging or virtualization. Prove stale-page
rejection, focus and popover preservation, loading and failure states, bounded row residency, and
page release.

# Phase 140: Mount Main-Window Notices (pending)

Mount the bounded notice queue, warned best-effort-home startup notice, exact soft-stop feedback,
lifecycle-yield outcomes, and accepted disabled/error states. Repair and running-session recovery
remain explicit unavailable states; mount no fabricated progress or retained-service fallback.
Prove admission, priority, preemption, replacement, dismissal, overflow, close/Exit, and stale timers.

# Phase 141: Implement Notification Audio Ownership (pending)

Add bounded notification-audio admission, encoded and decoded capacity transfer, the single
process-wide active/latest-waiting playback lane, cancellation, shutdown, and exact release. Prove
replacement, decode and device failure, settings change, close/Exit, and fail-closed runtime behavior
without changing turn semantics.

# Phase 142: Mount Transcript Presentation (pending)

Move the transcript prototype onto immutable shared pages without deep snapshot clones, then mount
realized-frame rendering, semantic anchors, selection, nested widgets, resource demand, explicit
repair-required provenance, and local page/cache release proofs. Prove atomic authority selection
without whole-turn residency and release superseded resident state after handoff. Deferred repair
dispatch remains unmounted.

# Phase 143: Verify And Close The Useful GUI Checkpoint (pending)

Verify multi-window lifecycle, activation, restoration, large drafts, logical marker scale, paged
navigation and settings, notices and audio, long transcript traversal, local cache/page release,
configured working-set behavior, and explicit fail-closed repair/recovery states. Reconcile GUI,
feature, system, package, and dependency authority, then close one fresh independent review before
Checkpoint 5.

# Phase 144: Prove The Exact 0.146.0 Terminal-Repair Surface (pending)

Inspect exact commit-scoped 0.146.0 processor, reducer, and generated-schema evidence for
`thread/turns/list`, both item-list spellings, the complete item union, history identity synthesis,
terminal-status materialization, cursor semantics, and generated-image fields. Update the focused
commit-scoped memory note with reproducible sources. Proceed only if the one descending
`thread/turns/list` request with `limit=1` and `itemsView=full` proves the latest correlated terminal
turn and complete semantic item view under the no-successor gate; otherwise stop and revise
authority without implementing a fallback.

# Phase 145: Implement The Private Terminal-Repair Adapter (pending)

Implement the private release-pinned adapter proven by Phase 144: one no-successor-gated
`thread/turns/list` request, one matching terminal turn, and one bounded semantic-final-item stream
with exact provenance and historical user-input correlation. Prove complete item coverage,
backpressure, typed incomplete outcomes, cursor discard without traversal, and no adjacent-turn,
item-history, whole-thread, general-history, or fabricated-live-event path.

# Phase 146: Add Snapshot-Specific Syndic Repair Storage (pending)

Add snapshot-specific paged Syndic records, codecs, reads, and one atomic repair mutation. It
validates exact CAS/Syndic correlation, terminal outcome, complete ordered item identities and
fields, digests, adapter/release provenance, and finalized media admission; then replaces the whole
canonical selection with the sealed snapshot and enters `FinalizingHistory`. It does not rebuild or
publish transcript projections inside that atomic selection. It never requires live-prefix equality
and never splices live, buffered, GUI, or partial snapshot content. Prove bounded paging/codecs,
identity and digest rejection, scoped reconciliation, zero partial publication, reopen behavior,
and ordinary Syndic-only reads; image-bearing snapshots remain inadmissible until Checkpoint 7
admits authenticated `savedPath` bytes.

# Phase 147: Enforce Repair-Required Successor Gates (pending)

Keep same-thread successor, fork, replacement, rollback, and compaction gated from the first exact
repair-required transition until coherent repaired or explicitly incomplete finalization releases
the gate. Other threads and structurally healthy unrelated work remain independent. Prove every
gated command family, concurrent joins, restart persistence, unrelated progress, and rejection of
live-prefix, buffered-content, or GUI-state bypasses before any repair dispatch can be mounted.

# Phase 148: Claim Terminal Repair Durably (pending)

Implement the durable target-scoped Syndic repair-request claim and derive the only private backend
capability from its consumed disposition. A consumed but unsettled claim survives process loss as
terminal incomplete authority and can never authorize a second repair request. Keep runtime claim
consumption and backend dispatch unmounted until Phase 158 installs cross-domain repair-media
admission; every repair-required target remains gated with its claim unconsumed. Prove the unmounted
boundary, concurrent admission, every dispatch crash cut, backend refusal, store loss, restart
recovery, unrelated-thread progress, and permanent rejection of duplicate dispatch.

# Phase 149: Integrate Atomic Terminal-Turn Repair (pending)

Connect the private backend sink to snapshot staging and the one atomic Syndic replacement behind
the unmounted runtime boundary. Once later mounted, a repair-required target consumes the
no-successor proof and durable Phase 148 claim, then converges to exactly repaired or explicitly
incomplete. Both dispositions enter
`FinalizingHistory`; repaired selects the complete snapshot while incomplete selects no replacement.
Bounded durable work reaches a fixed point, publishes one coherent transcript generation, and only
then atomically releases the gate. Prove request/store-loss convergence, bounded whole-turn rebuild,
generation-atomic presentation, same-thread exclusion, unrelated progress, and exactly-once
repaired-or-incomplete release.

# Phase 150: Add Bounded Outage Capture (pending)

Add the fixed-capacity prioritized outage buffer for already active exact targets. Identity,
terminal outcome, final answer, narrative, user correlation, and generated-media handoff metadata
precede operational content. Any rejected, evicted, partial, or unrepresentable canonical fact marks
the whole turn repair-required; buffered content is transient presentation only. Prove priority and
hard limits, complete versus dropped capture, loss behavior, repair classification, and no replay.

# Phase 151: Publish Entirely Fresh Same-Home Recovery (pending)

Rebuild running-session recovery as: fence new durable commands; close and dispose the failed
service; recover the same home into a newer healthy generation with fresh writer and handles;
construct a fresh backend/app service and connections; converge durable pending, stop, compaction,
and repair obligations behind the startup fence; attach the supervisor; publish atomically; then
reacquire CAS projections from durable binding authority. No old connection, broker, router,
projection, loaded session, lease, candidate, scheduler, or worker crosses the boundary. Prove the
ordered fence, complete disposal, zero old-authority reuse, durable obligation convergence, atomic
publication, post-publication reacquisition, and failure before publication.

# Phase 152: Verify And Close The Repair And Recovery Checkpoint (pending)

Run the complete functional storage, protocol, concurrency, restart, configured-limit, static-
boundary, and source-residue gates. Factor proofs by Fjall, home-store, domain, sidecar, app, backend,
and CAS-repair ownership, with only representative end-to-end compositions. Verify named Beryl-owned
queues, pages, caches, pools, and workers release or evict after repetition; treat RSS and renderer
counters as observational evidence only.
Reconcile API docs, memory/failure notes, and the tracker, then close a fresh independent review
before Checkpoint 6. Sustained stress still requires the Operator's AC-power gate and proves
configured correctness bounds rather than performance targets. Verify the declared runtime repair
gap remains fail-closed: no target dispatches and no durable claim is consumed before Phase 158.

# Phase 153: Implement Branch Discussion Creation (pending)

Implement immutable branch selection provenance, readonly context, durable child conversation
creation, first submission, ordinary child conversation, inherited image-label authority, and exact
branch-local label allocation. Prove creation failure leaves no runnable child or premature CAS work.

# Phase 154: Implement Resolution Handoff (pending)

Implement resolution admission, queued-input deferral, durable parent handoff, busy-parent ordering,
restart recovery, idempotency, retry, successful archive, and post-archive navigation outcomes.
Prove no lost input, duplicate handoff, early archive, or parent-order violation.

# Phase 155: Verify And Close Checkpoint 6 (pending)

Verify child creation, inherited label authority, first submission, resolution ordering, restart,
retry, archive, navigation, and failure states. Reconcile feature, system, package, GUI, and storage
authority, then close a fresh independent review before Checkpoint 7.

# Phase 156: Implement Asset Admission And Durable Ownership (pending)

Implement Beryl-home image admission for paste and generated output, content-addressed sidecars,
byte-for-byte collision verification, labels, references, Host/WSL projection, and generated-output
ownership. Prove arbitrary-size streaming, cancellation, collision, and durable reopen behavior.

# Phase 157: Implement Media Rendition Resources (pending)

Implement bounded file reads, header parsing, on-demand thumbnail and tile decode workers, CPU
surfaces, upload staging, shared media identity, GPU residency, eviction, and device-loss recovery.
Prove adversarial dimensions, concurrent windows, cancellation, and exact capacity release.

# Phase 158: Implement Generated-Image Repair Media (pending)

Authenticate `savedPath`, stream admitted bytes into inert sidecar and asset staging, then publish
the repaired Syndic snapshot and Beryl asset metadata through one atomic cross-domain cut. Missing,
unusable, incomplete, or failed media remains inert and finalizes the owning turn incomplete; never
retain inline base64. After this admission path is installed, mount the Phase 148 claim-consumption
and Phase 149 repair-dispatch path for all eligible targets. Prove crash cuts, collision, orphan
invisibility, coherent publication, and no pre-admission claim consumption.

# Phase 159: Mount Maintenance Presentation And Deferred Cleanup (pending)

Mount generated-title maintenance and successful branch-archive presentation through established
Syndic authority and bounded Beryl projections. Preserve unreachable turns and resources until a
separately designed future garbage-collection operation; add no graph-dependent semantic search.

# Phase 160: Verify And Close Checkpoint 7 (pending)

Verify arbitrary asset size, adversarial dimensions, concurrent windows, cancellation, collisions,
generated-image repair, atomic cross-domain publication, device loss, cache eviction, and deferred
cleanup boundaries. Reconcile authority and close a fresh independent review before Checkpoint 8.

# Phase 161: Reconcile Final Live Authority (pending)

Reconcile root, feature, system, package, GUI, settings, hotkey, diagnostics, source, dependency,
memory, failure, plan, and tracker authority against the implemented target state.

# Phase 162: Remove Obsolete Surfaces (pending)

Remove every remaining shim, obsolete export, test, key, diagnostic, role, archived-source
membership edge, and forbidden API reference. Prove the live source graph exposes only target-state
authority and no compatibility path.

# Phase 163: Verify Named Resource Boundaries (pending)

Verify every named Beryl-owned queue, cache, pool, page set, editor window, transcript window, media
decoder, CPU surface cache, GPU texture cache, and worker set obeys its configured limit and releases
or evicts after repetition. Treat RSS and renderer counters as observational diagnostics only.

# Phase 164: Run End-To-End Functional Verification (pending)

Run storage, runtime/CAS, multi-window, conversation, branch, asset, recovery, and Windows functional
verification. Coordinate with the Operator before sustained stress or performance measurement so
the laptop can remain on AC power.

# Phase 165: Close The Rework (pending)

Obtain fresh independent architectural completion review, close every finding, compact and archive
the rework tracker under the project convention, and leave no unresolved target-state authority.
