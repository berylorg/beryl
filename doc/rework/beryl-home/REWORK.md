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
- [Runtime/root admission and first-editor recovery](../../../crates/beryl-app/doc/design-runtime-root-admission.md)
- [Beryl backend package](../../../crates/beryl-backend/doc/design.md)
- [Beryl home-store package](../../../crates/beryl-home-store/doc/design.md)
- [Beryl model package](../../../crates/beryl-model/doc/design.md)
- [Beryl state package](../../../crates/beryl-state/doc/design.md)
- [Beryl stream package](../../../crates/beryl-stream/doc/design.md)
- [Syndic storage package](../../../crates/syndic-storage/doc/design.md)
- [Syndic draft storage](../../../crates/syndic-storage/doc/design-draft-storage.md)
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
- Explicit-incomplete recovery closes the proven-unavailable source boundary; repair adapters and
  their snapshot/media staging, claim and dispatch are excluded from the current cutover scope.
  Future eligible historical repair requires separately authorized source eligibility and complete
  admission before any dispatch; no media-less fallback conceals an implementation gap.
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

- [x] Closed: independently accepted the composed multi-window shell, navigation, transcript/composer, Settings and recovery product flows with configured limits, original custody and owned-resource release; [qualification](../../audits/checkpoint-four-product-flow-qualification.md).

## Checkpoint 5: Resolve Incomplete Turns And Fresh Same-Home Recovery

- [x] Explicitly driven selected-window and sole-threadless recovery composes resident/service retirement, fresh same-home preparation, attachment, publication, activation and coherent completion under exact-request admission.
- [x] The running owner retains service configuration, selected inputs, appearance and partial progress across abandoned waits, with initial, retired and prepared continuations that avoid replaying completed stages.
- [x] Failed candidate and service preparation return home custody before typed outcome transfer and bounded retry, preserving original Exit evidence, resume outcomes and exact reconciliation custody.
- [x] Fresh selected-resident admission and adoption validate source, history, generation and resource limits while preserving native identity, input, selection and renewed draft tickets.
- [x] Publication and completion retain fences through stale, competing, cancelled and dropped waits, partial binding and pending mount cleanup, with independent component lifecycle and persistence review.
- [x] Automatic interrupted-Exit recovery runs from reported failure through coherent completion with fresh production inputs, bounded task lifetime, exact cancellation/disposal and independently accepted native evidence. Ordinary close/Exit mounting and executable composition remain separate.
- [x] Established exact pinned evidence requiring unavailable repair for the current supported thread population, with the [source limitation](../../failures/cas-terminal-repair-full-view-is-not-completeness.md) retained.
- [x] Accepted ordinary and candidate startup recovery through explicit incomplete history, with retained repair provenance, bounded finalization, reconciliation custody and gate release. Product mounting and fresh-service replacement remain separate.
- [x] Retired the unprovable-source repair adapter and dependent snapshot, media, claim and dispatch work in favor of accepted explicit-incomplete recovery; [policy](../../systems/cas-live-syndic-transcript/design.md#exact-terminal-turn-historical-repair).
- [x] Enforce repair-required successor gates before mounting dispatch while unrelated threads
  remain independent.
- [x] Accepted bounded explicit-incomplete finalization and coherent publication before releasing repair-required gates while preserving durable request disposition; storage and startup regression evidence passed independent semantic review.
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
- [x] Reconciled accepted complete graph and interrupted-Exit evidence against the remaining ordinary running-home supervisor and runtime Retry gaps; [readiness](../../failures/executable-bootstrap.md#running-session-recovery-mounting-readiness).
- [x] Established ordinary Running-session recovery custody in the app lifecycle and composer supplements with independent readiness review; [evidence](../../failures/executable-bootstrap.md#ordinary-running-home-recovery-custody-readiness).
- [x] Accepted ordinary running-home recovery through complete old-stack disposal, fresh same-home convergence and atomic publication with preserved windows and no Exit request; [qualification](../../failures/ordinary-running-home-recovery.md#mounted-qualification).

## Checkpoint 6: Implement Branch Discussion And Resolution Handoff

- [x] Specified process-owned handoff coordination, candidate/runtime scan separation, exact
  branch correlation, bounded job reads and Unicode payload limits, parent ordering frontier and
  transition custody. Independent readiness review accepted correlation and candidate-read
  prerequisites; concrete atomic participants remain separate before coordinator implementation.

- [x] Accepted non-GUI handoff coordination and complete-graph retained-proof shutdown/recovery custody, with child execution and status/Retry mounting remaining separate; [qualification](../../audits/non-gui-discussion-handoff-qualification.md).
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
- [x] Mounted bounded readonly discussion context with exact provenance, themed realization and
  Copy-only interaction; [acceptance](../../audits/transcript-mounting-readiness.md#synthetic-discussion-context-acceptance).
- [x] Accepted finite discussion-context proof and replay authority before ordinary execution; [readiness](../../audits/discussion-context-projection-readiness.md).
- [x] Accepted ordinary discussion execution through exact native seed, continuation/resume and complete context replay, with bounded proof/page and failure custody; [qualification](../../audits/discussion-context-execution-qualification.md).
- [ ] Implement branch discussion creation, immutable selection provenance,
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
- [x] Accepted mounted coherent discussion status/composer/notice and exact same-job Retry, including first-frame/recovery fencing, original outcome custody and readonly session allocation; [qualification and evidence limits](../../audits/discussion-presentation-qualification.md).
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
