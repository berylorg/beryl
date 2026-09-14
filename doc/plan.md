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

# Phase 410: Classify Terminal Compaction As Deferred Work (finished)

Accepted bounded authentication of terminal-but-unsettled compaction as deferred recovery and
compacting-ineligible stop admission. Ordinary classification remains unchanged. Independent
semantic review, normal/test-faults package checks and the broader 166-test storage selection
passed after the separately committed pending-continuation prerequisite. The
[acceptance record](failures/process-shutdown-pending-turn.md#finalizing-compaction-classification)
retains the failing case, outcome/identity/budget coverage and prerequisite distinction.

# Phase 411: Wait For Exact Compaction Gate Release (wip)

Consume the corrected classification under the
[app live-control contract](../crates/beryl-app/doc/design-live-control.md#compaction-and-continuation).
Keep an existing stop barrier waiting while that same compaction remains gate-selected, and
preserve different-target, ended-ownership and safe-reopen failure behavior. Verify the classification
matrix, real terminal-before-settlement waiting and existing stop regressions; complete normal and
test-faults checks and independent review before resuming process-wide coordinator acceptance.

# Phase 326: Coordinate Process-Wide Graceful Shutdown (pending)

Compose the accepted admission fence and exact all-work convergence boundary shared by final-window
close and explicit Exit. Preserve accepted queues and proven-undispatched pending turns, prevent
successor dispatch, retain dispatched or uncertain noninterruptible targets through terminal history
or authority-loss convergence, and return to
coherent windows on failure. Confirmation, final-window designation, durable restore mode and OS
close integration remain their subsequent rework acceptance boundary.

The [pending-turn correction](failures/process-shutdown-pending-turn.md) records the authorized
completion distinction. Verify durable pending preservation separately from uncertain dispatch,
including preparation cleanup, reconciliation failure and later recovery; no coarse work snapshot
or absent provider identity proves either completion outcome.

The acquisition fence and exact terminal handoff prerequisites are accepted. Resume composition
using both the original captured execution obligation and any winning successor's separate pending
obligation. Do not replace exact completion with current Idle state, missing live metadata, pending
provenance for another turn, or a stop response. The initial unaccepted coordinator was removed;
its [review record](failures/process-shutdown-pending-turn.md#terminal-predecessor-completion-handoff)
remains evidence for the required proof and cleanup distinction.

The authorized [bounded obligation prerequisites](failures/process-shutdown-pending-turn.md#bounded-shutdown-obligation-ownership)
are accepted. Readiness review confirms the execution capture, exact stop, thread settlement and
owner-driven cleanup primitives can be composed. Preserve exact admission validation and generic
cleanup joins while allowing progress over later work.

- Retain one service-bound shutdown attempt with its admitted fence and bounded execution capture;
  concurrent requests join that attempt and stale attempt/service inputs cannot affect another.
- Refresh and progress every captured ordinary and compaction obligation before waiting for global
  admission closure. Preserve earlier accepted identities across drift and capture late winners
  before the final convergence check. Use the existing exact stop and continuation-cancellation
  paths without treating their responses as completion.
- Visit generic custody with bounded pages. Await preparation and direct loaded-projection owner
  release, retire eligible scheduled sessions through their existing exact owner, and explicitly
  poll every retired connection's join, including threadless failures. Preserve reconciliation
  custody and do not discard an execution obligation when cleanup disappears.
- After admission closure, validate a complete current generic inventory and page durable non-idle
  gates. Authenticate each current pending/terminal turn through the accepted settlement guard;
  keep guard retention bounded and account only for the coordinator's own flight-revision changes.
  Any external revision change invalidates the sweep. Accepted queues remain unchanged.
- Retain failed attempts until coherent failure permits reopening. Do not repeat uncertain dispatch,
  restore cancelled continuation intents, or release windows/claims through this component.

Verify real active soft-stop convergence, noninterruptible and uncertain work, predecessor/successor
proofs, late admission behind a progress cursor, multiple live and durable pages, exact guard
revision changes versus external mutation, cleanup/join failures, reconciliation and later recovery,
duplicate requests, stale generations and coherent reopening. Run focused normal-library and
test-faults checks and the affected shutdown, execution, stop and compaction regressions. Independent
semantic review must accept the exact completion and cleanup composition before this phase finishes.

Resumable milestone: the partial coordinator is uncommitted and unaccepted. Focused tests cover
271 durable pending rows, 270 generic records with late slot capture, exact guard revision accounting,
real managed soft stop, failed detached joins and reconciliation.

The connection traversal prerequisite is accepted as `4b3c463e`, following bounded persistent-failure
capture in `19c9338e`. Readiness covers the complete cleanup path and bounded exact execution
capture. The
[failure record](failures/process-shutdown-pending-turn.md#bounded-shutdown-obligation-ownership)
preserves the invalidated capacity assumption; never infer a registry bound from worker capacity.

The [coherence correction](failures/process-shutdown-pending-turn.md#failure-reopening-coherence)
is accepted through home election `6b148050` and admission composition `eb3ee8c1`. The coordinator now
delegates failure reopening to that exact service boundary. Resume complete verification and
independent review, adding integrated predecessor/successor completion and noninterruptible work
evidence. Do not substitute another snapshot or count stop acknowledgement as completion.

Final-readiness review also required home election before the final captured-revision validation.
The implementation now rejects returned custody before installation and retries commits on either
side of election; focused regressions cover both cases. Integrated runtime tests cover predecessor
completion across both successor-handoff barriers. The Operator authorized resolving the reported
compaction blocker; exact-stage inspection and independent review corrected that diagnosis.
The [compaction progress record](failures/process-shutdown-pending-turn.md#noninterruptible-compaction-progress)
records the fixture's missing interrupt response and the subsequently exposed rejection of provider
events while stopping. The app publication correction is accepted in `dbab7017`. Resume with fixture evidence
for initial waiting, later exact stop, acknowledgement without completion, terminal settlement and
cancelled continuation. The separate concurrent stop-read retry remains applicable.

Blocked on 2026-09-15 after normal and test-faults checks passed and 17 of 18 focused tests passed.
The runtime compaction test now passes initial waiting, eligible stop and acknowledgement, but
final convergence exposes a separate [classification gap](failures/process-shutdown-pending-turn.md#finalizing-compaction-classification).
A terminal provider turn remains selected by its compacting gate until final settlement; the
ordinary blocking-turn classifier rejects this valid state. Independent review confirms that a
canonical bounded compaction classification correction is needed, followed by same-target waiting
in the legacy stop barrier. Removing barrier polling or retrying invariant failures would hide
other required failure outcomes. Implementation is paused under the Operator's invalid-plan rule;
derive the owning storage and app prerequisite boundaries before resuming this phase. The phase
remains unaccepted, with full affected regression execution pending.

# Phase 400: Diagnose Repeated Draft Content Materialization (pending)

Determine the bounded correction and acceptance evidence for the
[repeated-content failure](failures/syndic-draft-materializer-content-identity.md#repeated-content-collision)
observed while constructing shutdown backlog. Compare immutable content identity, exact-root mapping,
existing sealed content and concurrent build ownership against the
[draft storage authority](../crates/syndic-storage/doc/design-draft-storage.md#materialization-restoration-and-text-reads).
This phase accepts diagnosis and readiness evidence only; resolve any missing architectural choice
in owning authority before planning an implementation correction.

# Phase 382: Mount Crash Reporting At Process Entry (pending)

After target executable bootstrap exists, connect the accepted reporter and GUI boundaries before
ordinary storage/work startup. Verify reserved reporter mode cannot enter normal bootstrap,
ordinary startup installs fatal handling first, normal exit leaves no reporter, and a real
isolated application panic terminates its process while the report remains usable. No helper-only
or library-only evidence accepts this production mount; the current bootstrap removal gap remains
explicit until its owning rework checkpoint closes.
