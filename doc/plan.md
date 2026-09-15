# Scope

Production marker admission and canonical widget publication have passed acceptance with the LLVM, one-job,
no-normal-debug and nonincremental settings under the root
[technology decisions](design.md#implementation-technology). The Operator authorizes pushing projects and owned forks as needed for
this work, including the accepted widget revision and Beryl changes. Preserve required
checks and independent review while using conditional delegation and bounded evidence.

Keep GUI thread switching clean without overengineering. The two authorized non-GUI corrections,
marker admission and healthy scheduler-conflict handling, are accepted. The Operator's instruction to
continue implementation resumes process-provider composition and the remaining phases below.
The controlling contracts are [conversation threads](features/conversation-threads/design.md),
[backend recovery](features/backend-runtime-recovery/design.md), and the
[app package](../crates/beryl-app/doc/design.md). Complete process composition, Running threads,
final-window shutdown and other broader background-work requirements remain separate rework work.

The Operator authorizes continuous implementation until a blocker requires attention, with a
commit after each accepted phase. Temporary-directory deletion and obsolete-directory cleanup
remain authorized; verify exact targets and preserve unrelated or concurrent work. The active
Beryl-home architectural replacement remains tracked by [REWORK.md](rework/beryl-home/REWORK.md).
The former window-owned stop/wait plan is superseded. Previously accepted exact-stop,
continuation-cancellation, draft-flush and session primitives are reusable evidence, not authority
for stopping a background thread when a nonfinal view closes.

At every phase boundary, explain any blocker directly to the Operator and suggest concrete next
steps. Evidence links supplement that explanation rather than replacing it.

Apply the [simplification audit](audits/code-simplification/report.md) selectively within each
owning acceptance boundary. It is evidence, not authority or a second plan; preserve its baseline
estimates and record actual dispositions only for accepted selected findings. Separate an
independently implementable simplification or material scope growth before work begins. Keep
source names behavior-based and follow the canonical single GPUI graph. No compatibility shell,
universal resource governor, or compile-only substitute fulfills target behavior.

The final shell mounts every declared slot and feature contribution using accepted target
services and widgets. Leave deferred contributions visibly absent or unavailable until their
bounded implementation is accepted. Startup, restoration, Exit/close mounting, catalog,
transcript, Running threads, attention, approval-policy reconciliation, Settings, repair, recovery,
branch, assets and bootstrap remain explicit rework checkpoints. Preserve their separate gates.

The Operator selected [fatal crash reporting](features/crash-reporting/design.md) with a separate
reporter process and immediate failed-application termination, explicitly requiring restrained
complexity. The [system boundary](systems/crash-reporting/design.md) supersedes the proposed
in-process panic-recovery correction. Establish its bounded components before returning to CAS
regression reconciliation; final production mounting remains dependent on executable bootstrap.

The Operator authorizes defining and reconstructing target executable bootstrap. Establish the
private home candidate and its typed consumers, explicit recovery access and prepared service
composition before restore-set and native process-entry integration. Preserve each separate
acceptance boundary and the intentional removal gaps; complete registration alone does not accept
the service graph or visible startup.

# Phase 429: Diagnose Remaining Memtable Pressure After History Retirement (finished)

Accepted [remaining-pressure diagnosis](failures/syndic-draft-build-memtable-capacity.md#accepted-remaining-pressure-diagnosis):
physical charge equals aggregate active memtables plus the prepared batch prefix, with no pending
rotation, flush or compaction. Two unchanged-workload captures and independent review passed;
temporary probes were removed. Further correction must cover bounded aggregate-pressure progress
and snapshot-safe reclamation without depending on another application write. No production remedy
or app qualification is accepted by this diagnostic result.

# Phase 419: Restore App Construction Evidence With Initial Candidates (pending)

Apply the [app public boundary](../crates/beryl-app/doc/design.md#public-boundary) and
[initial publication contract](../crates/beryl-app/doc/design-shell-lifecycle.md#initial-service-preparation-and-publication)
to app-owned fixture construction.

- Convert shared, direct and library-test fixtures to unpublished candidates with complete exact
  Beryl-state, Syndic and applicable test-domain registration before explicit publication. Generic
  openers retain candidates; fixed complete fixture composers may publish their declared graph.
- Rebuild live handles on each fresh physical open, preserve initial versus same-home recovery
  distinctions, and keep rejected candidate cleanup and typed failure assertions intact.
- Preserve each fixture's services, identity, initial state and fault behavior. Reconcile existing
  source and test setup with accepted APIs without treating fixture publication as production
  bootstrap or complete service-graph acceptance.
- Compile affected app targets, run representative construction, runtime and window regressions
  with bounded test concurrency, and run normal compilation, formatting and independent review of
  changed composition and failure evidence before acceptance.

App fixture conversion is in place. Normal compilation, all integration-target compilation, 360
library cases and 24 marker-service cases passed. Runtime qualification and its outstanding fixture
corrections are recorded in [app candidate evidence](failures/target-bootstrap-composition.md#app-candidate-qualification).
This phase is not accepted or committed.

The separately accepted chunk-frontier (`28c17be9`) and cooperative-reuse (`1ccd6cb1`) corrections
now allow the original repeated marker-free inputs and the 16-image scale input to complete.
All 19 cases across the GPUI and main-window composer targets passed, and the corrected
backpressure case passed. Compilation and focused independent fixture reviews passed.

The separately accepted [marker-fold correction](failures/syndic-text-insertion-marker-fold.md)
now permits valid marker-bearing text reshaping. The original scale workload completed the
64-image case, then source construction for the first 128-image input reached a safe
[aggregate memtable-capacity refusal](failures/syndic-draft-build-memtable-capacity.md).
Independent review found no supported fixture-only remedy preserving the workload and same-service
evidence. The accepted history-retirement correction now permits the first 128-image input, but
the repeated 128-image input reaches a safe capacity refusal at marker 34. This fixture phase
remains paused at the separate aggregate-pressure design and implementation boundary identified by
the accepted diagnosis above. Preserve hard limits, genuine pins, exact maintenance completion and
pre-journal failure semantics; do not raise limits or reopen between cases. Flush completion alone
does not yet guarantee physical reclamation under the existing sequence fence.

The failure-taxonomy rerun also remains unqualified. After both test barriers were released,
ordinary execution continued polling an open target queue with no ingester or failure coordinator
remaining. The precise failure-capture exit is unresolved; preserve the
[captured wait evidence](failures/target-bootstrap-composition.md#qualification-after-marker-authority-correction)
without inventing a fixture workaround. Candidate recovery and later bootstrap phases remain pending.

# Phase 420: Establish Explicit Candidate Recovery Access (pending)

Implement the distinct candidate recovery access and its typed consumers with ordinary command,
durability, receipt and reconciliation semantics. Preserve exclusive publication and candidate
custody; separate independently verifiable package boundaries when activating this work.

# Phase 421: Prepare CAS Services Before Initial Publication (pending)

Connect accepted candidate recovery access to sequential initial convergence and dormant CAS service
construction, retaining worker creation, startup fencing, cancellation and joined disposal before
publication. Preserve healthy ordinary-service behavior and exact recovery outcomes.

# Phase 422: Establish Explicit Home Marker-Service Ownership (pending)

Construct one marker-seal service from candidate identity and immutable limits, inject shared clones
from the home owner and remove global discovery while preserving flight and retirement custody.

# Phase 423: Publish The Complete Initial App Service Graph (pending)

After every required service factory is independently accepted, compose and publish the complete
private graph with the same home generation, then release ordinary workers. Verify last-constructor
failure, cancellation, startup convergence and publication rejection with full cleanup ownership.
This integration cannot absorb missing service implementations or accept restored GUI visibility.

# Phase 415: Specify Restore-Set Startup Composition (pending)

Resolve exact restoration custody, complete-set first visibility, threadless empty-session startup,
placement and native process-lifetime composition in owning authority, using the accepted session,
window and graceful-shutdown components. Derive bounded implementation phases before wiring the
ordinary executable. The [bootstrap readiness evidence](failures/target-bootstrap-composition.md)
identifies the remaining gaps without authorizing alternate startup behavior.

# Phase 382: Mount Crash Reporting At Process Entry (pending)

After target executable bootstrap exists, connect the accepted reporter and GUI boundaries before
ordinary storage/work startup. Verify reserved reporter mode cannot enter normal bootstrap,
ordinary startup installs fatal handling first, normal exit leaves no reporter, and a real
isolated application panic terminates its process while the report remains usable. No helper-only
or library-only evidence accepts this production mount; the current bootstrap removal gap remains
explicit until its owning rework checkpoint closes.

Blocked on 2026-09-15: `crates/beryl/src/main.rs` remains an intentional compile-error placeholder
for the later target bootstrap checkpoint. There is no ordinary executable composition root at
which to mount the accepted reporter and GUI services. Resume after that bootstrap boundary is
specified and reconstructed; do not substitute helper-only evidence or invent an alternate entry.
