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

# Phase 417: Qualify Beryl State Initial Candidates (finished)

Accepted explicit complete-state fixture registration and publication, exact custom-domain
declarations and preserved schema, failure, recovery and theme behavior. All 152 state cases,
normal and test compilation, formatting and independent review passed; [acceptance evidence](failures/target-bootstrap-composition.md#beryl-state-candidate-qualification)
retains the scope. Production state behavior was unchanged.

# Phase 418: Qualify Syndic Storage Initial Candidates (wip)

Apply the [Syndic public boundary](../crates/syndic-storage/doc/design.md#public-boundary) and its
inherited persistent-state-integrity contract to Syndic-owned fixtures.

- Convert shared and direct fixture opening to `HomeOpenCandidate`, exact package-owned Syndic
  registration and `required_domains` before explicit publication. Fixed typed fixture composers
  may own the complete sequence; generic openers must retain the unpublished candidate.
- Merge exact state, Syndic and test-domain declarations where a fixture composes them. Rebuild
  live registrations on every fresh physical open, preserving recovered cleanup attachment custody
  and distinguishing ordinary initial opening from same-home recovery candidates.
- Preserve routine versus exhaustive validation, compact recovery sources, V7 record behavior,
  typed failures and rejected candidate cleanup. Adapt pre-existing library-test setup where needed;
  remove obsolete source examples of the replaced opening API.
- Compile affected test targets and run the Syndic suite with required fault features, keeping
  concurrent test resource use bounded. Run normal compilation and focused independent review of
  changed composition and failure assertions. This does not accept candidate recovery access or
  application service publication.

# Phase 419: Restore App Construction Evidence With Initial Candidates (pending)

Convert app fixtures to declared complete candidate registration and explicit publication, preserving
their exact services and initial conditions. Compile affected app targets and run representative
construction, runtime and window regressions; do not claim production bootstrap from fixtures.

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
