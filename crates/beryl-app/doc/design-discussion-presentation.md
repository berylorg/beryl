# Discussion Presentation

This supplement owns the package-local selected discussion observation, composer-gate projection,
and explicit retry adapter. Product states and commands remain in the
[branch-discussions feature](../../../doc/features/branch-discussions/design.md); composition remains
in its [GUI supplement](../../../doc/features/branch-discussions/gui.md). Durable coordination and
outcome semantics remain in the [handoff system](../../../doc/systems/branch-discussion-handoff/design.md)
and the existing [admission/settlement adapters](design-feature-adapters.md#discussion-resolution-admission).

## Process Ownership And Published Capability

- The published graph exposes one generation-qualified weak discussion presentation capability
  over its original State, Syndic and `DiscussionSettlementOperations` owners. A window never
  constructs another settlement service, operation pool, coordinator or reconciliation registry.
  An admitted worker may temporarily acquire the existing service lifetime; snapshots and retired
  adapters retain no Home, graph, repository, GPUI or execution capability.
- The capability supplies bounded observations and exact retry preparation. Presentation values
  are descriptive facts, not mutation or CAS authority. Retry still enters the original
  `DiscussionSettlementService::prepare_retry` and its original command/outcome path.
- One selected-window adapter retains one current snapshot, at most one coalesced refresh and one
  explicit retry flight under existing window/worker and handoff-operation limits. Hidden target
  preparation uses the existing selection preparation budget. There is no selected-job collection,
  unbounded subscription list, second mutation queue or new persistent record.

## Coherent Bounded Observation

- Observation joins the exact selected thread, discussion owner/parent, archive attributes,
  discussion/input gate, State latest-attempt pointer and named job. It performs bounded typed
  reads off GPUI, retaining at most one job and compact source facts. Ordinary inherited context
  without a discussion owner does not make a thread a branch discussion.
- The same join supports explicit unpublished candidate access for initial/restored/recovery
  preparation. It uses the ordinary/candidate typed readers and identical validation, bounds and
  failure classes. Archive comes from the authenticated Syndic attributes, never from State's
  succeeded checkpoint alone. Missing or disagreeing source identities cannot become Open.
- State/Syndic source revisions or an equivalent coherent Home observation fence the complete
  join. A separate bounded process observation fences local operation/audit changes. Home changes
  and local outcome changes both trigger existing selected-status refresh work; a local Collision
  must become visible even when no durable write occurs. In-memory revision mutation intervals
  cover only short registry changes, never storage I/O or a command's whole lifetime.
- The local observation inspects the already configured-slot-bounded operation/audit registry by
  child and exact attempt. Admission witnesses identify that child before any durable job or
  latest-attempt pointer exists. Transition witnesses identify the same child through retry,
  settlement and archive. Extract only compact identities, pending/disposition facts and bounded
  failure projections; do not clone resolution payloads or retain original outcome descriptors.
- The observer correlates an existing job with the exact latest attempt, child/parent provenance
  and required gate/archive facts. Where the parent execution phase matters, use Syndic's typed
  exact parent-execution observation. Its existing typed request may temporarily own a copy of the
  already-read State job's bounded resolution (at most the existing 65,536 Unicode scalars), off GUI
  and outside local registry locks. That request is released with the read; no payload enters the
  snapshot or remains in a selected adapter. The local-registry extraction rule above still forbids
  cloning retained audit payloads. Starting-parent alone cannot prove CAS acceptance; accepted
  execution may precede State's parent-active checkpoint. A proven terminal parent awaiting its
  State/archive settlement remains resolution-pending, without claiming archive or an active CAS
  turn. Source failure or drift preserves the prior coherent presentation and grants no new action;
  it is not itself proof of the product's terminal Collision state.
- Pending original admission preserves the last coherent Open presentation while its dependent
  mutation gate remains closed. A retained Collision overrides otherwise Open, failed, or succeeded
  durable records with Unavailable for that exact attempt. It cannot expose retry, archive,
  resubmission or another resolution. A retained unresolved retry/settlement likewise cannot enable
  another action just because the durable job still has its previous checkpoint.

## First Frame, Refresh And Replacement

- Initial, restored, ordinary target and fresh recovery preparation carry the small immutable
  discussion snapshot and composer contribution in the common prepared selected-composer transport.
  Preparation occurs before selected-shell publication; constructors consume prepared values and
  never add storage reads to GPUI. An initial discussion does not briefly appear as an ordinary
  ungated thread while a later query discovers its status.
- Ordinary selection installs status, retry availability and composer contribution inside the
  same existing selection/transcript/composer publication election. Fresh recovery stages them with
  the recovered claim target until the existing coherent replacement publication. Failed or stale
  preparation cannot publish half of that tuple or replace the prior selected editor.
- Recovered resident and failed-resident publication refreshes the staged tuple off GUI through
  the existing bounded selected observer before its final release. All participating prepared
  tuples must share the exact weak service, local revision and opaque Home observation interval.
  The original recovery admission reopening elects that Home observation with the local election
  outside its command permit, and applies the prepared resident publications within the same Home
  election. All source/I/O, appearance, protection and native/GUI readiness checks precede the
  local election. Exact selection-release intent is prepared there too; its final short registry
  ownership election follows the existing CAS-to-process lock order and precedes Home election.
  Busy or stale ownership refuses before mutation, without waiting while holding those guards.
  The Home callback performs only the prepared bounded in-memory changes and reopening. Refusal retains
  original resident/claim custody and publishes no actionable stale frame.
- Live refresh uses one accepted observation for the strip and composer gate in one GUI update.
  Every delivery checks exact Home/service, window/selection and composer publication identities.
  Thread switch, disposal and replacement cancel/fence old demand and reject late updates.
- Status changes preserve the existing editor, local draft, selection and accepted publication
  custody. Inert and archived-readonly contributions use the existing composer mutation/focus
  gates; renderer styling alone cannot disable input. Ordinary writable presentation remains
  subject to every other composer gate.

## Explicit Retry And Original Outcome Custody

- An enabled retry descriptor names the exact displayed child, latest job and captured job revision
  with its Home/service and selected-window publication scope. It changes no payload, parent,
  accepted input, turn or attempt identity. The service reauthenticates current retryable state,
  pending child gate and original source provenance before preparing the existing command.
- Activation synchronously suppresses duplicates and keeps the same action visibly pending and
  disabled with its closest pending reason. Stale callbacks, foreign generations and nonretryable
  snapshots refuse before command admission. No automatic refresh, scan or failed invocation
  invokes retry.
- After admission, original command/outcome custody belongs to the existing process owners.
  Cancellation, widget removal, selection change or caller disposal cannot erase an original
  committed, indeterminate, local-finalization or unsuccessful-close capability. Registry and
  natural-scope reconciliation must agree before dependent work or another retry becomes available.
  UI completion checks its original presentation scope and cannot update a successor selection.
- A committed retry's retained later failure or local-finalization capability is not released by
  ordinary exact-new observation. It may retire after genuine fresh same-Home recovery proves exact
  natural/registry settlement, disposes the original physical Home attachment, and the exact original
  presentation owner has expired. Owner expiry alone, a numeric generation change or a revoked
  presentation cannot establish that disposal. This uses the original bounded attempt slot and
  creates no separate retirement queue or reconstructed capability.
  Recovery qualifies this retirement through the actual typed `HomeRecoveryCandidate`, matching the
  original Home/path and opaque opening identity against the fresh candidate before releasing its
  retained attempt. Borrowed generic candidate access, which also supports initial opening, may
  classify the outcome but cannot alone authorize this release. The original recovery graph runs
  this bounded qualification over existing retained slots before handing off its recovery prefix.
- That prefix also reconciles retained ordinary admission and settlement audits, including an
  admission whose job is not yet durably visible, before live-job convergence acquires their
  slots. It uses one original audit at a time within the configured reconciliation-slot bound;
  exact noncommit or settlement releases ordinary custody. Pending or Collision evidence remains
  retained and blocks the dependent recovery prefix rather than being discarded to free a slot.
- Proven retry commitment wakes the existing coordinator through its ordinary route. Proven
  noncommit leaves current durable authority intact and reports the bounded command failure.
  Uncertainty remains pending; Collision remains terminally Unavailable. Neither causes duplicate
  delivery, another job, automatic selection change or an invented successful archive.

## Mounted Presentation And Notices

- The feature mounts the bundled segmented status bar in its existing discussion-status slot.
  It has one passive segment and at most one direct-action segment: a statically bounded full
  render with no list, independent scrolling, popup or keyboard-focus model. Geometry, state
  styling, truncation and canonical expanded roles follow the bundled widget specification and
  theme runtime. Feature labels and state mapping remain in the feature GUI authority.
- Long failure/Unavailable detail contributes one bounded persistent condition record through the
  existing Notifications arbiter, with stable selected-attempt identity and ordinary feature-error
  priority. The feature owns eligibility/content; Notifications retains its priority, capacity,
  preemption and single-visible-notice rules. There is no competing overlay or detail expansion
  inside the fixed-height strip. Same-condition refresh updates one contribution; ending or
  retiring that selected scope removes only its own contribution.
- Notice text uses bounded user-facing failure projections, preserving explicit truncation where
  required. The notice adds no Resolve, Archive, retry/resubmit on Unavailable, rollback or manual
  repair command. It cannot dismiss or mutate the underlying job merely because presentation ends.

## Verification Boundary

Qualify the full selected workflow, including first-frame/candidate publication and coherent
six-state/composer changes, pre-job admission Collision, accepted parent before State checkpoint,
terminal settlement before archive, exact same-job retry and pending suppression, stale callbacks,
switch/recovery/disposal, local-only outcome changes, original uncertain custody and bounded release.
Reuse unchanged settlement/notice/widget evidence only within its qualified inputs; test the mounted
integration and normal production graph separately. This supplement adds no provider/model-run or
paint/performance qualification by itself.
