# Checkpoint Four Product-Flow Qualification

## Status And Boundary

Accepted on 2026-10-20 after independent whole-gate review against implementation through `5e063ae8`. This gate
qualifies the multi-window shell, navigation, transcript/composer, Settings and associated recovery
flows. It does not close the Beryl-home rework or accept remaining branch handoff, asset processing
and final hardening boundaries.

Controlling requirements are the root [product design](../design.md),
[main windows](../features/main-windows/design.md),
[conversation threads](../features/conversation-threads/design.md),
[composer](../features/composer/design.md), [transcript](../features/transcript/design.md),
[Settings](../features/settings/design.md) and
[bounded resource dataflow](../systems/bounded-resource-dataflow/design.md).
The gate composes their accepted owning qualifications without changing their supported envelopes.

## Product-Flow Composition

- Startup prepares a complete hidden native set from the published graph, validates readiness and
  desktop placement, then hands off through the process owner. Failure, Retry and Exit retain exact
  original cleanup and Home custody. Actual executable mounting, running close/Exit and failed-home
  replacement have later acceptance beyond their earlier component-only records;
  [bootstrap acceptance](../failures/executable-bootstrap.md#bootstrap-acceptance) and
  [ordinary running-home recovery](../failures/ordinary-running-home-recovery.md#mounted-qualification).
- New Window and New Thread preserve original claims, reservations, configuration commands and
  outcome custody through coherent publication or exact cancellation/recovery. Current Settings
  qualification covers ready construction waiting for the original appearance loan, exact native
  identity, cancellation before return, created-window close/Exit and recovered creation;
  [Settings integration](settings-workflow-mounting.md). Earlier creation qualifications retain
  their stated partial-run limits; they are not relabelled clean aggregate runs.
- The immediate frozen Thread Switcher, Back/Forward, lineage, model/reasoning and context/Compact
  controls compose through the selected window's exact source, generation and original admission.
  Bounded presentation does not grant execution permission; current/unavailable/elsewhere choices,
  failed admission and stale callbacks preserve coherent content and exact recovery;
  [switcher](thread-switcher-mount-qualification.md),
  [navigation](thread-navigation-mount-qualification.md),
  [lineage](thread-lineage-mount-qualification.md),
  [model](model-selection-mount-qualification.md) and
  [context/compaction](context-compaction-mount-readiness.md).
- The mounted transcript composes narrative, Code, Tables and synthetic context with source-qualified
  Copy/Quote, user menus, replacement editing and captured branch creation/Retry. The accepted mixed
  native workflow traverses real outer/nested scrolling, input, appearance/source invalidation and
  disposal. Later native composer finish qualification reruns that same mixed workflow after the
  stack correction; [whole transcript](mounted-transcript-workflow.md),
  [surface and command qualifications](transcript-mounting-readiness.md) and
  [current mixed native route](native-composer-finish-stack.md).
- Ordinary draft edits, submission, Undo/Redo, accepted-input history and clipboard operations retain
  exact roots, directed selection and original uncertain settlement. Recall uses authenticated sealed
  pages and bounded checkpoints; large-draft refusal preserves the prior successful cut and releases
  operation custody without a whole-draft resident fallback;
  [accepted-input history](composer-input-history-mounting.md) and
  [large-draft clipboard](composer-marker-feedback/large-draft-clipboard.md).
- Activity, notices, exact stop/status and completion audio remain finite projections and existing
  process-owned services. Activity releases hidden queries/pages, notices preserve exact actions and
  focus, and sound is best-effort with one terminal notification attempt. Settings publishes the
  coherent applied scalar source to ordinary instructions, compaction timeout, completion sound and
  autosave, including existing/new/recovered composer timer mounts;
  [Activity](activity-panel-mounting.md),
  [status/stop](../failures/executable-bootstrap.md#exact-status-mount-qualification),
  [audio](../failures/executable-bootstrap.md#notification-audio-qualification) and
  [Settings consumers](settings-workflow-mounting.md).

## Configured Limits And Release

Owning evidence qualifies configured caps, distinct refusal and release at the actual boundary;
this gate does not substitute a global RSS assertion or a combinatorial product of all limits.

- Frozen Catalog queries retain at most 32 collections, use bounded record/byte scans and pages,
  serialize worker access and keep release available under saturation. The mounted selector reaches
  the exact final row beyond one transport/widget page while retaining bounded realization;
  [query ownership and limits](frozen-catalog-query-qualification.md) and
  [mounted selector](thread-switcher-mount-qualification.md).
- Activity retains at most four pages and one demand, with visible rows and fixed overscan. Scroll,
  supersession, hidden state and retirement release page/query/model and tooltip ownership;
  [mounted Activity](activity-panel-mounting.md).
- Draft and transcript logical growth uses bounded pages, source identities, operation custody and
  resident generations. Representative 3 MiB draft clipboard cases use 4 KiB mutation pages,
  repeated size/capacity refusal, exact Undo and weak-owner release. Transcript qualification covers
  provider/panel disposal, cancellation and replacement; the owning audits preserve their exact
  test-local allowances and distinguish them from production operation profiles.
- The 256-main-window capacity aligns with 258 startup/runtime appearance slots, including the
  Settings auxiliary window and replacement slot. Window creation has finite reservations, original
  acquisition/abandonment custody, a 30-second
  publication deadline and settlement after cancellation. Repeated creation/release returns registry
  and appearance counts to baseline; [creation](../failures/main-window-creation.md). Settings adds
  one process-owned auxiliary window with no Session record, awaits its original exact destruction
  receipt before service cleanup and preserves separate main-window identity.
- Settings owns a finite five-key baseline/applied source and one window-wide draft. Saved and draft
  instruction limits are 60 KiB and 64 KiB respectively; atomic oversize refusal is qualified across
  mutation routes. Theme collections/roles remain revision-qualified bounded pages. Scalar Home and
  repository outcome custody remain separate through cancellation and fresh recovery;
  [complete Settings](settings-workflow-mounting.md).

## Evidence Reuse And Limits

No production input changes are introduced by this gate. The final Settings record independently
reconciles 462/462 distinct affected passes, its owning State/theme/window/consumer checks and the
current normal App/executable check. It includes the current ordinary startup/recovery/shell/native
interactions. The whole-transcript record retains its accepted 297-case combined, 189-case public
and five-case native-close evidence; accepted-input history retains its owning 317 distinct cases.
These totals describe separate owning scopes and are not summed into a new unique global count.

Earlier prerequisite records' statements that a later mount remained separate describe their
acceptance boundaries at that time. Later linked mounted/workflow acceptance supplies that
composition evidence. Earlier failed or interrupted packets remain diagnostic; only explicitly
qualified passing subsets and corrected cases are reused. Historical resident-close fixture
limitations are not converted into passing receipts by this gate; later ordinary-selection,
transcript and current Settings qualification supplies the corresponding composition evidence.

Native evidence qualifies the particular input, identity, publication, cancellation and destruction
routes it exercises. Simulated Settings behavior does not prove native Settings paint or sustained
scrolling, and measured frame corrections do not establish a universal native stack maximum.
Remaining full asset processing and final rework-wide resource/performance closure retain their
own later acceptance gates.

## Task-Owned Resource Closure

After Settings acceptance and termination of every owned build/test/debug process, twelve exact
fixture ledgers identified 361 unique task-owned temporary targets. Of these, 248 were already
absent and 113 were removed with `cleanup-dir.exe` after containment and reparse-point checks;
all 361 are verified absent. Two formerly ledgerless Code Save outputs were attributed by exact
source-specific file sets/body hash and frozen run chronology before cleanup. Ambiguous temporary
directories, shared build targets, unrelated memory and earlier denied cleanup targets were left
untouched. After the final gate reviewer consumed the retained evidence, the exact Settings task
directory and its ephemeral logs/configuration were removed and its absence verified.

Independent final review accepts the composition, configured limits and refusal, original outcome
custody and release boundary without a new production change or unsupported performance claim.
