# Non-GUI Discussion Handoff Qualification

## Status And Boundary

Accepted on 2026-10-20 after independent aggregate source and runtime review against implementation
through `4397eb5f` plus the three-path test-only qualification. The coordinator, typed participants,
scoped tool routing and graph seams are qualified. This does not accept complete child conversation
execution or discussion status/Retry presentation.

Controlling authority is the [handoff system](../systems/branch-discussion-handoff/design.md),
[branch behavior](../features/branch-discussions/design.md) and
[App participants](../../crates/beryl-app/doc/design-feature-adapters.md#discussion-resolution-admission).

## Accepted Components And Composition

- Coordinator commit `26d9d9b9859539f5b7f75dff2bd85111d7dfab46` records eighteen focused passing
  cases and independent review. Ordinary scanning retains bounded pages and exact ready identities,
  shares publication fencing, coalesces relevant wakes and joins disposal. Waiting or retryable jobs
  do not create an automatic retry loop.
- Scoped resolution routing commit `f72191dd26ef81123fc2eaa663bf8eb7cde69a7b` installs the actual
  generation-qualified handler. Correlation grants no mutation right: exact admission preserves
  historical idempotency, queued-input deferral, the original single reply owner and unresolved
  command custody. Its sixteen focused admission/tool cases and App test-target check passed with
  independent review. No replacement handler or CAS sender is needed.
- Accepted typed State/Syndic participants compose admitted intent and the child gate, generated
  parent input and starting-parent transition, exact parent execution and atomic terminal
  failure/archive. Explicit retry preserves the same job, payload and any admitted parent input.
  Component tests qualify Unicode/record bounds, ordering, revision conflicts, uncertainty,
  cancellation, unknown-dispatch refusal and retained nondispatch custody.
- [Complete initial graph acceptance](../failures/target-bootstrap-composition.md#complete-initial-graph-publication-and-retirement)
  passed sixty owning graph/component cases. Initial preparation settles retained nondispatch proof
  custody before CAS preparation, performs candidate convergence before ordinary handoff work, and
  retains the coordinator in the complete published graph behind its shared publication fence.
- Recovery graph preparation/publication retains the same ordering with fresh candidate authority.
  Existing publication tests verify zero completed coordinator passes before release and progress
  after release. Retirement joins that coordinator. Successful final cleanup requires both Activity
  enrollment and nondispatch proof counts to be zero; pending proof refusal preserves ownership.

The earlier graph custody fixture specifically exercises Activity enrollment with nondispatch count
zero. Empty-job publication progress is evidence for release ordering, not a positive branch-job
outcome. Neither is relabelled as graph-level retained nondispatch qualification.

## Focused Aggregate Evidence

Independent review identified the interface that still needs direct proof: a real retained
generated-parent nondispatch event crossing the complete process graph's shutdown and failed-home
replacement boundaries.

Qualification must establish exact shutdown refusal with enrollment count zero and nondispatch
count one, without dropping the original graph or proof. Fresh recovery must refuse CAS preparation
before the retained prefix settles, preserve original proof through cancellation, settle the same
parent/input/job into retryable failure, then publish/release a fresh graph without automatic retry
or duplicate parent input. Existing component fault, unknown-dispatch and capacity matrices remain
separate reusable evidence; this gate adds no production capability.

The final two cases pass 2/2 in 3.408 seconds, run
`e20a2cb8-7325-4cec-b51d-765e0144ccc4`. The shutdown case carries genuine preactivation
preparation-failure proof through exact `PendingCustody { enrollments: 0, nondispatch: 1 }`
without losing its graph, job, input or pending dispatch. The activated-parent case establishes
pre-prefix refusal, cancellation-preserved proof, `Prior` to `Exact` activation cancellation,
the same retryable parent/input/job, fresh publication and a completed real scanner pass without
automatic retry or duplicate input. Both finish real retirement, Home closure and reopen checks.

The minimal fixture constructs only its completed parent source and child through normal commands,
validates its actual context provenance and proves the unrelated open thread is absent. Earlier
failed fixtures are diagnostic, not passing evidence;
[the source/audit lessons](../failures/discussion-outcome-custody.md#graph-level-retained-proof-qualification)
preserve the invalidated assumptions.

Fixtures seed closed nondispatch evidence through the existing test-only `for_test` constructors
after real paired State/Syndic admission and generated input. This qualifies graph custody and
ordering; it is not a new live CAS rejection-capture or complete child/tool workflow receipt.
Existing component tests retain those capture/classification guarantees. The fresh replacement
coordinator actually runs. All three test paths match the canonical checkout by SHA-256. Tests use
locked dependencies, one build job, no debug/incremental compilation, optimization zero, the stable
Windows LLD linker, a process-local 32 MiB Rust thread stack and serial nextest with bounded
slow/termination handling. No production source or dependency changed, so the current normal
App/executable production check and unaffected owning matrices remain applicable. Separate owning
case totals are not combined into a new unique global count.

Independent final review accepts the aggregate non-GUI boundary and confirms reuse of the owning
coordinator/routing/graph and current broader qualification within their stated scopes. Both final
fixtures explicitly close and reopen their Home before closing the temporary directory. No owned
build/test process remains. After the last review consumer, the exact handoff task directory and
its ephemeral logs/configuration were removed and absence verified; ambiguous/shared resources
were left untouched.

## Remaining Product Boundaries

Captured branch creation, immutable provenance, ordinary child activation, readonly context,
first-submission storage ancestry and inherited-label prerequisites already have accepted owning
evidence. They do not establish a runnable discussion conversation or the full resolution product.

Ordinary execution preflight still rejects a pending input with a context owner. Complete child
execution needs exact-once immutable assistant-context delivery through its ordinary lineage;
removing the guard alone would not satisfy that contract. The chosen injection primitive is viable
in the supported provider; [pinned source research](../memory/github.com/berylorg/codex-fork/commit/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/assistant-context-injection.md)
does not substitute for the finite context-proof, versioned frame and bounded replay contract that
must precede implementation. Discussion status and explicit same-job
`Retry handoff` are also unmounted; the settlement service's retry preparation has no production
caller. Existing creation-activation Retry retries selection of the existing child and grants no
handoff retry authority. These are subsequent bounded product outcomes, not missing coordinator
implementation or justification to rebuild accepted creation.
