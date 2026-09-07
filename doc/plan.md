# Scope

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

# Phase 331: Expose Bounded Edit Evidence Before Admission (finished)

Accepted the [text-input package](../../gpui-text-input/doc/design.md) evidence/replay boundary:
exact producer/pass identity, asynchronous page acknowledgement, explicit EOF, bounded replay,
and complete closure before commit. Independent semantic review passed after the late-response
settlement correction; 22 mutation tests, three mounted evidence tests, and the locked local check
passed. Production marker composition remains Phase 329.

# Phase 332: Authenticate Fresh Assets And Mixed Label Assignment (pending)

Extend existing Syndic readiness and Asset witnesses to ordinary admitted fresh AssetIds, keeping
all final label allocation in Syndic and preserving existing labels in mixed edits. Authority is
the [image-asset system](systems/image-assets/design.md), [Syndic system](systems/syndic-conversation-history/design.md),
[Syndic package](../crates/syndic-storage/doc/design.md), and [Asset package](../crates/beryl-state/doc/design.md).

- Add the homogeneous fresh evidence shape and exact 42-byte correlation entries through the
  existing typed source/witness composition; reject missing, malformed, or substituted metadata.
- Derive authenticated preservation, foreign source-thread/label allocation, and fresh AssetId
  allocation groups in Syndic; bind them through canonical storage, byte-exact replay, assignment,
  target consumption, and terminal closure. Reserve only from authenticated allocating counts.
- Verify repeated fresh assets across pages, mixed preserved/fresh/foreign sources, duplicate
  targets, cross-shape rejection, witness substitution, exact vectors/codecs, exhaustion,
  reservation separation, cancellation, indeterminate reconciliation, and generation retirement.
- Run focused readiness/assignment/Asset tests and locked local package checks; obtain independent
  semantic review plus the package-required adversarial pass over authority and custody boundaries.

Readiness: independent architecture review passed, including the separate admitted-target resolver
boundary below. Source/witness, grouping, canonical bytes, reservation, and custody contracts are
ready for implementation after the widget phase is accepted.

# Phase 334: Resolve Admitted Marker Targets For Staging (pending)

Expose the Syndic-owned bounded target resolver under the [draft-storage contract](../crates/syndic-storage/doc/design-draft-storage.md).
Authenticate exact current staging and transferred readiness custody before returning one typed
marker with its assigned label. Verify unchanged counters under repeated reads, rejection of wrong
target/asset/predecessor/generation and post-finish queries, and sole builder consumption. Accept
this package boundary independently before app composition; no app-selected fresh label is needed.

# Phase 329: Connect Production Marker Admission (pending)

Compose accepted widget evidence and Syndic fresh-asset readiness before production storage
MutationBegin. Preserve generic HomeStore proof composition, exact selection and operation,
bounded replay, cancellation and reconciliation. Remove the test-only admission restriction and
verify fresh insertion, replacement, moves, removal, and the three previously failing mounted GUI
cases without weakening their assertions. Broader process-provider work remains deferred.

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
