# Reason For Investigation

Operator requested a complete investigation of replacing CAS with a Beryl-owned, personal-Pro
subscription runtime. This inventory distinguishes required product behavior from current
provider mechanisms and unfinished work before estimating replacement scope.

# Outcome

Replacing CAS transfers an agent runtime, not just an HTTP connection. Beryl must account for
authentication, model/limit discovery, execution, context, local tools, sandbox/policy,
instructions/configuration, skills, MCP and subagents, plus Beryl-specific application tools.

All 16 root feature entry points and all 10 root system entry points were covered against
baseline `bea3f35b7f49645f98541e3f5faafa28a1e4c781`. Findings below are local authority/source
evidence, not observations of the subscription API or new approved design. The production hold
remains in force. Current root CAS-only/non-reimplementation decisions need explicit reconciliation
before replacement implementation.

## Feature Coverage

- **Beryl home:** retain exclusive ownership, validated startup, coherent failure presentation and
  same-home recovery. CAS retirement mechanisms change, durability does not.
  Source: `doc/features/beryl-home/design.md`, Home Opening, Persistent Store Failure, Recovery Boundary.
- **Main windows:** retain process-owned background execution after nonfinal close; final close
  and Exit freeze dispatch and settle work while preserving their different restore semantics.
  Source: `doc/features/main-windows/design.md`, Ordinary Window Close, Application Exit.
- **Backend recovery:** retain explicit affected-target unavailability, readable history/drafts,
  exact target identity and no duplicate delivery. Native-lineage recovery UI is a CAS mechanism
  to replace or remove explicitly. Source: `doc/features/backend-runtime-recovery/design.md`.
- **Conversation threads:** retain durable identities, immutable execution binding, root scoping,
  occupancy, Running threads, coherent activation and branch/replacement behavior. Configured
  executable paths and CAS lineage need new dispositions. Generated titles remain independent
  maintenance with no global developer instructions, validated output and history-derived fallback.
  Source: `doc/features/conversation-threads/design.md`, especially Thread Titles.
- **Branch discussions:** retain exact selected assistant context, ordinary discussion threads,
  model-requested resolution, durable parent delivery and archive/retry ordering; this is not
  interchangeable with spawning a subagent. Source: `doc/features/branch-discussions/design.md`.
- **Composer:** retain draft durability, coherent acceptance/clear, ordered queued fragments,
  steering eligibility and delivery-unknown state. Hidden global instructions apply to top-level
  starts and lifecycle continuation, excluding steering, subagents, titles and compaction.
  Source: `doc/features/composer/design.md`, Submission And Queuing, Developer Instructions On User Turns.
- **Image assets:** retain stable labels, markers, previews, historical identity and generated-media
  admission/unavailability. Returned bytes or URLs are not automatically durable assets.
  Source: `doc/features/image-assets/design.md`.
- **Transcript:** retain canonical narrative, Markdown/media, exact selection/quote/branch provenance,
  scrolling and coherent activation. Tool activity, raw reasoning and hidden instructions do not
  become parent narrative. Source: `doc/features/transcript/design.md`.
- **Status line:** retain truthful model/reasoning selection, context/usage/limit information,
  compaction and exact stop. Unknown metadata stays unknown; no inference calls solely to decorate
  status. Define the replacement for backend parent-turn view counts.
  Source: `doc/features/status-line/design.md`.
- **Activity panel:** retain exact selected-thread/subagent identity, deterministic bounded
  retention and period invalidation; replace CAS event taxonomy explicitly.
  Source: `doc/features/activity-panel/design.md`, Activity Collection.
- **Settings:** retain validated drafts, Apply/reconciliation and availability. Authentication,
  backend configuration, skills and MCP settings are currently excluded: adding their UI is an
  explicit scope choice. Source: `doc/features/settings/design.md`.
- **Theming:** retain agent-facing schema/guide reads, validation, preview, install/update/Save As
  and active-choice staging, in addition to appearance behavior. Exact broker/revision and Settings
  Apply ownership survive replacement. Source: `doc/features/theming/design.md`, Theme Dynamic Tools.
- **Notifications:** retain bounded notices, exact stop feedback, lifecycle attention and bounded
  sound playback. Titles, compaction and automatic continuation have distinct sound eligibility.
  Source: `doc/features/notifications/design.md`.
- **Lifecycle yield:** retain four closed outcomes, one controlling yield per turn, compaction then
  fixed continuation, accepted-input precedence and soft-stop cancellation. Unaccepted continuation
  intent is process-local and must not be reconstructed after loss.
  Source: `doc/features/lifecycle-yield/design.md`.
- **Diagnostics:** retain bounded observation and exact isolated-child operations through ordinary
  product paths. Replace CAS-specific observations without bypassing gates or acquiring hidden
  history/media for display. Source: `doc/features/diagnostics/design.md`.
- **Crash reporting:** retain independent same-executable reporting, bounded fatal reports and
  failed-process termination. This requirement is execution-provider independent.
  Source: `doc/features/crash-reporting/design.md`.

## System Coverage

All paths in this section are under `doc/systems/`.

- `beryl-home-storage/design.md`: retain lock, typed domains, revisions, durability, sidecar
  ordering, free-space admission, reconciliation and generation health. Runtime/root bindings change.
- `syndic-conversation-history/design.md`: retain drafts, immutable parentage, resources,
  projections and integrity. CAS Live Source Boundary, Terminal Repair Snapshots and execution
  bindings prevent assuming unchanged schema reuse.
- `cas-live-syndic-transcript/design.md`: remove candidates include loaded projections, native
  lineage/fork/rollback, one-time injection and historical-repair eligibility machinery. Retain
  request custody, scheduling, capture, stop/compaction coordination and honest incomplete outcomes.
- `branch-discussion-handoff/design.md`: retain provenance, tool admission, durable jobs, exact
  parent input, busy-parent deferral and archive ordering; replace provider dispatch.
- `image-assets/design.md`: retain content addressing, references and atomic admission. Replace
  Pinned-Release Generated Output Admission based on `image_gen.imagegen`/`savedPath`. Resolve
  Host/WSL projection alongside execution-environment scope.
- `backend-runtime/design.md`: CAS executable/release admission, listener tokens, native sessions
  and separate maintenance connections may disappear. Transfer execution/configuration/lifecycle
  duties; Neutral Maintenance Roots encodes isolation still needed by title jobs.
- `transcript-presentation/design.md`: retain bounded residency, live-suffix reconciliation,
  async identity/disposal, viewport publication and renderer demand; focus change on producer adapters.
- `theme-runtime/design.md`: retain repository, appearance generations, preview arbitration and
  joined lifecycle; workers must not acquire GUI/Settings/repository authority from tool dispatch.
- `bounded-resource-dataflow/design.md`: retain limits at accumulation/amplification boundaries.
  Moving execution into Beryl enlarges its owned resource surface. Existing non-goals reject a
  global RSS theorem and exact accounting of every allocation.
- `crash-reporting/design.md`: retain the independent reporter and process-entry contract.

## Scope Distinctions That Affect Architecture

Current V1 **denies CAS approval requests automatically**, preferably with interruption. Root
`doc/design.md`, Cross-Feature Safety Rules, does not require interactive approval dialogs.
Sandbox/permission enforcement still needs an owner; adding approval UI is a separate decision.

Current root Persistence forbids Beryl storing backend-owned credentials, configuration, skills
and MCP state. Direct ownership requires a deliberate new storage/security boundary, not merely
putting these records into the existing home database.

Host/WSL execution identity currently includes an exact executable and working root. Removing CAS
does not decide which execution environments remain or how an immutable thread binding identifies
them. Root non-goals also exclude restricting the product to Windows only.

Context includes native continuation, reasoning and tool state, compaction and branch validity.
Rendered Syndic narrative cannot be assumed to reproduce the model context. Pending calls and
their results need valid pairing across stop, model change, branching and recovery.

Skills/MCP/subagents are delegated requirements rather than already implemented local subsystems.
An exact compatibility envelope needs investigation; neither cloning all Codex features nor
silently dropping integrations follows from replacing CAS.

Beryl dynamic tools are a separate retained integration boundary: lifecycle yield, branch
resolution, theme authoring and diagnostics require exact app brokers, not arbitrary tool-side
mutation of GUI/storage state.

## Source Anchors And Existing Gaps

- `crates/beryl-backend/src/session.rs`: `ManagedBackendSession`,
  `ManagedBackendReleaseAdmission`; `thread_lineage.rs` and `thread_injection.rs` identify
  provider-specific mechanics, not directories safe for wholesale deletion.
- `crates/beryl-app/src/cas_projection/service.rs::ProjectionConnectionService`,
  `service/initial_preparation.rs::PreparedCasServices` and `service/shutdown.rs` own
  preparation/retirement duties that need explicit replacement custody.
- `crates/beryl-app/src/cas_projection/process_tools.rs::UnavailableBranchResolution` returns
  explicit unavailability. Tool definitions and durable job records do not implement branch coordination.
- `crates/beryl-app/src/lifecycle_dynamic_tools.rs::LifecycleYieldRequestHandler` and
  `dispatch_beryl_lifecycle_dynamic_tool_request` separate semantic lifecycle behavior from dispatch.
- `crates/beryl-app/src/shell/thread_title/backend.rs::ThreadTitleBackend` starts native
  thread/turn work and unsubscribes; adjacent title code owns prompt/options/validation.
- `crates/beryl-app/src/cas_projection/connection/provider_broker/ingester/approval.rs`
  integrates approval obligations into exact operation custody.
- `crates/beryl/src/main.rs` contains an intentional `compile_error!` for unreconstructed
  bootstrap. There is no accepted complete executable graph to count as proven integration.

The active rework accepts storage foundations and many private bounded service components, but
still lacks complete graph publication, fresh same-home recovery integration, branch coordination,
substantial GUI mounting, image integration, title mounting, crash-entry mounting and final
end-to-end acceptance. Keep these pre-existing gaps separate from new runtime work when estimating
cost or claiming simplification.

## Investigation Consequences

The existing agenda covers the identified boundaries. Focus its later phases on exact turn/request/
tool identities; context versus transcript; retained execution environments; permissions without
invented approval UI; title and continuation jobs; app-tool brokers; credential/configuration
ownership; and the separation of accepted reuse from unfinished product work.

Completion review checked coverage against root entry points, backend Protocol Ownership and
remaining rework gates. Root independently verified the approval-denial rule, theme tools,
title-instruction exclusion and actual bootstrap/branch stubs. This inventory establishes research
coverage; each subsystem still requires deeper evidence before architecture selection.

# Sources

- Beryl local baseline `bea3f35b7f49645f98541e3f5faafa28a1e4c781`, inspected 2026-09-17:
  `doc/design.md`, all 16 feature and 10 system entries identified above,
  `doc/rework/beryl-home/REWORK.md`, and selected source anchors listed above.
- `crates/beryl-backend/doc/design.md` and its linked boundary documentation for delegated
  protocol duties; root/system authority governs cross-package conclusions.
- [Initial assessment](beryl-feasibility.md) used as a starting inventory, not proof of current
  server behavior. Read-only inventory performed by the research worker and integrated with
  targeted source/authority validation by root; no production files changed or builds run.
