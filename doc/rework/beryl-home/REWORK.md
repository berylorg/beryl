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
- Until Checkpoint 7 supplies atomic cross-domain repair-media admission, every terminal repair
  remains successor-gated and undispatched because the historical request may be the first complete
  media proof. Runtime consumes no durable repair-request claim, fixes no incomplete outcome merely
  because the target slice is absent, and admits no media-less fallback.
- No whole-value compatibility path may conceal missing range-backed editor, storage, or
  presentation boundaries; declared individual-operation limits do not authorize such a path.
- During the active cutover, Syndic V7 registers only implemented families; deferred materializer
  and repair families join in their owning phases rather than existing as empty placeholders.
- Marker-seal construction retains its current shared registry until validated-home bootstrap
  enforces single construction and injects shared clones; that slice removes discovery without
  permitting duplicate home-level flight capacity.

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
- [ ] Compose production runtime admission and process work ownership across direct submission,
  accepted input, compaction, continuation and terminal-history convergence.
- [ ] Mount immediate live detach/reattach and the bounded application-wide Running threads picker,
  with process-owned lifecycle attention independent of the originating window.
- [ ] Implement process-wide dispatch fencing and exact graceful shutdown before native final-window
  and Exit confirmation, serialized close designation, and durable restore-mode integration.
- [ ] Implement ordinary close versus Exit, restoration, progressive bootstrap, runtime/root
  creation, and zero-runtime onboarding through the accepted bounded main-window boundary.
- [ ] Mount revision-bound paged catalog, search, lineage, activity, model, navigation-history,
  composer-history, transcript, and settings sources with virtualized presentation.
- [x] Established and independently accepted the bounded per-window notice arbiter with protected-condition capacity, exact updates, and stale-action rejection.
- [x] Established and independently accepted canonical notice color and typography roles with exact supported fallbacks.
- [x] Implemented and accepted the bounded notice widget with selectable detail, exact commands, focus continuity, inert behavior, and themed rendering.
- [x] Mounted and accepted the per-window notice projection with exact ownership, stable overlay geometry, focus return, and atomic appearance updates.
- [ ] Implement the warned best-effort-home startup notice, exact soft-stop
  feedback, bounded notification-audio ownership, and explicit fail-closed repair and recovery
  unavailable states without pretending deferred capabilities are mounted.
- [ ] Rework the transcript prototype onto immutable shared pages without deep snapshot clones and
  mount realized-frame rendering, anchors, selection, nested widgets, and resource demand.
- [ ] Verify diagnostic child activation uses ordinary coherent transcript publication and subsequent
  bounded media preparation without a stronger readiness gate.
- [ ] Gate: confirm the Checkpoint 4 product flows, configured limits, and owned-resource release
  before Checkpoint 5.

## Checkpoint 5: Add Terminal Repair And Fresh Same-Home Recovery

- [ ] Add the private bounded exact terminal-turn backend adapter; accept and discard bounded cursor
  metadata without following it and provide no adjacent-turn, item-history, or whole-thread
  fallback.
- [ ] Add snapshot-specific paged Syndic repair records, atomic repaired snapshot selection,
  and bounded projection-finalization records without mounting repair dispatch.
- [ ] Enforce repair-required successor gates before mounting dispatch while unrelated threads
  remain independent.
- [ ] Implement, but do not runtime-mount, one durable target-scoped request claim and the repaired
  or explicit-incomplete path through `FinalizingHistory`, coherent generation publication, and
  gate release.
- [ ] Add the prioritized outage buffer for already-active exact targets without making buffered
  content canonical history.
- [ ] Rebuild running-session same-home recovery through old-service disposal, an unpublished fresh
  candidate stack, valid-successor-aware durable convergence, supervisor attachment, and atomic publication.
- [ ] Gate: verify terminal repair, fail-closed successor gating, outage capture, and fresh same-home
  recovery before branch-discussion implementation begins.

## Checkpoint 6: Implement Branch Discussion And Resolution Handoff

- [ ] Implement branch discussion creation, immutable selection provenance, readonly context,
  first submission, ordinary child conversation, and inherited image-label authority without
  copying historical label maps.
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
- [ ] Implement and verify authenticated repair-media staging, sealed paged Asset sets, and bounded
  owner-qualified resource lookup.
- [ ] Integrate and verify compact atomic Asset/Syndic repair selection and complete-only recovery
  with final-command work independent of the staged media count.
- [ ] Mount the durable claim and exact repair-dispatch path for eligible targets, with explicit
  incomplete outcomes for missing or unusable media and no inline-base64 fallback.
- [ ] Mount generated-title maintenance and successful branch-archive presentation through their
  established Syndic authority and bounded Beryl projections.
- [ ] Preserve unreachable turns and resources until a separately designed future garbage-
  collection operation.
- [ ] Gate: confirm representative large inputs and configured admission limits, hostile dimensions,
  generated-image repair, atomic publication, owned-resource release, and deferred cleanup before
  Checkpoint 8.

## Checkpoint 8: Integrate, Harden, And Close The Rework

- [x] Named live code, tests, fixtures, and configuration by behavior and verified the complete live naming surface.
- [x] Completed the accepted duplicate-implementation and bounded-theme simplification batch, retaining its recorded verification limits.
- [ ] Replace marker-service global discovery with explicit shared home ownership when bootstrap composition is assembled.
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
