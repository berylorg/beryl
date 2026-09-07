# Scope

The earlier code simplification and behavior-based naming batch is complete. The Operator has
accepted using the [complete simplification audit](audits/code-simplification/report.md) selectively
within the Beryl-home architectural rework tracked by `doc/rework/beryl-home/REWORK.md`. The Operator
has resumed implementation; host request sequencing, submission quiescence and already-durable
composer integration, exact close-gate release and resident-preserving close flush are accepted.
Ordinary-close mounting is next, subject to its active-work and continuation-cancellation readiness check.
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

Complete ordinary-close readiness and mounting in the phase below.
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

The earlier cleanup batch closure and composer opening/request/wait prerequisites are accepted.
Preserve the retained close implementation and its explicit verification gaps below. Use the smallest implementation
that fulfills the high-level feature contracts; ordinary close mounting, bootstrap and other later
boundaries retain their own acceptance gates.

The naming policy is in `AGENTS.md`. Historical rework snapshots and immutable investigation
artifacts remain historical material. Update live source references without rewriting historical
execution evidence. Theme scope follows the updated Theming feature, theme-runtime system, and
state theme-service contracts. Bootstrap-dependent ownership and unstable editor-algorithm
consolidation remain explicit later rework work; resolve their target decisions before activating
their owning slices.

# Phase 302: Preserve The Resident Editor Through Close Flush (finished)

Resident close now retains the coherent read-only editor through authenticated draft readiness,
releases only the failed attempt's gate and requires exact final authorization before disposal.
All 62 selected integration tests and independent lifecycle review passed; current locked checks
and formatting cover the unchanged source. [Resident-close evidence](failures/main-window-close-readiness.md)
records direct versus shared-path coverage. OS close and its active-work/session obligations remain separate.

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
