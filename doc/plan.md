# Scope

The earlier code simplification and behavior-based naming batch is complete. The Operator has
accepted using the [complete simplification audit](audits/code-simplification/report.md) selectively
within the Beryl-home architectural rework tracked by `doc/rework/beryl-home/REWORK.md`. The Operator
has resumed implementation; host request sequencing and submission quiescence progress are now
accepted. Already-durable composer integration acceptance is the next bounded phase.
The Operator authorizes continuous implementation until a blocker requires attention, with a
commit after each accepted phase. Temporary and obsolete directory cleanup is authorized for
the remainder of this thread; preserve unrelated work and verify exact cleanup targets.
Protect concrete supported-envelope consequences without duplicating dependency
guarantees, runtime validation, or review machinery.

The audit and its [canonical findings](audits/code-simplification/estimates.tsv) are supporting
evidence, not design authority or a second implementation plan. Its line estimates are not delivery
targets, and completing every proposal is not a rework completion gate. Before activating a bounded
rework slice, inspect the relevant findings against current source and owning target docs; the audit
describes a frozen baseline and does not establish that a finding still applies after later work.

Complete the retained composer integration acceptance gaps, then exact close-gate release and
resident-preserving close-flush acceptance in the phases below.
Resolve relevant correctness and test-evidence findings in their owning acceptance boundaries.
Include a simplification within an existing phase only when it serves that phase's same acceptance
boundary and cannot be independently implemented, verified, reviewed, or resumed. Otherwise derive
a separate bounded phase before starting it; place optional cleanup between accepted slices only
when it reduces the work or uncertainty of reaching the next milestone. Broad restructuring of
unstable or soon-replaced code remains deferred.

Keep later ownership and algorithm work in its existing rework milestones: marker-service discovery
removal accompanies bootstrap composition, and persistent-tree rebalancing sharing follows stable
editor behavior. Contract-changing audit alternatives require decisions in the owning feature,
system, package or GUI authority before implementation phases can be derived. A parser feasibility
investigation, if selected, has an evidence-only acceptance boundary and does not authorize the
production rewrite. No such alternative is selected by this policy update.

For each selected finding, the phase records its ID, retained contracts, dependencies and concrete
verification derived from the owning engineering-rigor requirements. After acceptance, record the
finding's disposition and actual implementation/verification evidence in the audit, retain its
baseline estimate, and compact the phase and corresponding rework outcome before advancing. Leave
unselected proposals in the audit rather than expanding them into speculative pending phases.

The replacement shell is the final target-state composition boundary, not a compatibility shell or
a reduced copy of the archived workspace-era view. It ultimately mounts every declared main-window
slot and feature contribution, including theme roles, toolbar and lineage, transcript and its owned
scrolling, optional activity and discussion surfaces, composer, status line, overlays, notices, and
Settings entry. Reuse accepted live target-state services, hosts, projections, and widgets; keep
unimplemented mounts visibly absent or unavailable until their owning bounded phase completes.

Use the accepted, canonically pinned single GPUI dependency graph and atomic GPUI window-set
appearance publication when shell implementation resumes. Keep startup restoration,
onboarding, placement, close, Exit, later catalog/navigation/activity/status/notice/settings/
transcript mounts, repair, recovery, branch, asset, integration, and closure boundaries in the
active rework tracker until their own bounded slices are ready.

Marker-operation admission and visible refusal, diagnostic activation, and compact repair-media
implementation and acceptance remain in their owning
rework checkpoints; updating target authority does not mark those behaviors implemented.

The earlier cleanup batch closure and composer request/wait prerequisites are accepted. Preserve the compiled, unaccepted
composer implementation and its explicit verification gaps below. Use the smallest implementation
that fulfills the high-level feature contracts; ordinary close mounting, bootstrap and other later
boundaries retain their own acceptance gates. Phase 307 still requires final integration verification
and its own completion review.

The naming policy is in `AGENTS.md`. Historical rework snapshots and immutable investigation
artifacts remain historical material. Update live source references without rewriting historical
execution evidence. Theme scope follows the updated Theming feature, theme-runtime system, and
state theme-service contracts. Bootstrap-dependent ownership and unstable editor-algorithm
consolidation remain explicit later rework work; resolve their target decisions before activating
their owning slices.

# Phase 318: Yield While Mounted Submission Awaits Editor Quiescence (finished)

Submission now yields through one retained timer, prepares the newest settled same-editor state,
and fences cancellation, stale callbacks, replacement and terminal editor errors. All 13 mounted
submission cases, both locked app library configurations, formatting and independent semantic
review passed. [Quiescence evidence](failures/mounted-submission-quiescence.md) records exact
verification and the corrected empty-rejection assertion. No newly owned temporary resources remain.

# Phase 307: Integrate Already-Durable Composer Openings (pending)

Use Phase 304's exact correspondence for clean host state, flush, final disposal, and submission
under [composer behavior](features/composer/design.md) and
[the app composer contract](../crates/beryl-app/doc/design-catalog-and-composer.md).

- Keep the host's dirty presentation local while recognizing an unchanged opening as clean. Derive
  it from validated host state without introducing another request or lifecycle owner.
- Route clean flush through existing off-GUI capture/advance work and exact storage authentication
  before satisfying its barrier. Preserve pending publication custody and invalidate captured
  readiness after later adoption, selector drift, or replacement.
- Adapt the native-lineage disposal caller to perform its capture and storage advancement in
  background work with exact selection/flush completion fencing and existing custody. Remove its
  fabricated clean-disposal shortcut without moving storage work onto the GUI thread.
- Submission retains the exact live candidate and durable selector separately, materializes the
  authenticated durable root, and uses accepted storage contribution/status/reconciliation paths.
  Final disposal reuses the accepted ordinary command and its normalized opening receipt.
- Verify untouched empty and populated drafts, reopened nonzero generations, no publication effects
  from clean flush, submission, final disposal, later edits/history, stale capture rejection, and
  focused lifecycle regressions. Run the affected locked check, formatting, and independent semantic
  review before acceptance.

Readiness confirmed that local clean-state shortcuts in `begin_flush` and `flush_state` must defer
readiness until the existing worker path authenticates storage. No new service or persisted record
is required. The native-lineage adapter audit also identified its clean shortcut and synchronous
disposal advancement; those same-flow integration corrections are included above. Request numbering
is an earlier prerequisite; exact close-gate release retains its later acceptance boundary.

Resumed checkpoint: the host integration and native-lineage adapter are implemented but unaccepted.
The adapter runs capture, advancement, and released-widget completion on one background task,
preserving exact selection/flush fencing and real widget-release proof. Drop uses detached bounded
cleanup rather than synchronously taking the storage-held slot lock. The locked app library check,
targeted formatting, and source diff check passed. This phase remains unaccepted.

- All seven `saved_opening` and 28 `composer_lifecycle` cases pass after correcting test handle
  ownership, explicit clean authentication, marker-readiness setup, flush timer expectations and
  typed pre-admission rejection assertions; stale-capture tests retain durable-state and exact
  service-custody release checks.
- Audit AM-008's mounted-test module wiring is corrected. Three existing native-lineage mounted
  cases and a new actual disposal-reconciliation/drop case pass, including real widget release,
  eventual weak-service release, exact terminal session state and unchanged durable draft.
- Independent source review found no blocker in clean capture, submission fencing or native
  disposal custody. The 32-step cleanup bounds advancement attempts rather than promising terminal
  completion; installed ambiguous commands remain independently owned by HomeStore. Final review
  of stable test changes and complete regression evidence remains pending.
- All 13 `mounted_composer_submission` cases pass after the accepted request-sequence and
  quiescence prerequisites, including the eight existing scenarios and five new waiting/cancellation
  regressions. Empty rejection follows the typed pre-acceptance `Empty` error through the ordinary
  `Failed` path; corrected assertions prove unchanged durable/editor state and released custody.

The locked app library check and targeted formatting pass. All 39 saved-opening, lifecycle and
selected native-lineage cases passed together in the final rerun. Bounded timed-out test processes,
exact residual fixture homes and temporary diagnostics from that verification were reclaimed.
AP-007/AM-007 request sequencing and submission-start quiescence are accepted. Run the integrated
regression selection against those prerequisites and complete independent review of the retained
source and test changes before accepting this phase. Optional fixture consolidation and other audit reductions
remain unselected.

# Phase 306: Consolidate Exact Close-Gate Release (pending)

Share the exact slot-release and service-reservation retirement decision used by foreground,
background, and unmounted cleanup paths. Preserve their scheduling, bounded cleanup, independent
interaction restrictions, and actual widget-release proof under
[the app composer contract](../crates/beryl-app/doc/design-catalog-and-composer.md). Verify stale
release, pending publication, and failed-disposal return through the existing focused close tests
and independent semantic review.

# Phase 302: Preserve The Resident Editor Through Close Flush (pending)

Establish and verify a close flush that freezes mutations while preserving the resident editor and
read-only interaction until later close obligations settle, under
[composer behavior](features/composer/design.md) and
[ordinary close](features/main-windows/design.md). The existing WindowClose flush disposes the
editor before a subsequent session failure can be known; the
[readiness finding](failures/main-window-close-readiness.md) records why it cannot be used unchanged.

- Keep WindowClose publication separate from final editor disposal. Retain the exact mounted
  editor, candidate and history authority, caret, selection, and scroll state while later close
  obligations remain unsettled; suppress new draft mutations and submission while permitting
  coherent read-only interaction.
- Provide exact close-attempt settlement: later failure releases only the close gate and preserves
  independent unavailability, while final success may dispose the saved editor. Duplicate or stale
  requests must not repeat publication, release another attempt, or dispose a replacement editor.
- Reuse the existing captured-save, supersession, and reconciliation paths. A pending or ambiguous
  save cannot satisfy the barrier; proven noncommit preserves the dirty editor, and terminal
  unavailability preserves evidence without authorizing a retry.
- Verify clean and dirty drafts, an already-running save and admitted edit, repeated activation,
  failed later obligation, final disposal, stale settlement, independent gates, and mounted
  selection/copy/scroll with mutation rejection. Run focused locked nextest regression targets,
  the affected library check, formatting, and independent semantic lifecycle review.

Ordinary OS-window close, active-work coordination, session removal, and application Exit remain
outside this prerequisite and in their owning later slices.

Resume after Phases 304, 307, 305, and 306. The resident close implementation and eight acceptance tests
are retained. The previous locked app library check and targeted formatting passed, and semantic
source review completed, but focused acceptance remained failing and the planned regression
targets have not run. The [close finding](failures/main-window-close-readiness.md) records the
publication and request-sequence defects and the previous verification limits.

# Phase 298: Mount Ordinary Main-Window Close (pending)

Mount ordinary close under [main-window behavior](features/main-windows/design.md), preserving
the visible window and claim until exact active work, dirty draft, and durable session removal
settle. Await the notice and resident-preserving flush prerequisites above. Verify duplicate close
admission, exact stop and continuation cancellation, failure return
to the coherent open state, independent-window preservation, and final-window empty-restore
termination. Inspect accepted stop, composer, session, and notice dependencies before activating;
split any independently missing component into its own prerequisite phase. Exact-stop and typed
session-removal primitives exist; noninterruptible active-work waiting and close-time continuation
cancellation still require a focused readiness check before activation.

Startup, Exit, restoration, onboarding, and the other deferred mounts remain in the rework tracker.
