# Scope

The Operator prioritizes permanent LLVM linking and disabling normal-build debug information across Beryl's
resolved local dependency graph, with opt-in debugging, under the root
[technology decisions](design.md#implementation-technology). Investigate Serena cache controls
and audit the previous memory exhaustion separately. Marker acceptance remains paused during
this tooling work; its successful 145-case diagnostic run is retained evidence.

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

# Phase 346: Assess Semantic Tool Memory Retention (finished)

The [retention investigation](memory/topic/semantic-tool-memory/serena-cache-controls.md) found no
Serena cache-capacity or TTL setting; analyzer LRU capacity already equals 32 and bounds syntax
trees only. Saved one-job and no-dev-debug Cargo environment settings in the parent Serena
project while preserving all eight workspaces and existing navigation controls. Full service
relaunch remains required for adoption; language-server-only restart does not reload project
configuration. This is an accepted configuration investigation, not a measured live memory
reduction. [ENV.md](../ENV.md#windows-build-and-semantic-tool-memory) records activation status;
semantic use of the changed Cargo model remains paused until that full relaunch.

# Phase 347: Record Build Concurrency Failure Evidence (wip)

Reconstruct the OOM invocation and nearby build activity from available evidence. Record proven
concurrency settings, overlap and uncertainty, together with the guarded build/linker comparison.

# Phase 329: Connect Production Marker Admission (pending)

All production marker-admission prerequisites are accepted, including mapped build frontiers,
authenticated terminal cleanup and the bounded settlement-stack correction. The reviewed app
composition and Notifications contribution are implemented; finish their joint acceptance under
the [app composer contract](../crates/beryl-app/doc/design-catalog-and-composer.md#edit-marker-and-candidate-adaptation)
and [feature reconciliation rules](features/composer/design.md#durable-mutation-reconciliation).
The [admission record](failures/composer-marker-admission.md) preserves the earlier findings.

Run the complete focused app set on the accepted storage correction: composer lifecycle, public
marker evidence, mounted composer, slot, notices, pending activation, history, mutations and
publication. Preserve exact marker, candidate/root/history, cancellation, noncommit generation,
typed feedback, late-flight and disposal assertions. Repeat the isolated production app check with
the reviewed 29-file app composition in documented local dependency mode, and obtain final
independent acceptance review before the scoped commit. The detached path is reviewed structurally;
public disposal and mounted lifetime behavior are exercised. No larger test stack substitutes for
the required ordinary-stack witnesses.

Use existing bounded local producers, replayable propagated cut and the admitted-Asset marker entry
point. Mounted clipboard image decoding and large/rich paste remain the separate clipboard/image-assets
checkpoint. Broader process-provider work remains deferred. Canonical dependency publication and
pinning remain Phase 344; the formal old widget pin cannot compile the new evidence/restart APIs.

# Phase 344: Finalize Canonical Marker Dependency Revision (pending)

After local app acceptance and Operator authorization for dependency publication, publish the
accepted widget revision, update Beryl's formal pin and canonical lockfile, and verify locked
canonical metadata plus the focused production app check. Refresh semantic navigation only after
the validated Cargo model passes. This boundary follows the local-development workflow in
[ENV.md](../ENV.md); it does not activate deferred process-provider work.

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
