# Scope

Production marker admission has passed local acceptance with the accepted LLVM, one-job,
no-normal-debug and nonincremental settings under the root
[technology decisions](design.md#implementation-technology). The next boundary is canonical widget
publication and pinning, which requires explicit Operator authorization below. Preserve required
checks and independent review while using conditional delegation and bounded evidence.

Keep GUI thread switching clean without overengineering. The Operator now authorizes the two
reported non-GUI corrections: marker admission and healthy scheduler-conflict handling. Keep each
in its own acceptance boundary and reuse existing mechanisms. Broader process-provider composition
and other deferred work remain outside this authorization.
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

# Phase 329: Connect Production Marker Admission (finished)

Accepted the production evidence/admission composition, exact retained mutation custody and
Notifications feedback, including unavailable build and committed-presentation failures. All 156
distinct focused cases passed on ordinary stacks, and the isolated 30-file production app check
passed with the accepted storage correction and local dependencies. Independent review found no
remaining blocking issue. The [admission record](failures/composer-marker-admission.md#evidence-and-status)
preserves evidence and structural-coverage limits; canonical publication remains below.

# Phase 344: Finalize Canonical Marker Dependency Revision (pending)

After local app acceptance and Operator authorization for dependency publication, publish the
accepted widget revision, update Beryl's formal pin and canonical lockfile, and verify locked
canonical metadata plus the focused production app check. Refresh semantic navigation only after
the validated Cargo model passes. This boundary follows the local-development workflow in
[ENV.md](../ENV.md); it does not activate deferred process-provider work.

Blocked on explicit publication authorization: accepted widget commit
`5f2f272b71666b56f990a1d2ee15e83219a05c9e` is ready for `gpui-text-input` origin/main. Read-only
remote verification still reports `fc17c5738c35350e58e32437cdae74e28ebc31af`, matching Beryl's
formal pin. No dependency push or canonical pin change has been performed.

# Phase 324: Own Scheduled Execution Sessions In The Process (pending)

Deferred independent scheduler composition work. The uncommitted provider has no production GUI
switching caller and is not a prerequisite for Phase 327. Preserve its unaccepted source separately;
do not include it in the GUI phase's commit or infer it is required merely to detach a view.
Its concurrency test exposed [existing healthy-conflict fatalization](failures/cas-phase13-global-revision-publication.md).
The narrow scheduler correction is authorized separately in Phase 330; that authorization does not
activate this provider work. Re-establish readiness before resuming this phase.

# Phase 325: Own Running Work Independently Of Views (pending)

Compose process-owned execution interest across direct submission, scheduled input, compaction,
continuation and terminal-history work. Keep exact request routing and background attention under
their owning policies. Establish the bounded revision-bound work inventory for shutdown and
Running threads without mounting a GUI per thread. Verify view-interest release and immediate
reattachment preserve the same live execution and capture, and all terminal/replacement paths
release required resources. Split any independently missing composition prerequisite before
activation; production runtime/provider composition remains explicit rather than inferred.

# Phase 326: Coordinate Process-Wide Graceful Shutdown (pending)

Implement one admission fence and exact all-work convergence boundary shared by final-window close
and explicit Exit. Preserve accepted queues, prevent successor dispatch, retain exact pending and
noninterruptible targets through terminal history or authority-loss convergence, and return to
coherent windows on failure. Confirmation, final-window designation, durable restore mode and OS
close integration remain their subsequent rework acceptance boundary.
