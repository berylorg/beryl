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
- Until Checkpoint 6 supplies atomic cross-domain repair-media admission, every terminal repair
  remains successor-gated and undispatched because the historical request may be the first complete
  media proof. Runtime consumes no durable repair-request claim, fixes no incomplete outcome merely
  because the target slice is absent, and admits no media-less fallback.
- Large drafts and marker collections remain logically unbounded. Their final editor, storage, and
  presentation paths must be range-backed and paged; no whole-value compatibility path is allowed.

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

- [x] Established typed Syndic threads, drafts, submitted and accepted input, canonical history,
  transcript projections, exact CAS bindings, thread properties, summaries, and catalog sources.
- [x] Established bounded incremental provider ingress, exact narrative and operational capture,
  replayable outbound descriptors, generated-media handoff metadata, and normal terminal audit.
- [x] Established exact ordinary start, steering, next-turn scheduling, restart recovery, exact soft
  interruption, context compaction, and CAS-native lineage or bounded one-time injection.
- [x] Reconciled practical local bounds across Beryl, the owned Fjall stack, and CAS-facing parser,
  queue, page, worker, storage, and projection boundaries without a universal resource governor.
- [x] Cut the pinned CAS target atomically to 0.146.0 without a dual-version branch or Beryl-owned
  collaboration imitation.
- [x] Removed runtime capability probes while preserving exact release admission, ordinary capture,
  model paging, and metadata-only activity reads.
- [x] Removed diagnostic and backend hard-stop/coarse-cleanup surfaces and obsolete tests while
  preserving exact soft-stop approval, close, compaction, and command-bound pre-writer ordering.
- [x] Removed retained-service adoption, service-epoch transfer, quarantine and candidate promotion,
  replacement publication, and obsolete tests; failure now ends in terminal disposal and stable
  unavailability without publishing a replacement.
- [ ] Preserve Fjall `NotCommitted`, `Committed`, and `Indeterminate` through every home-store
  caller, including synchronous provider-ingress registry custody before acknowledgement and no
  reconciliation execution from custody installation alone.
- [ ] Add operation-scoped domain-owned targeted reconciliation while unrelated structurally
  healthy work remains available.
- [ ] Split routine reopen and fresh-writer reacquisition from exhaustive schema validation and scrub.
- [ ] Remove home/state/sidecar object-identity continuity and classify native NTFS versus warned
  best-effort filesystems while retaining reliable lifetime locking and crash-safe publication.
- [ ] Add the synchronous one-query free-space reserve API and invoke it once at every direct or
  queued new-turn start attempt before durable admission, preserving draft or queued input on denial.
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
  candidate stack, durable startup convergence, supervisor attachment, and atomic publication.
- [ ] Gate: complete functional storage, protocol, concurrency, restart, configured-limit, static
  boundary, residue, boundary-owned outcome coverage, and independent architectural review before
  Checkpoint 4.

## Checkpoint 4: Build The Multi-Window Shell And Navigation

- [ ] Obtain the existing Operator gate before beginning GUI implementation.
- [ ] Implement the typed theme repository, exact reconciliation, atomic appearance publication,
  and single preview arbiter before mounting the Themes settings surface.
- [ ] Implement the missing range/page/edit-sink API in `gpui-text-input` before mounting large
  Beryl drafts; no Beryl whole-string adapter may substitute for that dependency boundary.
- [ ] Replace whole-payload draft activation and mutable marker residency with revision-bound text
  and marker ranges, bounded editor windows and compact restoration seeds, incremental
  autosave/undo/submission, and very-large-draft verification.
- [ ] Mount native-lineage recovery loading, unavailable, failure, and ready states; unmount and
  rebind the composer through the bounded compact restoration seed without retaining whole values.
- [ ] Implement independent main windows, exact claims, close versus Exit, restoration, progressive
  bootstrap, runtime/root creation, zero-runtime onboarding, and a practical window-count limit.
- [ ] Mount revision-bound paged catalog, search, lineage, activity, model, navigation-history,
  composer-history, transcript, and settings sources with virtualized presentation.
- [ ] Mount the bounded notice queue, warned best-effort-home startup notice, repair provenance,
  backend recovery, exact soft-stop feedback, bounded notification-audio ownership, and other
  accepted disabled/error states.
- [ ] Rework the transcript prototype onto immutable shared pages without deep snapshot clones and
  mount realized-frame rendering, anchors, selection, nested widgets, and resource demand.
- [ ] Gate: verify multi-window lifecycle, activation, large drafts, logical marker scale, long
  transcript traversal, local cache/page release, and configured working-set behavior before
  Checkpoint 5.

## Checkpoint 5: Implement Branch Discussion And Resolution Handoff

- [ ] Implement branch discussion creation, immutable selection provenance, readonly context,
  first submission, and ordinary child conversation.
- [ ] Implement resolution admission, queued-input deferral, durable parent handoff, busy-parent
  ordering, restart recovery, idempotency, retry, successful archive, and navigation outcomes.
- [ ] Prove production child creation and inherited image-label authority without copying historical
  label maps.
- [ ] Gate: verify child creation, inherited labels, first submission, resolution ordering, restart,
  retry, archive, navigation, failure states, and independent architectural review before
  Checkpoint 6.

## Checkpoint 6: Implement Assets And Deferred Cleanup Boundaries

- [ ] Implement Beryl-home image admission for paste and generated output, content-addressed
  sidecars, labels, references, Host/WSL projection, and generated-output ownership.
- [ ] Integrate bounded file reads, header parsing, on-demand thumbnails and tiles, decode workers,
  CPU surfaces, upload staging, shared media identity, GPU residency, eviction, and device-loss
  recovery.
- [ ] Complete generated-image repair from authenticated `savedPath` through inert sidecar/asset
  staging and one atomic cross-domain Beryl-state/Syndic publication cut; missing or unusable media
  resolves the owning turn incomplete without retaining inline base64, then mount the durable claim
  and exact repair-dispatch path for all eligible targets.
- [ ] Mount generated-title maintenance and successful branch-archive presentation through their
  established Syndic authority and bounded Beryl projections.
- [ ] Preserve unreachable turns and resources until a separately designed future garbage-
  collection operation.
- [ ] Gate: verify arbitrary asset size, adversarial dimensions, concurrent windows, cancellation,
  collisions, generated-image repair, atomic publication, device loss, eviction, deferred cleanup,
  and independent architectural review before Checkpoint 7.

## Checkpoint 7: Integrate, Harden, And Close The Rework

- [ ] Reconcile final root, feature, system, package, GUI, settings, hotkey, diagnostics, source,
  dependency, memory, failure, and plan authority after implementation exposes the target state.
- [ ] Remove every remaining shim, obsolete export, test, key, diagnostic, role, archived-source
  membership edge, and forbidden API reference.
- [ ] Verify every named Beryl-owned queue, cache, pool, page set, editor window, transcript window,
  media decoder, CPU surface cache, GPU texture cache, and worker set obeys its configured limit and
  releases or evicts capacity after repetition.
- [ ] Treat process RSS and renderer counters as observational diagnostics only; do not require an
  exact global CPU/GPU high-water theorem, allocator accounting, CAS memory accounting, or
  GPU-driver residency proof.
- [ ] Run end-to-end storage, runtime/CAS, multi-window, conversation, branch, asset, recovery, and
  Windows functional verification.
- [ ] Coordinate with the Operator before sustained stress or performance measurement so the laptop
  can remain on AC power.
- [ ] Obtain independent architectural completion review and close every finding before archiving
  this tracker under the project convention.
