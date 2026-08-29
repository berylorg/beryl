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
- Large drafts and marker collections remain logically unbounded. Their final editor, storage, and
  presentation paths must be range-backed and paged; no whole-value compatibility path is allowed.
- During the active cutover, Syndic V7 registers only implemented families; deferred materializer
  and repair families join in their owning phases rather than existing as empty placeholders.

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
- [ ] Establish attachment-owned Syndic draft-marker admission with monotonic label protection,
  bounded authenticated indexes, home-wide cross-restart retained-resource limits, and exact
  package-owned replay custody.
- [ ] Replace whole-payload composer mutation and residency with cursor-paged edits, durable root-
  transition history, credit-gated editor realization, compact restoration, autosave, submission,
  and very-large-draft verification.
- [ ] Mount native-lineage recovery loading, unavailable, failure, and ready states; unmount and
  rebind the composer through the bounded compact restoration seed without retaining whole values.
- [ ] Implement independent main windows, exact claims, close versus Exit, restoration, progressive
  bootstrap, runtime/root creation, zero-runtime onboarding, and a practical window-count limit.
- [ ] Mount revision-bound paged catalog, search, lineage, activity, model, navigation-history,
  composer-history, transcript, and settings sources with virtualized presentation.
- [ ] Mount the bounded notice queue, warned best-effort-home startup notice, exact soft-stop
  feedback, bounded notification-audio ownership, and explicit fail-closed repair and recovery
  unavailable states without pretending deferred capabilities are mounted.
- [ ] Rework the transcript prototype onto immutable shared pages without deep snapshot clones and
  mount realized-frame rendering, anchors, selection, nested widgets, and resource demand.
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
  candidate stack, durable startup convergence, supervisor attachment, and atomic publication.
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
- [ ] Complete generated-image repair from authenticated `savedPath` through inert sidecar/asset
  staging and one atomic cross-domain Beryl-state/Syndic publication cut; missing or unusable media
  resolves the owning turn incomplete without retaining inline base64, then mount the durable claim
  and exact repair-dispatch path for all eligible targets.
- [ ] Mount generated-title maintenance and successful branch-archive presentation through their
  established Syndic authority and bounded Beryl projections.
- [ ] Preserve unreachable turns and resources until a separately designed future garbage-
  collection operation.
- [ ] Gate: confirm representative large inputs and configured admission limits, hostile dimensions,
  generated-image repair, atomic publication, owned-resource release, and deferred cleanup before
  Checkpoint 8.

## Checkpoint 8: Integrate, Harden, And Close The Rework

- [ ] Remove every remaining shim, obsolete export, test, key, diagnostic, role, archived-source
  membership edge, and forbidden API reference.
- [ ] Close evidence gaps, if any remain after owning phases, for named Beryl-owned queues, caches,
  pools, page sets, editor and transcript windows, media resources, and workers. Require configured
  limits and owned-capacity release or eviction; treat process RSS and renderer counters as
  observational diagnostics rather than exact global accounting.
- [ ] Run end-to-end storage, runtime/CAS, multi-window, conversation, branch, asset, recovery, and
  Windows functional verification.
- [ ] Run sustained stress or performance measurement only for a concrete unresolved supported-
  envelope question, after coordinating with the Operator so the laptop can remain on AC power.
- [ ] Obtain independent architectural completion review, resolve applicable findings, perform
  targeted live-authority/reference checks, and archive this tracker under the project convention.
