# Target Docs

- [Root design](../../design.md)
- [Beryl home](../../features/beryl-home/design.md)
- [Main windows](../../features/main-windows/design.md)
- [Backend recovery](../../features/backend-runtime-recovery/design.md)
- [Conversation threads](../../features/conversation-threads/design.md)
- [Branch discussions](../../features/branch-discussions/design.md)
- [Composer](../../features/composer/design.md)
- [Transcript](../../features/transcript/design.md)
- [Activity panel](../../features/activity-panel/design.md)
- [Image assets](../../features/image-assets/design.md)
- [Notifications](../../features/notifications/design.md)
- [Status line](../../features/status-line/design.md)
- [Settings](../../features/settings/design.md)
- [Theming](../../features/theming/design.md)
- [Lifecycle yield](../../features/lifecycle-yield/design.md)
- [Diagnostics](../../features/diagnostics/design.md)
- [Crash reporting](../../features/crash-reporting/design.md)
- [Crash-report GUI](../../features/crash-reporting/gui.md)
- [Crash-reporting system](../../systems/crash-reporting/design.md)
- [Activity-panel GUI](../../features/activity-panel/gui.md)
- [Backend-recovery GUI](../../features/backend-runtime-recovery/gui.md)
- [Beryl-home GUI](../../features/beryl-home/gui.md)
- [Branch-discussion GUI](../../features/branch-discussions/gui.md)
- [Composer GUI](../../features/composer/gui.md)
- [Conversation-thread GUI](../../features/conversation-threads/gui.md)
- [Main-window GUI](../../features/main-windows/gui.md)
- [Notification GUI](../../features/notifications/gui.md)
- [Settings GUI](../../features/settings/gui.md)
- [Status-line GUI](../../features/status-line/gui.md)
- [Theming GUI](../../features/theming/gui.md)
- [Transcript GUI](../../features/transcript/gui.md)
- [CAS-live Syndic transcript](../../systems/cas-live-syndic-transcript/design.md)
- [Syndic conversation history](../../systems/syndic-conversation-history/design.md)
- [Syndic concepts](../../systems/syndic-conversation-history/concepts.md)
- [Backend runtime](../../systems/backend-runtime/design.md)
- [Beryl-home storage](../../systems/beryl-home-storage/design.md)
- [Branch-discussion handoff](../../systems/branch-discussion-handoff/design.md)
- [Bounded resource dataflow](../../systems/bounded-resource-dataflow/design.md)
- [Image-asset system](../../systems/image-assets/design.md)
- [Theme-runtime system](../../systems/theme-runtime/design.md)
- [Transcript presentation](../../systems/transcript-presentation/design.md)
- [Transcript renderer architecture](../../systems/transcript-presentation/renderer-architecture.md)
- [Transcript shell boundary](../../systems/transcript-presentation/shell-boundary.md)
- [GUI integration](../../gui/integration.md)
- [External GUI specs](../../gui/external-specs.md)
- [Shared input hotkeys](../../input-hotkeys.md)
- [Activity-panel widget](../../gui/widgets/activity-panel/spec.md)
- [Code-panel widget](../../gui/widgets/code-panel/spec.md)
- [Conversation-composer widget](../../gui/widgets/conversation-composer/spec.md)
- [Image-marker widget](../../gui/widgets/image-marker/spec.md)
- [Image-preview widget](../../gui/widgets/image-preview/spec.md)
- [Main-window-notice widget](../../gui/widgets/main-window-notice/spec.md)
- [Native-lineage recovery prompt](../../gui/widgets/native-lineage-recovery-prompt/spec.md)
- [Table-panel widget](../../gui/widgets/table-panel/spec.md)
- [Theme-editor widget](../../gui/widgets/theme-editor/spec.md)
- [Two-segment split-button widget](../../gui/widgets/two-segment-split-button/spec.md)
- [Thread-lineage widget](../../gui/widgets/thread-lineage/spec.md)
- [Thread-root-picker widget](../../gui/widgets/thread-root-picker/spec.md)
- [Thread-selector-trigger widget](../../gui/widgets/thread-selector-trigger/spec.md)
- [Transcript-view widget](../../gui/widgets/transcript-view/spec.md)
- [Expected-action contract](../../gui/widgets/contracts/expected-action-availability.md)
- [Beryl command geometry](../../gui/widgets/contracts/beryl-command-geometry.md)
- [Scroll-ownership contract](../../gui/widgets/contracts/scroll-ownership.md)
- [Beryl package](../../../crates/beryl/doc/design.md)
- [Beryl app package](../../../crates/beryl-app/doc/design.md)
- [Beryl backend package](../../../crates/beryl-backend/doc/design.md)
- [Beryl home-store package](../../../crates/beryl-home-store/doc/design.md)
- [Beryl model package](../../../crates/beryl-model/doc/design.md)
- [Beryl state package](../../../crates/beryl-state/doc/design.md)
- [Beryl stream package](../../../crates/beryl-stream/doc/design.md)
- [Syndic storage package](../../../crates/syndic-storage/doc/design.md)
- [Owned Fjall design](../../../../fjall-fork/doc/design.md)
- [Owned LSM-tree design](../../../../fjall-fork/crates/lsm-tree/doc/design.md)
- [GPUI text-input spec](../../../../gpui-text-input/doc/gui/widgets/text-input/spec.md)
- [GPUI text-input design](../../../../gpui-text-input/doc/design.md)
- [GPUI text-input external specs](../../../../gpui-text-input/doc/gui/external-specs.md)
- [Owned Unicode segmentation design](../../../../unicode-segmentation-fork/doc/design.md)
- [Owned GPUI fork design](../../../../zed-fork/doc/design.md)
- [Owned GPUI hidden-window first publication](../../../../zed-fork/doc/features/hidden-window-first-publication/design.md)
- [GPUI settings-window design](../../../../gpui-settings-window/doc/design.md)
- [GPUI settings-window external specs](../../../../gpui-settings-window/doc/gui/external-specs.md)
- [GPUI settings-window spec](../../../../gpui-settings-window/doc/gui/widgets/settings-window/spec.md)
- [GPUI settings-row spec](../../../../gpui-settings-window/doc/gui/widgets/settings-row/spec.md)
- [GPUI color-input spec](../../../../gpui-settings-window/doc/gui/widgets/color-input/spec.md)
- [GPUI color-picker spec](../../../../gpui-settings-window/doc/gui/widgets/color-picker/spec.md)
- [GPUI scrollbar spec](../../../../gpui-scrollbar/doc/gui/widgets/scrollbar/spec.md)

# Cutover Boundary

- Old workspace-era state is discarded. No importer, dual write, compatibility reader, migration
  adapter, or renamed old model is allowed.
- CAS remains the execution provider; the root and CAS-live design's pre-route buffering exception
  applies while completing this rework, and provider replacement is deferred until closure.
- Live source may depend only on final target packages and explicitly retained low-level leaves; it
  may not import or expose archived source.
- Intentional cutover gaps stay visible. No compatibility alias, aggregate buffer, compile-only
  facade, universal governor, or other bridge may conceal an unimplemented target boundary.
- Runtime capability probes, hard-stop and coarse-cleanup surfaces, and retained-service adoption
  are removed before their replacements. Exact release admission, exact soft stop, and fresh-service
  recovery are the only retained runtime directions.
- Removing retained-service adoption intentionally makes running-session same-home recovery
  unavailable until the fresh-service recovery slice closes the gap. During that interval a failed
  store remains fail-closed with coherent durable/user state preserved; no old connection transfer,
  compatibility recovery path, or unbounded outage buffer may conceal the missing boundary.
- The private release-pinned exact terminal-turn repair adapter is the only allowed live CAS-history
  dependency; no ordinary transcript, catalog, or replay path may consume it.
- A proven-unavailable pinned repair source may converge incomplete through the accepted recovery
  components; missing implementation alone does not authorize that outcome. Any future eligible
  repair remains successor-gated and undispatched until atomic cross-domain repair-media admission
  is accepted, because the request may provide the first complete media proof; no claim consumption
  or media-less fallback conceals that implementation gap.
- No whole-value compatibility path may conceal missing range-backed editor, storage, or
  presentation boundaries; declared individual-operation limits do not authorize such a path.
- During the active cutover, Syndic V7 registers only implemented families; deferred materializer
  and repair families join in their owning phases rather than existing as empty placeholders.
- Recovery discovery uses accepted compact sources; new inventory composition must preserve
  source-only discovery and cannot reintroduce broad input-gate or history sweeps.
- Marker-seal construction borrows the graph-owned private home candidate and exposes no ordinary constructor
  or global discovery. Complete-graph composition must retain that single construction owner and
  distribute its shared clones only after publication; isolated test construction grants no
  production replacement or duplicate-capacity authority.
- Replacing immediately healthy initial open intentionally breaks typed consumers until their
  candidate registration and publication boundaries are rebuilt; no alternate healthy opener or
  automatic-publication substitute may conceal that gap.

# Reference Snapshot

- Obsolete documentation is retained under `old-doc/` only as historical reference.
- Removed source is retained under `old-code/` and excluded from live project membership.
- Reusable investigations and invalidated approaches live under `doc/memory/` and `doc/failures/`.

# Forbidden Local APIs

- Any live import, manifest edge, include, test mount, or runtime path into `old-code/`.
- Workspace, semantic-graph, graph-upkeep, checklist, threaded-decision, or graph-search authority.
- Old workspace persistence, direct app-owned home storage, app-local draft/history, or legacy
  catalog and selector models.
- CAS transcript/catalog authority, contextual replay, repeated recovery injection, or historical
  reads outside the exact terminal-turn repair adapter.
- Runtime capability probes against user or synthetic targets; hard-stop or coarse background
  cleanup APIs; retained failed-service connection, projection, lease, quarantine, stable-core, or
  service-epoch adoption.
- Filesystem-object identity continuity, home/sidecar anti-replacement authority, unconditional UNC
  rejection, or a periodic free-space poller.
- Raw Fjall outside `beryl-home-store`, raw provider JSON outside `beryl-backend`, an unbounded queue,
  cache, decode path, or whole-history application projection at a named risk boundary.
- Universal allocation capabilities or exact accounting for allocator metadata, CAS memory,
  GPU-driver residency, or total process RSS.

# Checklist

## Operator Decision Gate

- [x] Operator selected continued CAS use with necessary pre-route buffering and authorized resuming the rework under the reconciled [plan](../../plan.md).

## Checkpoint 0: Complete And Accept Target Authority

- [x] Closed: reconciled all linked feature, system, package, GUI, external-spec, plan, and tracker
  authority with the simplified runtime, storage, repair, recovery, and bounded-resource target.

## Checkpoint 1: Archive And Remove The Obsolete Architecture

- [x] Closed: archived and removed workspace-era source, tests, manifests, settings, diagnostics, theme
  roles, and GUI surfaces without compatibility adapters.

## Checkpoint 2: Establish The Beryl-Home Foundation

- [x] Closed: established the typed home, shared Fjall database, domain registration, writer, durability,
  recovery, lifetime lock, sidecar, state, and package foundations.

## Checkpoint 3: Establish Syndic Threads And CAS Projections

- [x] Closed: established and verified the bounded fail-closed Syndic/CAS projection and Beryl-home
  storage foundation, exact package and outcome boundaries, restart behavior, configured limits,
  residue removal, terminal unavailability, and one-query new-turn free-space admission.

## Checkpoint 4: Build The Multi-Window Shell And Navigation

- [x] Obtained the Operator gate to begin the ordered GUI implementation phases.
- [x] Implemented the typed editable-file theme repository, coherent hot reload, exact retained
  reconciliation, atomic appearance publication, and single preview arbiter without mounting GUI.
- [x] Established the owned `unicode-segmentation` fork with exact bounded streaming word
  boundaries from the resolved 1.13.2 source commit.
- [x] Established fixed-residency segmentation and source-selected UTF-8-safe `gpui-text-input`
  page envelopes over the accepted streaming owners.
- [x] Exposed clone-stable actual-handle identity from the owned GPUI `ScrollHandle` boundary.
- [x] Established the exact keyed-owner, instance-receipt, provider-reentrant `gpui-scrollbar`
  lifecycle before range-backed text input consumes it.
- [x] Completed bounded exact-geometry pre-context replay across released pages and authoritative
  opaque-atom segmentation boundaries.
- [x] Integrated the range-backed `gpui-text-input` widget lifecycle without a whole-string adapter.
- [x] Integrated the keyed `gpui-settings-window` lifecycle, removed the unowned Beryl scrollbar
  render chain, and published one exact-GPUI dependency graph.
- [x] Implemented and independently accepted revision-bound bounded-page split sources in
  `gpui-settings-window` without a resident or compatibility path.
- [x] Established and independently accepted composite positions, ordered zero-width objects,
  exact geometry and hits, and bounded hot-path resume and accounting in owned GPUI.
- [x] Established exact composite positions, bounded object paging and presentation, crate-owned
  scalar proofs, separate capped residency, and the ordinary accepted-GPUI cutover in `gpui-text-input`.
- [x] Added exact staged source-zero-width object mutations and successor adoption in `gpui-text-input`.
- [x] Added bounded exact composite clipboard and payload-free compact restoration validation in
  `gpui-text-input`.
- [x] Integrated bounded exact source-zero-width object realization into canonical GPUI geometry.
- [x] Established the bounded widget-owned cross-owner staged-publication boundary for exact
  geometry, text and object residency, terminal surfaces, queued requests, desired state, active-
  object state, and deferred effects, with exact post-retirement admission and no stable rendering-
  path work.
- [x] Corrected owned-GPUI composite trailing-boundary ownership, zero-width line occupancy and
  resume validation, and capped UTF-8 style-run boundary admission without changing hot-path
  asymptotics.
- [x] Finished exact inline-object interaction, activation, bounded presentation, lifecycle release,
  and atomic text, object, and geometry-index delivery through the accepted staged-publication
  boundary without an accessibility payload or integration.
- [x] Published and canonically pinned the accepted `gpui-text-input` boundary and its owned GPUI dependency chain.
- [x] Established, verified, published, and canonically pinned owned GPUI hidden-window first publication and its unified widget dependency chain before main-window shell construction.
- [x] Replaced the fixed domain-family ceiling with the exact encoded-metadata-derived capacity
  needed by registered domains.
- [x] Established persistent composite draft roots with exact candidate-session, logical-line,
  directional range-source, and bounded marker-proof conformance.
- [x] Established immutable build-transition receipts and bounded session-qualified candidate-only
  edit adoption with exact replay, custody, crash reconciliation, and fail-closed corruption.
- [x] Established bounded exact-root ComposerV1 materialization in `syndic-storage`.
- [x] Added bounded typed valid-successor HomeStore reconciliation before exact-root composer
  submission can complete.
- [x] Added exact bounded abandonment for authenticated pristine unpublished composer candidates,
  including typed replay, rejection, collision, and crash reconciliation.
- [x] Established generation-owned typed domain runtime attachments with borrow-preserving clone-
  stable non-`Copy` handle views, sole generation-slot ownership, exact synchronous retirement,
  stale capability rejection, and failed-candidate cleanup.
- [x] Reconciled proportional engineering-rigor authority and compacted the remaining execution
  window without weakening bounded streaming, durable reconciliation, or Syndic/CAS fencing.
- [x] Completed package-owned prepared-mutation cutover across Syndic, Beryl state, and HomeStore
  integration fixtures without a compatibility path.
- [x] Reconciled durable-start footprint authority and fixtures with the accepted Syndic image-label-
  authority family before further draft-marker implementation.
- [x] Reconciled draft-marker admission authority around operation-owned durable indexes, post-EOF
  assignment, byte-exact replay, and package-derived binding.
- [x] Established independent monotonic Syndic draft-label protection with atomic thread creation,
  exact creation reconciliation, accepted-authority containment, and bounded corruption evidence.
- [x] Established prepublication persisted-aware domain attachment construction with bounded typed
  reads, exact failure classification, and no second initialization stage.
- [x] Established canonical durable draft-marker admission schemas, bounded authenticated index and
  charge primitives, and explicit semantic validation without routine-open scans.
- [x] Reconstructed the bounded generation-owned draft-marker admission attachment before domain
  publication without reviving process capabilities or performing durable cleanup.
- [x] Established fixed HomeStore draft-marker proof composition and the bounded private Asset
  witness without cross-package record exposure or durable admission mutation.
- [x] Established bounded source-only Syndic draft-marker proof attempts with exact candidate/cut
  authority, opaque receipt custody, and no durable admission mutation.
- [x] Extended the opaque Syndic draft-marker proof attempt to accepted local and inherited sources
  through one coherent dependency-neutral Asset witness without durable admission mutation.
- [x] Established operation-owned Syndic draft-marker admission with bounded authenticated indexes,
  exact ordinary and historical candidate adoption, cross-restart replay custody, and complete
  retained-resource reclamation.
- [x] Established cursor-paged composer edits, durable root-transition history, credit-gated editor
  realization, autosave, owned-resource release, and representative large-draft verification.
- [x] Mounted selection-qualified composer submission through the bounded exact-root materialization
  and atomic admission boundary without a whole-value path.
- [x] Mount native-lineage recovery loading, unavailable, failure, and ready states; unmount and
  rebind the composer through the bounded compact restoration seed without retaining whole values.
- [x] Reconciled and independently accepted growable-content limits, compact repair-media
  publication, diagnostic activation, and repair/recovery outcome authority.
- [ ] Implement and verify marker-operation size refusal, shared-capacity refusal, and storage-failure
  feedback while preserving large drafts, bounded residency, and atomic edit outcomes.
- [x] Established and verified bounded process main-window reservations with exact release and independent acquisition/abandonment flight custody.
- [x] Established bounded selected-editor preparation before native construction, exact stale-selection rejection, and canonical first-presentable readiness with supported configuration verification.
- [x] Established immutable GPUI streaming-fragment paint-color overrides with verified geometry and default-rendering preservation.
- [x] Established synchronous range-input live appearance with verified retained scene colors, editor-state preservation, and unchanged host-work custody.
- [x] Identified the stale scrollbar visibility fixture and its obsolete-frame admission evidence gap without changing production behavior.
- [x] Corrected and independently accepted the scrollbar visibility fixture with mutation-sensitive obsolete-frame admission evidence.
- [x] Published and canonically pinned the accepted live-appearance dependency chain with neutral
  package qualification and independent review.
- [x] Established and independently accepted actual atomic GPUI window-set appearance publication
  before shell integration, with [publication evidence](../../failures/theme-runtime-gpui-publication.md).
- [x] Built and independently accepted the injectable hidden main-window shell with exact editor
  binding, shared appearance adoption, adaptive layout, and reservation custody through cleanup.
- [x] Established and independently accepted initial-composer activation, prepared-shell transfer,
  and typed retirement with exact candidate and reservation custody.
- [x] Restored and independently accepted ordinary composer release-fence progress while preserving
  exact semantic settlement and the separate native-lineage quiescence requirement.
- [x] Settled and independently accepted native-lineage seed publication, exact suspension release,
  and stale-route fencing, with [publication evidence](../../failures/native-lineage-seed-publication.md).
- [x] Mounted and independently accepted bounded independent main-window creation with exact claims,
  coherent publication, cancellation settlement, and [verification evidence](../../failures/main-window-creation.md).
- [x] Established and independently accepted monotonic host request allocation across editing,
  history, publication and mounted rebinding, with fresh-generation reset and checked exhaustion;
  [implementation evidence](../../audits/code-simplification/implementation.md) records focused verification and limits.
- [x] Established and independently accepted responsive submission quiescence with exact editor
  fencing, newest-state preparation and bounded waiting/cancellation, with
  [submission evidence](../../failures/mounted-submission-quiescence.md).
- [x] Established and independently accepted [unchanged-opening durable correspondence and host
  integration](../../failures/pristine-editor-publication.md), clean flush and submission, normalized
  normal disposal and native disposal custody with actual widget release.
- [x] Established and independently accepted shared exact close-gate release across foreground,
  worker and unmounted cleanup, with [close-release evidence](../../failures/main-window-close-readiness.md).
- [x] Accepted resident-preserving close flush, coherent read-only interaction, exact failure release
  and authorized final disposal, with [integrated close evidence](../../failures/main-window-close-readiness.md).
- [x] Established and independently accepted atomic fixed-continuation content publication with
  exact reuse, conflict preservation and opaque reconciliation; [publication evidence](../../failures/lifecycle-continuation-staging.md)
  retains the separate app-adoption and close-cancellation acceptance boundaries.
- [x] Adopted and independently accepted atomic fixed-content publication in compaction settlement,
  bounded partial-content failure and exact custody through retirement, and retired the displaced
  seal API with [app-adoption evidence](../../failures/lifecycle-continuation-staging.md).
- [x] Restored and independently accepted compaction response reconciliation with original-driver authority through router handoff and [ordering evidence](../../failures/cas-phase72-compaction-terminal-ordering.md).
- [x] Established and independently accepted bounded same-thread window-close continuation cancellation while preserving accepted input and already-admitted work.
- [x] Restored and independently accepted the existing [submission disposal receipt contract](../../failures/submission-editor-disposal-receipt.md), including atomic receipt publication and historical validation after draft replacement.
- [x] Reconciled process-owned background execution, independent view lifetime, bounded attention,
  and final-window confirmation/shutdown in feature, GUI, CAS-live, runtime, storage and app authority.
  Existing approval-denial policy remains unchanged; interactive approvals are a separate Operator
  decision. The former stop-on-nonfinal-close plan is superseded.
- [x] Corrected recovery-prompt switching at the existing composer mount boundary, preserving
  process recovery routes, exact draft flush, failed-switch context and restored-editor autosave;
  [focused evidence](../../failures/native-lineage-view-lifetime.md) passed independent review.
  Corrected stale capture drivers; the final GUI aggregate passed 32/35. Full shell activation
  integration and [marker admission](../../failures/composer-marker-admission.md) remain open.
- [x] Corrected accepted-next promotion after proven, uncommitted revision conflicts through
  existing fresh scheduling, preserving shutdown and failure precedence; independently reviewed
  [evidence](../../failures/cas-phase13-global-revision-publication.md) covers repeated conflicts,
  single dispatch/capture and durable accepted input through joined shutdown.
- [x] Accepted the bounded scheduled session checkout provider with exclusive custody and exact
  retirement; independent semantic review and 20 focused provider/scheduler/lease checks passed.
- [x] Accepted bounded runtime-interest ownership with coalesced managed launch, foreground admission,
  independent view/work interest and joined retirement; independent review and 74 focused checks passed.
- [x] Accepted production execution-session admission retaining exact required runtime interest through
  checkout and retirement, with runtime-wide configuration invalidation and independent semantic review.
- [x] Retained exact backend policy metadata with loaded-session authority across native and recovered
  projection paths, reuse and retirement; independent review and 25 selected regression checks passed.
- [x] Resolved backend-default ordinary policy and latest applied hidden instructions per attempt,
  including shared invalidation, retries and scheduled checkout; independent review and 42 checks passed.
- [x] Resolved latest applied compaction timeouts at manual and lifecycle admission with retained
  deadlines and typed settings outcomes; corrected distinct-proof handoff and 79 regression checks passed.
- [x] Accepted the bounded process lifecycle attention pool with exact attempt acknowledgement,
  omission and disposal fences; independent review and 24 pool/notice-arbiter checks passed.
- [x] Connected production yield acceptance and terminal/incomplete attention with exact service
  origin and capture-owned failure handoff; independent review and 72 selected checks passed.
- [x] Accepted exact continuation-failure attention across compaction, home loss and uncertain
  settlement, preserving proven admission and cancellation; independent review and 92 checks passed.
- [x] Accepted the process-owned ordinary tool dispatcher with bounded deferred-branch refusal,
  canonical registry preservation and exact lifecycle authority; review and 27 checks passed.
- [x] Accepted bounded managed session preparation for durable scheduler candidates with explicit
  runtime recovery, dependency-release re-entry and joined cleanup.
- [x] Accepted compact non-idle sources with atomic gate maintenance and exact reconciliation.
- [x] Replaced broad startup/pending discovery with bounded generation/revision-bound compact
  sources and exact resolution; independent review and 173 storage/app checks passed.
- [x] Accepted bounded revision-bound session/preparation facts without observation side effects;
  independent review and 26 session, managed-preparation and scheduler checks passed.
- [x] Accepted non-authorizing backend response-custody observations through approval handoff,
  successful writes and disposal; independent review and 46 response/stream checks passed.
- [x] Accepted bounded revision-bound connection target/request facts through handler and writer
  handoff, approval response and disposal; independent review and 34 lifecycle checks passed.
- [x] Retained existing connection worker reservations through primary stop custody and driver
  disposal; independent semantic review and 43 focused stop, approval and capacity checks passed.
- [x] Closed pending and reserved permission slots before caught-panic ingester completion and
  worker reuse; independent semantic review, production compilation and 49 selected checks passed.
- [x] Accepted bounded revision-bound stop and permission facts through removed-owner custody,
  ordered broker handoff and disposal; independent review, production check and 62 tests passed.
- [x] Bounded compaction command custody from admission through disposal with 72 reservations;
  independent review, production check, 37 regressions and final six-case verification passed.
- [x] Accepted revision-bound process work inventory with streaming durable deduplication, exact
  counts, bounded recent-first pages and current metadata without publication. Control custody
  and attention revisions preserve handoffs; independent review and 128 regressions passed.
- [x] Accepted autonomous runtime-owned connection disposal through exact terminal completion,
  poisoned cleanup and the persistent-failure fence; independent review and 118 regressions passed.
- [x] Retained the same admitted runtime demand through loaded projection, compaction, stop custody
  and final driver cleanup; independent review and 169 regressions passed, including real managed
  survival, same-period reattachment and final resource release without session/view ownership.
- [x] Accepted conditional exact idle-session retirement against checkout and loaded/promotion/cleanup
  ownership, preserving failure cuts and final runtime disposal; independent review and 99 regressions passed.
- [x] Shared non-owning exact service work readers and bounded admitted-session gate observations
  without backlog or presentation traversal; independent review, production check and 120 regressions passed.
- [x] Excluded completed response observations from required work while retaining independent
  stopping and cleanup obligations; independent review, production check and 72 regressions passed.
- [x] Added bounded one-shot backend response-completion notification through successful write or
  final capability release; independent review, production checks and 64 regressions passed.
- [x] Accepted the home mutation-observation boundary for the [idle-retirement admission correction](../../failures/process-idle-admission-race.md).
- [x] Accepted scheduler maintenance and autonomous idle-session release with required-work admission serialized against election.
- [x] Reconciled the [confirmed CAS regression baseline failures](../../failures/cas-regression-baseline.md); 19 baseline and 315 app/control cases pass with independent fixture review.
- [x] Implemented the independent fatal report handoff and direct process termination.
- [x] Established the independently reviewed bounded fatal-reporting authority.
- [x] Implemented the isolated report window and its two terminal commands; 23 combined checks and independent review passed.
- [x] Mounted fatal handling and reporter mode with [actual executable panic/window and normal-exit evidence](../../failures/crash-report-test-containment.md#executable-mount-acceptance).
- [x] Removed session-only bootstrap composition while preserving complete state registration and [its bounded acceptance](../../failures/target-bootstrap-composition.md#session-only-facade-removal).
- [x] Established private initial-home candidates and production typed registration adapters with [bounded acceptance](../../failures/target-bootstrap-composition.md#initial-candidate-boundary).
- [x] Qualified Beryl-state candidate fixtures with [complete registration, failure and identity evidence](../../failures/target-bootstrap-composition.md#beryl-state-candidate-qualification).
- [x] Qualified Syndic candidate fixtures with [complete registration and regression evidence](../../failures/target-bootstrap-composition.md#syndic-candidate-qualification).
- [x] Qualified application initial-candidate fixtures with [complete publication and runtime evidence](../../failures/target-bootstrap-composition.md#app-candidate-qualification).
- [x] Accepted explicit borrowed home-candidate recovery access and checked publication with [failure, custody and regression evidence](../../failures/target-bootstrap-composition.md#candidate-recovery-admission).
- [x] Accepted bounded Syndic candidate startup discovery and cursor rebase with [identity, corruption and read regression evidence](../../failures/target-bootstrap-composition.md#candidate-startup-discovery).
- [x] Accepted candidate pending-dispatch evidence with [stabilization, provenance and admission evidence](../../failures/target-bootstrap-composition.md#candidate-pending-evidence).
- [x] Accepted candidate stop-admission evidence with [exact authority, provider finalization and regression evidence](../../failures/target-bootstrap-composition.md#candidate-stop-evidence).
- [x] Accepted candidate delivery-recovery classification with [source fences, stabilization and provider authority evidence](../../failures/target-bootstrap-composition.md#candidate-recovery-classification).
- [x] Accepted candidate compaction recovery with [exact outcome, consumed-receipt and generation evidence](../../failures/target-bootstrap-composition.md#candidate-compaction-recovery).
- [x] Accepted candidate compaction admission with [binding, CAS ownership and stabilization evidence](../../failures/target-bootstrap-composition.md#candidate-compaction-admission).
- [x] Accepted candidate terminal-history evidence with [fixed-point, lifecycle and generation checks](../../failures/target-bootstrap-composition.md#candidate-terminal-history-evidence).
- [x] Accepted candidate history metadata with [exact family, generation and bounded-read evidence](../../failures/target-bootstrap-composition.md#candidate-history-metadata).
- [x] Accepted candidate turn-item pages with [owner isolation, continuation and exact-byte evidence](../../failures/target-bootstrap-composition.md#candidate-turn-item-pages).
- [x] Accepted candidate terminal-history convergence with [fixed-point and command-outcome evidence](../../failures/target-bootstrap-composition.md#candidate-terminal-history-convergence).
- [x] Accepted candidate source-less terminal publication with [stabilization, outcome and authority evidence](../../failures/target-bootstrap-composition.md#candidate-source-less-terminal-publication).
- [x] Accepted candidate active-binding and stop-operation abandonment with [exact outcome and custody evidence](../../failures/target-bootstrap-composition.md#candidate-abandonment-commands).
- [x] Accepted candidate deferred-compaction convergence with [settlement, confirmation and custody evidence](../../failures/target-bootstrap-composition.md#candidate-deferred-compaction-convergence).
- [x] Accepted sequential candidate startup recovery with [case, paging, drift and custody evidence](../../failures/target-bootstrap-composition.md#candidate-startup-integration).
- [x] Accepted candidate startup revision reads with [identity, publication and failure evidence](../../failures/target-bootstrap-composition.md#candidate-startup-revision).
- [x] Closed the typed candidate convergence consumer boundary with [independent scope review](../../failures/target-bootstrap-composition.md#candidate-consumer-closure).
- [x] Accepted gated candidate service references with exclusive retirement ownership and [lifecycle evidence](../../failures/target-bootstrap-composition.md#gated-service-references).
- [x] Joined partial ordinary compaction-worker construction before owner disposal; [acceptance evidence](../../failures/target-bootstrap-composition.md#partial-compaction-worker-construction).
- [x] Adapted ordinary CAS consumers to non-owning service references with exclusive owner retirement and explicit joined shutdown; [acceptance evidence](../../failures/target-bootstrap-composition.md#ordinary-cas-home-ownership).
- [x] Accepted complete initial process service-graph publication and joined retirement with retained proof custody; [acceptance evidence](../../failures/target-bootstrap-composition.md#complete-initial-graph-publication-and-retirement).
- [x] Accepted explicit failed initial-service disposal with original candidate return and retained home-close/reconciliation custody; [evidence](../../failures/target-bootstrap-composition.md#initial-service-attempt-disposal-prerequisite).
- [x] Accepted cancellable dormant worker fencing and partial compaction join wakeup; [acceptance evidence](../../failures/target-bootstrap-composition.md#cancellable-initial-worker-fence).
- [x] Accepted private initial CAS service preparation with candidate recovery and joined abandonment before home retirement; [acceptance evidence](../../failures/target-bootstrap-composition.md#initial-cas-service-preparation).
- [x] Accepted private marker preparation and removed global discovery; [acceptance evidence](../../failures/target-bootstrap-composition.md#initial-marker-service-ownership).
- [x] Accepted shared initial candidate custody across CAS and marker preparation with exact provenance and joined disposal; [acceptance evidence](../../failures/target-bootstrap-composition.md#shared-initial-candidate-custody).
- [x] Accepted dormant physical theme watchers with exact-generation release and joined cancellation; [acceptance evidence](../../failures/target-bootstrap-composition.md#dormant-physical-theme-watchers).
- [x] Accepted typed theme subscription preparation with candidate qualification and joined activity custody; [acceptance evidence](../../failures/target-bootstrap-composition.md#typed-theme-subscription-preparation).
- [x] Accepted private app theme-runtime preparation and shared postpublication loading with exact-generation fencing; [acceptance evidence](../../failures/target-bootstrap-composition.md#app-theme-runtime-preparation). Complete graph composition remains open.
- [x] Accepted bounded candidate runtime-record validation for managed-session preparation; [acceptance evidence](../../failures/target-bootstrap-composition.md#candidate-runtime-record-validation).
- [x] Accepted private candidate managed-session configuration with dormant scheduler attachment and joined failure cleanup; [acceptance evidence](../../failures/target-bootstrap-composition.md#candidate-managed-session-configuration). Complete graph composition and its remaining factory inventory remain open.
- [x] Accepted the generation-scoped [submission-to-execution handoff](../../failures/process-submission-execution-handoff.md) before successor-editor activation.
- [x] Accepted [managed process execution ownership](../../failures/process-lifecycle-continuation-projection.md#accepted-process-lifetime-composition)
  across direct/accepted input, compaction, continuation and terminal-history convergence without views.
- [ ] Mount immediate live detach/reattach and the bounded application-wide Running threads picker,
  with process-owned lifecycle attention independent of the originating window.
- [x] Implement process-wide dispatch fencing and exact graceful shutdown before native final-window
  and Exit confirmation, serialized close designation, and durable restore-mode integration.
  The service component is accepted with 18 focused and 81 affected regressions; native mounting
  remains separate. See [coordinator acceptance](../../failures/process-shutdown-pending-turn.md#coordinator-composition-acceptance).
- [x] Accepted process-owned ordinary close versus Exit, including created windows, exact layout,
  healthy and failed-home resident recovery and fresh activation after cancellation; [evidence](../../failures/ordinary-close-recovery.md#ordinary-command-integration-acceptance).
- [ ] Implement restoration, progressive bootstrap, runtime/root
  creation, and zero-runtime onboarding through the accepted bounded main-window boundary.
- [x] Accepted exact interrupted-close State recovery with preserved identity, renewed claims and
  independently verified persistence; [evidence](../../failures/ordinary-close-recovery.md#typed-persistence-acceptance).
  Ordinary command and complete recovery mounting remain separate.
- [x] Accepted failed-home resident custody, candidate publication with supplied typed proofs and
  protected same-editor reconstruction; [evidence](../../failures/ordinary-close-recovery.md#failed-resident-capability-acceptance).
  Ordinary-command recovery mounting remains separate.
- [x] Accepted bounded candidate marker preparation with retained original flight/outcome custody and exact host handoff; [evidence](../../failures/ordinary-close-recovery.md#candidate-marker-preparation-acceptance).
- [x] Accepted GPUI recoverable native destruction with exact surviving-window identity, bounded
  unresolved custody and a single aligned canonical dependency graph; [evidence](../../failures/ordinary-close-recovery.md#recoverable-native-destruction-acceptance).
  Healthy durable restoration and protected editor/claim integration remain ordinary-command work.
- [x] Accepted composed final teardown with bounded detached read-only sources, exact native cleanup and explicit blocked Quit Anyway; [evidence](../../failures/running-shutdown-retirement.md#final-teardown-acceptance).
- [x] Clarified complete restore-set validation and the approved native-publication failure exception; [decision evidence](../../failures/target-bootstrap-composition.md#restore-set-native-publication-boundary).
- [x] Accepted exact empty-session threadless initialization with revision and identity fencing,
  unchanged schema and retained uncertain-command reconciliation; 18 checks and independent review passed.
- [x] Accepted distinct restored-editor preparation and disposal with exact source and service-lifetime
  fences, preserving saved members and original reconciliation; [152-case acceptance](../../failures/target-bootstrap-composition.md#restored-editor-service-lifetime).
- [x] Accepted exact restored-claim activation before selected-editor binding, with serialized
  sibling advancement and retained uncertain-command custody; [25-case acceptance](../../failures/target-bootstrap-composition.md#restored-claim-activation).
- [x] Accepted exact threadless source preparation and ordinary shell mounting without fabricated
  acquisition or editor state; [77-case acceptance](../../failures/target-bootstrap-composition.md#threadless-shell-custody).
- [x] Bound acquisition, creation and initial editors to process-owned home service references;
  [84-case acceptance](../../failures/target-bootstrap-composition.md#restored-editor-service-lifetime).
- [x] Accepted restored-editor transfer into hidden ordinary native shells, retaining exact cleanup
  custody and saved members; [74-case acceptance](../../failures/target-bootstrap-composition.md#restored-native-shell-custody).
- [x] Accepted complete bounded restore-set worker coordination, exact empty-session branches and
  typed retained command outcomes; [87-case acceptance](../../failures/target-bootstrap-composition.md#complete-restore-set-coordination).
- [x] Accepted prepared native outer bounds, exact monitor validation and failed-construction cleanup;
  [evidence](../../failures/target-bootstrap-composition.md#prepared-native-outer-bounds).
- [x] Accepted bounded worker geometry resolution and exact saved-window/native-monitor binding;
  [evidence](../../failures/target-bootstrap-composition.md#worker-placement-resolution).
- [x] Connected prepared geometry to hidden restored, threadless and startup acquisition shells,
  preserving original cleanup custody and fractional-scale pixel edges;
  [evidence](../../failures/target-bootstrap-composition.md#hidden-shell-prepared-geometry).
- [x] Accepted exact hidden native worker lifetime, deferred GUI destruction and close/exposure
  fencing; [evidence](../../failures/target-bootstrap-composition.md#accepted-hidden-native-operation-lifetime).
- [x] Qualified documented hidden desktop assignment and current-desktop fallback through real
  first publication; [evidence](../../failures/target-bootstrap-composition.md#hidden-desktop-qualification).
- [x] Accepted the saved-desktop worker with exact GUID conversion, bounded fallback diagnostics
  and balanced COM/native lifetime; [evidence](../../failures/target-bootstrap-composition.md#saved-desktop-worker).
- [x] Bounded native selected-editor dispatch and authenticated storage-validation stack use; [evidence](../../failures/target-bootstrap-composition.md#native-selected-dispatch-stack).
- [x] Integrated native desktop placement with original shell custody, cancellation fences and typed cleanup; [evidence](../../failures/target-bootstrap-composition.md#shell-desktop-placement-flight).
- [x] Exposed exact native destruction receipts for startup disposal; [evidence](../../failures/target-bootstrap-composition.md#native-destruction-completion).
- [x] Accepted complete native startup-set validation, interaction-gated publication and exact failure disposal/retention; [evidence](../../failures/target-bootstrap-composition.md#complete-native-startup-set). Process Retry/Exit and executable mounting remain separate.
- [x] Accepted proven-retired initial-service Retry using exact process fences and original recovery custody; [37-case evidence](../../failures/target-bootstrap-composition.md#proven-retired-initial-service-reopening).
- [x] Accepted graph-derived window services and one registry across Retry, with worker-side restoration and exact retired-source cleanup; [104-case evidence](../../failures/target-bootstrap-composition.md#published-graph-window-services).
- [x] Accepted dedicated startup failure surfaces with bounded selectable detail, exact Retry/Exit and native close retention; [evidence](../../failures/target-bootstrap-composition.md#dedicated-startup-failure-surfaces).
- [x] Qualified explicit GPUI application lifetime across zero-window cleanup and reopening; [evidence](../../failures/target-bootstrap-composition.md#windows-last-window-executor-lifetime).
- [x] Accepted explicit blocked-startup cleanup presentation and user-requested Quit Anyway, preserving exact attempt and ordinary Exit semantics.
- [x] Accepted failed-home retirement of the complete startup service graph, retaining exact failure custody and granting Retry only after proven disposal.
- [x] Accepted the native startup owner with fixed-home Retry/Exit, exact retained cleanup and immediate complete-set handoff; [evidence](../../failures/target-bootstrap-composition.md#startup-cleanup-failure-presentation). Executable mounting and ordinary running-window shutdown remain separate.
- [x] Reconstructed the executable bootstrap with fixed-home selection, complete private registration,
  production inputs, ordinary process ownership and diagnostic channel-loss Exit;
  [canonical acceptance](../../failures/executable-bootstrap.md#bootstrap-acceptance).
- [ ] Mount revision-bound paged catalog, search, lineage, activity, model, navigation-history,
  composer-history, transcript, and settings sources with virtualized presentation.
- [x] Accepted runtime-scoped Activity producers and the bounded reader service before complete graph publication; see [integration](../../failures/target-bootstrap-composition.md#remaining-graph-factory-inventory) and [reader evidence](../../failures/target-bootstrap-composition.md#activity-reader-publication-and-initial-retry).
- [x] Established and independently accepted the bounded per-window notice arbiter with protected-condition capacity, exact updates, and stale-action rejection.
- [x] Established and independently accepted canonical notice color and typography roles with exact supported fallbacks.
- [x] Implemented and accepted the bounded notice widget with selectable detail, exact commands, focus continuity, inert behavior, and themed rendering.
- [x] Mounted and accepted the per-window notice projection with exact ownership, stable overlay geometry, focus return, and atomic appearance updates.
- [x] Mounted and accepted the best-effort-home warning through successful startup and native window publication with exact admission and visible timer lifecycle.
- [x] Accepted the production exact-stop eligibility and bounded request-feedback service,
  preserving durable/volatile identity, terminal outcomes and disposal independently of operational
  inventory; [evidence](../../failures/executable-bootstrap.md#exact-stop-feedback-admission-evidence).
- [x] Accepted generation-bound exact-stop worker access through published window services without
  retaining graph or execution resources; [canonical and review evidence](../../failures/executable-bootstrap.md#exact-stop-worker-access-readiness).
  Status controls and notice fallback remain separate mounts.
- [ ] Implement exact soft-stop feedback, bounded notification-audio ownership, and explicit fail-closed repair and recovery
  unavailable states without pretending deferred capabilities are mounted.
- [ ] Rework the transcript prototype onto immutable shared pages without deep snapshot clones and
  mount realized-frame rendering, anchors, selection, nested widgets, and resource demand.
- [ ] Verify diagnostic child activation uses ordinary coherent transcript publication and subsequent
  bounded media preparation without a stronger readiness gate.
- [ ] Gate: confirm the Checkpoint 4 product flows, configured limits, and owned-resource release before later product mounting, allowing prerequisite non-GUI service acceptance first.

## Checkpoint 5: Add Terminal Repair And Fresh Same-Home Recovery

- [x] Explicitly driven selected-window and sole-threadless recovery composes resident/service retirement, fresh same-home preparation, attachment, publication, activation and coherent completion under exact-request admission.
- [x] The running owner retains service configuration, selected inputs, appearance and partial progress across abandoned waits, with initial, retired and prepared continuations that avoid replaying completed stages.
- [x] Failed candidate and service preparation return home custody before typed outcome transfer and bounded retry, preserving original Exit evidence, resume outcomes and exact reconciliation custody.
- [x] Fresh selected-resident admission and adoption validate source, history, generation and resource limits while preserving native identity, input, selection and renewed draft tickets.
- [x] Publication and completion retain fences through stale, competing, cancelled and dropped waits, partial binding and pending mount cleanup, with independent component lifecycle and persistence review.
- [x] Automatic interrupted-Exit recovery runs from reported failure through coherent completion with fresh production inputs, bounded task lifetime, exact cancellation/disposal and independently accepted native evidence. Ordinary close/Exit mounting and executable composition remain separate.
- [x] Established exact pinned evidence requiring unavailable repair for the current supported thread population, with the [source limitation](../../failures/cas-terminal-repair-full-view-is-not-completeness.md) retained.
- [x] Accepted ordinary and candidate startup recovery through explicit incomplete history, with retained repair provenance, bounded finalization, reconciliation custody and gate release. Product mounting and fresh-service replacement remain separate.
- [ ] Add the private bounded exact terminal-turn backend adapter only after new exact source evidence proves eligibility, with no cursor traversal or history fallback.
- [ ] Add snapshot-specific paged Syndic repair records and atomic repaired snapshot selection only when repair eligibility is proven, without making those conditional components prerequisites of unavailable-repair recovery.
- [x] Enforce repair-required successor gates before mounting dispatch while unrelated threads
  remain independent.
- [x] Accepted bounded explicit-incomplete finalization and coherent publication before releasing repair-required gates while preserving durable request disposition; storage and startup regression evidence passed independent semantic review.
- [ ] Implement the durable target-scoped request claim and repaired finalization path only after repair eligibility is proven, without runtime dispatch mounting.
- [x] Accepted the private prioritized outage retention component with exact target qualification,
  encoded byte limits and sticky gap tracking; focused tests and independent review passed.
- [x] Accepted bounded unpublished outage assembly with exact late routing, explicit loss attribution and independently reviewed retention priorities.
- [x] Accepted bounded frozen-target custody through witness handoff and projection disposal.
- [x] Specified nonwaiting passive receive, bounded pre-inventory capture and independent cancellation in the app and system authorities.
- [x] Accepted non-authorizing driver receive and independently reachable exact-broker cancellation with staged-ingress, failure and shutdown verification and independent review.
- [x] Accepted the bounded pre-inventory observation slot with connection-only binding, exact qualified loss, conservative unqualified loss and shared payload/route limits; focused verification and independent review passed.
- [x] Accepted backend passive delayed-echo consumption with explicit unverified metadata, no replay after transition, closed structural validation and unchanged request outcomes; independent review and focused tests passed.
- [x] Accepted exact unanswered approval disposal for passive capture, with no denial fallback and complete response-capability release; approval/custody tests and independent review passed.
- [x] Accepted exact-failure passive ingress, nonwaiting generation-bound inventory handoff, unanswered approval cancellation and loss-preserving retirement; focused transport, custody and shutdown tests and independent review passed.
- [x] Accepted ordinary store failure mounting with exact prepared-ingester binding, atomic bounded frozen inventory and retirement disposal. Ordinary polling and dispatched-request failures retain only transient qualified facts; partial loss, overflow and no storage retry passed live transport tests and independent custody review.
- [x] Accepted non-GUI old-service disposal and fresh candidate preparation with the specified outer supervisor attachment/publication protocol. Complete supervisor mounting remains in the full-stack gate below.
- [x] Specified fresh same-home composition: exact runtime retirement separates storage recovery custody, distinct reopening candidates prepare fresh services, and one outer supervisor owns complete-graph publication and retry. Independent readiness review passed.
- [x] Accepted consuming CAS retirement with retained failed-home lock/reconciliation custody, exact rejection, and terminal-only cleanup errors. Focused recovery evidence, shutdown regressions and independent custody review passed.
- [x] Accepted fresh reopening-candidate CAS preparation, configuration, cancellation and failure custody through shared convergence and fenced construction; initial/recovery regressions and independent review passed.
- [x] Gate: independently accepted unavailable-repair convergence, fail-closed successor gating, outage capture and fresh same-home recovery component protocols before branch service implementation. This does not accept complete-stack publication or running-window recovery.
- [ ] After complete graph publication is accepted, verify running-session recovery through full old-stack disposal, a complete fresh candidate stack, valid-successor-aware convergence, supervisor attachment and atomic publication before recovery product mounting.

## Checkpoint 6: Implement Branch Discussion And Resolution Handoff

- [x] Specified process-owned handoff coordination, candidate/runtime scan separation, exact
  branch correlation, bounded job reads and Unicode payload limits, parent ordering frontier and
  transition custody. Independent readiness review accepted correlation and candidate-read
  prerequisites; concrete atomic participants remain separate before coordinator implementation.

- [ ] Accept non-GUI handoff coordination after the recovery service gate and before complete graph publication, without accepting branch GUI mounting.
- [x] Accepted exact ordinary branch-call context without changing lifecycle handlers or enabling
  resolution. Live correlation/schema, lifecycle and process-tool tests, app checks and independent
  authority/custody review passed.
- [x] Accepted explicit candidate durable-job reads and complete Unicode resolution storage bounds.
  Maximum payload, page bounds, restart, stale handles and publication-failure tests, dependent app
  check and independent review passed; atomic coordination remains pending.
- [x] Specified independent revisioned Syndic discussion-gate participants and schema, exact
  admission/release and command custody. Independent readiness review passed; mutation enforcement,
  generated parent-input provenance and app composition retain separate acceptance boundaries.
- [x] Corrected fixture-only creation readiness and specified production exact-selection proof,
  atomic discussion creation and app/catalog custody boundaries. Independent review accepted the
    source-proof prerequisite. Opaque home-bound source preparation and compact writer validation
    are accepted with role, lifecycle, membership, selected-path, UTF-8/range and stale-generation
    rejection evidence. Creation implementation remains pending and ordinary fallback deletion
    retains its existing scope.
- [ ] Implement branch discussion creation, immutable selection provenance, readonly context,
  first submission, ordinary child conversation, and inherited image-label authority without
  copying historical label maps.
- [x] Accepted closed generated parent-input provenance and schema with bounded historical
  validation, exact payload/identity proofs and route exclusion. Schema/corruption, ordinary
  route/admission and app replay regressions, dependent checks and independent review passed;
  atomic parent admission remains a separate boundary.
- [x] Accepted generated-input execution through the existing dispatch, CAS echo correlation,
  projection, stop and recovery paths. Exact Unicode transport with early echoes, one canonical
  item, stop settlement, bounded replay and ordinary regressions passed independent review;
  coordinator composition remains pending.
- [x] Accepted atomic generated parent-input admission with exact sealed content, permanent order,
  pending turn and draft preservation. Bounded natural outcomes and independent durable discovery
  cover maximum text, uncertain commit, restart, identity/content collisions and stale handles;
  focused regressions, dependent checks and independent review passed.
- [x] Accepted app composition of generated parent input and the State starting-parent checkpoint
  in one home command with existing bounded custody. Maximum payload, draft preservation,
  cancellation, writer races, mixed/uncertain outcomes and candidate inspection passed all fifteen
  settlement tests, dependent checks and independent review. Parent execution settlement and the
  durable coordinator remain separate.
- [x] Accepted bounded State transition preparations for exact parent CAS acceptance, retryable
  and terminal failure, and success. Shared ordinary/prepared transition rules retain the closed
  checkpoint matrix and original parent identities; twelve tests, dependent checks and independent
  review passed. Syndic execution observation and atomic terminal app settlement remain separate.
- [x] Accepted bounded parent execution proofs joining generated input, immutable activation,
  exact CAS acceptance and terminal source facts without a current-parent-tail dependency.
  Seven focused cases and eighteen handoff/execution regressions passed, with dependent checks
  and independent review. Unknown dispatch stays unresolved; session loss releases without
  invented acceptance, and execution success remains distinct from captured-history completeness.
- [x] Accepted app parent CAS progress and atomic terminal job/archive/gate settlement with shared
  custody and fresh candidate access. All twenty-two settlement cases, dependent checks and
  independent review passed, including uncertain acceptance and terminal commit, exact CAS
  mismatch, historical parent advancement and no-acknowledgement session loss. Production
  resolution admission, bounded scans and the process coordinator remain separate.
- [x] Accepted positive bounded handoff scan configuration preserving complete State record,
  version and key envelopes, plus shared ordinary/candidate rejection of mis-keyed or terminal
  live-index rows. Fourteen State cases, the app limit matrix, dependent checks and independent
  review passed. Candidate convergence and ordinary scheduling remain separate.
- [x] Accepted bounded sequential candidate handoff convergence with exact current jobs, shared
  settlement, retained command custody and coalesced rescans after own mutations. Five cases,
  app checks and independent review passed, including backlog beyond ready capacity, undispatched
  generated input and an earlier child released after a later parent archive. Graph mounting and
  ordinary coordinator scheduling remain separate.
- [x] Accepted shared prepared State admission with exact old/new record witnesses and historical
  request lookup independent of later attempts. Eighteen State tests, dependent checks and
  independent review passed; candidate access inspects outcomes without admitting new work.
- [x] Accepted generation-scoped resolution admission with exact idempotency, queue deferral,
  archive rejection and atomic State/Syndic publication. Ten admission cases, twenty-seven
  settlement regressions, State/app checks and independent review passed. Process-owned uncertain
  audits retain their bounded slots after caller disposal; fresh candidates inspect exact outcomes
  without admission. Coordinator and tool mounting remain separate.
- [x] Accepted exact explicit retry with shared State rules, captured job revision, pending-gate
  and parent provenance checks, unchanged payload/identities and retained uncertainty custody.
  Fourteen State cases, four app retry cases, the paused-scan regression, dependent checks and
  independent review passed. Candidate and ordinary scans never retry automatically.
- [x] Accepted shared ordinary/candidate activation-cancellation inspection with stable revision
  confirmation. Twenty-four binding cases, dependent checks and independent review passed,
  including uncertain recovery, stale handles and conflicting records. Atomic generated-parent
  failure settlement and ordinary scheduler eligibility remain separate prerequisites.
- [x] Accepted atomic generated-parent nondispatch cancellation and retryable State failure with
  exact shared outcome custody. Ten focused settlement/retry/terminal cases, dependent checks and
  independent review passed; unknown dispatch rejects and uncertain or mixed outcomes remain gated.
  Pre-dispatch reservation and ordinary scheduler integration remain separate.
- [x] Accepted exact pre-dispatch job reservations with original process authority and same-slot settlement re-preparation.
- [x] Accepted bounded nondispatch proof retention, exact fresh-candidate settlement and coalesced slot-release wakes, with recovery-prefix and shutdown-readiness mounting reserved for complete graph composition.
- [x] Accepted generated-parent ordinary execution with exact eligibility, atomic nondispatch pause, explicit same-turn retry, unknown-dispatch fencing and production capacity wakes.
- [x] Accepted pre-activation failure settlement preserving exact pending input and binding through existing bounded proof custody and fresh-candidate recovery.
- [x] Accepted managed preparation failure classification and capture without pausing temporary waits or letting runtime recovery retry a handoff.
- [x] Accepted bounded ordinary handoff coordination with shared publication fencing, coalesced wake routing and joined disposal before complete graph mounting.
- [ ] Implement resolution admission, queued-input deferral, durable parent handoff, busy-parent
  ordering, restart recovery, idempotency, retry, successful archive, and navigation outcomes.
- [ ] Gate: confirm child creation and resolution handoff, including restart and ambiguous outcomes,
  before Checkpoint 7.

## Checkpoint 7: Implement Assets And Deferred Cleanup Boundaries

- [ ] Implement Beryl-home image admission for paste and generated output, content-addressed
  sidecars, labels, references, Host/WSL projection, and generated-output ownership.
- [ ] Integrate bounded file reads, header parsing, on-demand thumbnails and tiles, decode workers,
  CPU surfaces, upload staging, shared media identity, GPU residency, eviction, and device-loss
  recovery.
- [ ] Implement and verify authenticated repair-media staging, sealed paged Asset sets, and bounded owner-qualified resource lookup only after repair eligibility is proven.
- [ ] Integrate and verify compact atomic Asset/Syndic repair selection and complete-only recovery after repair eligibility is proven, with final-command work independent of staged media count.
- [ ] Mount the durable claim and exact repair-dispatch path only after repair eligibility is proven, retaining explicit incomplete outcomes and no inline-base64 fallback.
- [ ] Mount generated-title maintenance and successful branch-archive presentation through their
  established Syndic authority and bounded Beryl projections.
- [ ] Preserve unreachable turns and resources until a separately designed future garbage-
  collection operation.
- [ ] Gate: confirm representative asset limits, hostile dimensions, atomic publication, owned-resource release and deferred cleanup before Checkpoint 8, verifying unavailable repair or eligible generated-image repair as applicable.

## Checkpoint 8: Integrate, Harden, And Close The Rework

- [x] Named live code, tests, fixtures, and configuration by behavior and verified the complete live naming surface.
- [x] Completed the accepted duplicate-implementation and bounded-theme simplification batch, retaining its recorded verification limits.
- [ ] Integrate the accepted private marker owner into complete bootstrap publication and graph retirement, distributing shared clones only from the published graph.
- [ ] Share only the duplicated persistent-tree rebalancing mechanics after editor behavior stabilizes.
- [ ] Remove every remaining shim, obsolete export, test, key, diagnostic, role, archived-source
  membership edge, and forbidden API reference.
- [ ] Close evidence gaps, if any remain after owning phases, for named Beryl-owned queues, caches,
  pools, page sets, editor and transcript windows, media resources, and workers using representative
  dataset growth and repeated-operation release checks rather than exact global RSS accounting.
- [ ] Run end-to-end storage, runtime/CAS, multi-window, conversation, branch, asset, recovery, and
  Windows functional verification.
- [ ] Run sustained stress or performance measurement only for a concrete unresolved supported-
  envelope question, after coordinating with the Operator so the laptop can remain on AC power.
- [ ] Obtain independent architectural completion review, resolve applicable findings, perform
  targeted live-authority/reference checks, and archive this tracker under the project convention.
