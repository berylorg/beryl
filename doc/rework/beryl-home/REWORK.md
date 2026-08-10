# Target Docs

- Root and GUI authority: `doc/design.md`, `doc/gui/integration.md`,
  `doc/gui/external-specs.md`, and the widget specs linked from those documents.
- Product authority: `doc/features/beryl-home/design.md`,
  `doc/features/main-windows/design.md`, `doc/features/conversation-threads/design.md`,
  `doc/features/branch-discussions/design.md`, `doc/features/composer/design.md`,
  `doc/features/image-assets/design.md`, and `doc/features/transcript/design.md`.
- Supporting feature authority: `doc/features/backend-runtime-recovery/design.md`,
  `doc/features/settings/design.md`, `doc/features/notifications/design.md`,
  `doc/features/activity-panel/design.md`, `doc/features/status-line/design.md`,
  `doc/features/diagnostics/design.md`, `doc/features/lifecycle-yield/design.md`, and
  `doc/features/theming/design.md`.
- Feature GUI authority: `doc/features/beryl-home/gui.md`, `doc/features/main-windows/gui.md`,
  `doc/features/conversation-threads/gui.md`, `doc/features/branch-discussions/gui.md`,
  `doc/features/composer/gui.md`, `doc/features/transcript/gui.md`,
  `doc/features/backend-runtime-recovery/gui.md`, `doc/features/settings/gui.md`,
  `doc/features/notifications/gui.md`, `doc/features/activity-panel/gui.md`,
  `doc/features/status-line/gui.md`, and `doc/features/theming/gui.md`.
- System authority: `doc/systems/beryl-home-storage/design.md`,
  `doc/systems/syndic-conversation-history/design.md`,
  `doc/systems/cas-live-syndic-transcript/design.md`,
  `doc/systems/branch-discussion-handoff/design.md`, `doc/systems/image-assets/design.md`,
  `doc/systems/backend-runtime/design.md`, and `doc/systems/bounded-resource-dataflow/design.md`.
- Presentation authority: `doc/systems/transcript-presentation/design.md`,
  `doc/systems/transcript-presentation/renderer-architecture.md`, and
  `doc/systems/transcript-presentation/shell-boundary.md`.
- Shared concept and input authority: `doc/systems/syndic-conversation-history/concepts.md` and
  `doc/input-hotkeys.md`.
- Package authority: `crates/beryl-home-store/doc/design.md`, `crates/beryl-stream/doc/design.md`,
  `crates/beryl-state/doc/design.md`, `crates/beryl-model/doc/design.md`,
  `crates/syndic-storage/doc/design.md`, `crates/beryl-backend/doc/design.md`,
  `crates/beryl-app/doc/design.md`, and `crates/beryl/doc/design.md`.
- External package authority: `../bounded-json/doc/design.md`,
  `../fjall-fork/doc/design.md`,
  `../fjall-fork/doc/systems/bounded-storage-limits/design.md`,
  `../fjall-fork/crates/lsm-tree/doc/design.md`,
  `../gpui-text-input/doc/gui/widgets/text-input/spec.md`, and
  `../gpui-settings-window/doc/gui/widgets/settings-window/spec.md`.

# Cutover Boundary

- Old workspace-era state is discarded; no importer, dual-write path, compatibility reader,
  migration adapter, or renamed old model is allowed.
- Live source may depend only on final target packages and explicitly retained low-level leaves; it
  may not import or expose archived source.
- Intentional gaps remain visible until their checklist boundary is implemented; no compatibility
  surface may hide a non-building or unmounted boundary.
- Removing the universal `beryl-stream` admission substrate first intentionally leaves backend and
  app service consumers non-building until the immediately following local-bound cutovers remove
  their obsolete imports; no compatibility alias, adapter, or substitute governor may bridge it.
- Removing the provider-capable ordinary lane intentionally makes full-profile session admission
  and each unrestored response or control family unavailable before dispatch until its final
  incremental boundary is installed; no raw-value, whole-DOM, or unbounded-buffering fallback
  may bridge that gap.
- Removing the materialized backend dynamic-tool request boundary intentionally leaves app-owned
  GUI tool modules unavailable until Checkpoint 4 installs their feature-owned incremental sinks;
  no aggregate request, raw-JSON parser, compatibility export, or compile-only facade may bridge it.
- The submitted-text descriptor cutover intentionally leaves image-bearing ordinary execution
  unavailable before dispatch until the immediately following image-descriptor boundary installs
  the final cursor path. The durable pending turn and borrowed loaded projection remain exact; no
  vector-backed adapter, dual API, materializing shim, or reconstructed image list may bridge the
  gap.
- The obsolete `beryl-thread-metadata` domain is removed only through the cohesive Syndic V5
  property cutover. No relationship-shaped replacement, alias, old decoder, dual read, or
  intermediate accepted dual-authority state may preserve execution, title, archive, activity, or
  usage ownership in `beryl-state`.
- No temporary cutover shim is currently authorized.
- Graph-independent semantic search, explicit garbage collection, runtime/root removal, manual
  thread management, and theme hierarchy/editor redesign remain outside this rework.

# Reference Snapshot

- Obsolete documentation is retained under `old-doc/` only as historical reference.
- Removed source is retained under `old-code/` and excluded from live project membership.
- Reusable investigations and invalidated approaches live under `doc/memory/` and `doc/failures/`.

# Forbidden Local APIs

- Any live import, manifest edge, include, test mount, or runtime path into `old-code/`.
- Workspace, semantic-graph, graph-upkeep, checklist, threaded-decision, or graph-search authority.
- Old workspace persistence, direct app-owned home storage, app-local draft/history, or legacy
  catalog and selector models.
- CAS historical transcript/catalog authority, workspace-shaped launch APIs, contextual replay, or
  repeated recovery injection.
- Raw Fjall outside `beryl-home-store`, raw provider JSON outside `beryl-backend`, or an unbounded
  queue, cache, decode path, or whole-history application projection at a named risk boundary.

# Checklist

## Checkpoint 0: Complete And Accept Target Authority

- [x] Accepted and reconciled complete target feature, system, package, GUI, CAS-lineage,
  context-compaction, and recovery authority before implementation resumes.

## Checkpoint 1: Archive And Remove The Obsolete Architecture

- [x] Closed: archived and removed obsolete source, tests, manifests, settings, diagnostics, theme
  roles, and GUI surfaces without retaining compatibility adapters.

## Checkpoint 2: Establish The Beryl-Home Foundation

- [x] Closed: established the final model, home-store, state, lock, durability, recovery, sidecar,
  and package boundaries.

## Checkpoint 3: Establish Syndic Threads And CAS Projections

- [x] Established typed Syndic schemas, durable drafts and submitted input, chunked content,
  canonical history, bounded transcript projections, exact CAS bindings, and recovery sources.
- [x] Established the shared bounded-resource substrate, reusable bounded JSON parser, streamed
  provider ingress, and unpublished Syndic staging.
- [x] Accepted the capacity-one ordered connection broker and exact-target publication permits as
  one verified component.
- [x] Publish routed turn activation durably through the ordered broker before acknowledgement.
- [x] Represent exact-route sealed provider lifecycle conflicts as durable bounded issue source
  events without mutating or replacing canonical item authority.
- [x] Complete atomic ordered-broker publication of checked user-message, sealed provider, and
  loss-convergence sources so no caller-local frontier can race wire order.
- [x] Restored normal `turn/completed` through incremental foreground ingress, the sole ordered
  broker, bounded durable item audit, and exact terminal handoff.
- [x] Remove the remaining materialized provider-event path and every fallback or obsolete caller.
- [x] Verify the complete streamed provider cutover without implementing new components.
- [x] After provider cutover fixed final ownership, inventoried the remaining Beryl-owned
  proportional copies and split them by owning target boundary; no live provider-lifecycle
  materialization remains.
- [x] Replaced submitted text input's app, broker, backend, wire, and correlation topology with one
  count/digest-bound replayable descriptor cursor, leaving local-image execution explicitly
  unavailable before dispatch until its own boundary.
- [x] Restore local-image ordinary execution through one-at-a-time marker, asset-reference,
  sidecar-verification, runtime-path, and descriptor cursor state without retained image lists,
  maps, paths, sources, or handles.
- [x] Replace materialized approval requests and raw diagnostic params with compact incremental
  identity, kind, route, auto-denial, response-authority, and interruption facts.
- [x] Replaced dynamic-tool argument values and request clones with pinned-order registry selection,
  one feature-owned incremental typed sink, compact routing, and one response owner.
- [x] Removed the raw-capture and whole-DOM ordinary lane from provider-capable foreground sessions
  after every admitted size-unbounded server request received its final incremental path.
- [x] Restore the final full-profile streamed `turn/start` composition through replayable dispatch,
  ordered checked-user publication, exact non-idempotent classification, and deliberate stdio
  non-dispatch.
- [x] Reconcile the risk-based resource authority across Beryl and the selected owned forks.
- [x] Accepted the final owned Fjall and nested `lsm-tree` functional contract with practical
  limits, metadata-first reads, checked batches, explicit durability, and no dependency escape.
- [x] Cut Beryl's typed home store and every state and Syndic caller directly to the accepted Fjall
  boundary, leaving `beryl-home-store` as the sole raw dependency owner.
- [x] Cut the backend directly to immutable local foreground capacity, ordinary bounded response
  values, nongeneric fixed pages, and compact closed controls; removed universal resource and exact
  residency surfaces while leaving the app at its explicit direct-cutover boundary.
- [x] Cut the app projection service to immutable local configuration, atomic two-worker
  admission, fixed provider and recovery pages/rings, incremental branch bounds, and concrete
  local diagnostics; removed universal accounting and the obsolete rate-limit map while preserving
  the deliberately unmounted future test island.
- [x] Stress the major provider, input, history, activity, queue, cache, and repeated-operation
  paths against their configured working-set limits without requiring exact allocator accounting.
- [x] Reconciled active-turn steering authority across composer, backend-runtime, CAS-live,
  conversation-history, `beryl-backend`, and atomic `syndic-storage` disposition before
  implementing steering delivery.
- [x] Established the specialized streamed backend steering request, exact outcomes, and delayed
  correlation boundary without mounting app delivery.
- [x] Established durable accepted-input replay and canonical steering correlation authority
  without mounting delivery.
- [x] Mount bounded exact active-steering delivery through durable response, lifecycle, and
  target-loss disposition without mounting scheduler dequeue or restart replay.
- [x] Preserve steerable accepted input under local worker pressure without a durable next-turn
  reclassification.
- [x] Mount level-triggered bounded active-steering scheduling from durable ready-source pages.
- [x] Implement ordered next-turn selection and atomic durable dequeue into a pending ordinary
  turn.
- [x] Establish protected no-backlog scheduled ordinary-execution admission.
- [x] Keep queued accepted-input admission coherent with active and completed transcript builds by
  preserving unchanged selected-path work across path-neutral broad thread revisions.
- [x] Correct accepted-promotion reconciliation to authenticate its immutable witness across
  compatible monotonic descendants.
- [x] Mount bounded scheduling and dispatch of promoted pending turns.
- [x] Recovered accepted delivery work across restart through bounded gate recovery, no-replay
  active convergence, durable terminal-history finalization, and recovered-pending scheduler
  handoff.
- [x] Mounted live awaiting-terminal authority required by the exact stop-gate replacement.
- [x] Mounted exact version-scoped backend interruption and coarse-cleanup authority.
- [x] Exposed the complete keyed exact stop-admission authority required by app coordination.
- [x] Integrated exact stop coordination, hard-stop escalation, and context compaction as
  independently planned slices.
- [x] Reconciled intrinsic thread-property authority in Syndic, including immutable execution,
  title/archive attributes, usage, history-derived title, compact summary, and exact catalog-search
  normalization contracts.
- [x] Replace the obsolete Beryl thread-metadata domain with final Syndic V5 intrinsic-property
  records and cut non-GUI scheduler, title, usage, archive, and catalog-row consumers directly to
  that authority.
- [x] Integrate bounded canonical-content title derivation, compact Syndic summary rebuild, and the
  Beryl catalog-source join without adding maintenance, query, shell, or compatibility mounts
  assigned to Checkpoint 4.
- [x] Build the bounded all-or-nothing pending-projection quarantine over one sealed running-service
  failure inventory, including exact registry, target, barrier, and late-publication ownership.
- [x] Replace Beryl's exact CAS target atomically with 0.146.0, including refreshed retained
  compatibility evidence and native `spawn_agent` model/reasoning selection, without a dual-version
  branch, compatibility adapter, or Beryl-owned spawning tool.
- [ ] Mount running-session same-home service recovery against the final Syndic and Beryl handles,
  including bounded failure quarantine, connection-scoped closure, all-or-nothing service-epoch
  adoption, terminal whole-attempt disposition, preserved durable admitted-work authority, and
  stable-core-gated startup publication through the existing startup fence.
- [ ] Gate: complete storage, protocol, concurrency, restart, risk-bound stress, and independent
  review before Checkpoint 4 because the shell must consume final storage and execution boundaries.

## Checkpoint 4: Build The Multi-Window Shell And Navigation

- [ ] Cut app-owned diagnostic, settings, theme, and GUI-control dynamic tools to their final
  feature-owned incremental registry sinks and bounded shell bridges.
- [ ] Implement independent main windows, exact thread claims, ordinary close versus Exit,
  restoration, and a practical process window-count limit.
- [ ] Implement runtime and root creation, Host/WSL identity, zero-runtime onboarding, empty restore,
  and eligible empty-thread reuse.
- [ ] Implement progressive bootstrap and independent catalog, history, draft, and CAS readiness.
- [ ] Implement revision-bound paged catalog, search, lineage, activity, model, navigation-history,
  and composer-history sources with virtualized presentation.
- [ ] Implement the external range-backed multiline text-input and revision-bound paged
  settings-window source contracts without whole-value compatibility buffers.
- [ ] Mount bounded draft editing, autosave, clipboard paste, export, submission preparation, and
  coherent activation through the target shell.
- [ ] Integrate transcript residency, chunked realization, nested widgets, layout, snapshots, media,
  and coarse CPU/GPU cache budgets without a universal process resource runtime.
- [ ] Mount the native-lineage recovery prompt and accepted loading, failure, and readiness surfaces.
- [ ] Gate: verify multi-window lifecycle, activation, bounded repeated surfaces, large drafts,
  and configured working-set behavior before Checkpoint 5 because branch handoff depends on final window and thread
  ownership.

## Checkpoint 5: Implement Branch Discussion And Resolution Handoff

- [ ] Implement branch discussion creation, immutable selection provenance, readonly context, first
  submission, and ordinary child conversation.
- [ ] Implement resolution admission, queued-input deferral, durable parent handoff, busy-parent
  ordering, restart recovery, idempotency, and retry.
- [ ] Implement successful-handoff archive and accepted post-archive navigation and failure states.
- [ ] Prove production child creation, inherited image-label authority, and exact branch-local label
  allocation without copying historical label maps.
- [ ] Gate: verify no premature CAS work, lost input, duplicate handoff, or early archive before
  Checkpoint 6 because production child lineage supplies inherited asset authority.

## Checkpoint 6: Implement Assets And Deferred Cleanup Boundaries

- [ ] Implement Beryl-home image admission for paste and generated output, content-addressed
  sidecars, labels, references, Host/WSL projection, and generated-output ownership.
- [ ] Integrate bounded file reads, header parsing, thumbnail and tile decoding, CPU surfaces, upload
  staging, shared media identity, GPU residency, eviction, and device-loss recovery.
- [ ] Mount generated-title maintenance and successful branch-archive presentation through their
  already-established Syndic authority and bounded Beryl projections.
- [ ] Remove graph-dependent semantic-search implementation while leaving future graph-independent
  search non-authoritative and unimplemented.
- [ ] Preserve unreachable turns and resources until a separately designed future garbage-collection
  operation.
- [ ] Gate: verify arbitrary asset size, adversarial dimensions, concurrent windows, cancellation,
  and device loss before Checkpoint 7 because closure must include the final media paths.

## Checkpoint 7: Integrate, Harden, And Close The Rework

- [ ] Reconcile final root, feature, system, package, GUI, settings, hotkey, diagnostics, and source
  authority after implementation exposes the complete target state.
- [ ] Remove every remaining shim, obsolete export, test, key, diagnostic, role, archived-source
  membership edge, and forbidden API reference.
- [ ] Run end-to-end storage, runtime/CAS, multi-window, conversation, branch, asset, recovery,
  performance, and Windows verification.
- [ ] Audit every external-source-to-visible-consumer path for whole values, proportional copies,
  unbounded queues, clone-heavy snapshots, local governors, and missing release edges.
- [ ] Prove fixed CPU and GPU high-water marks under synthetic arbitrary logical sizes, traversal
  distance, repetition, and window churn.
- [ ] Obtain independent architectural completion review and close every finding before archiving
  this tracker under the project convention.
