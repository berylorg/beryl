# Process Lifecycle Continuation Projection

## Invalidated Assumption

The process-composition phase cannot infer executable lifecycle continuation from accepted
compaction settlement and independently tested pending execution. Their production composition
rejects the turn kind that successful lifecycle settlement creates.

## Evidence

On 2026-09-13 the managed-runtime test
`managed_lifecycle_compaction_runs_continuation_after_retirement_without_views` submitted through
the real acceptance wake, detached all views, completed provider compaction, and paused after
durable lifecycle settlement. It observed a consumed `LifecycleContinuation` settlement and a
`PendingTurn` gate. After release, the scheduler started a second worker, then became fatal before
any replacement projection request. The master command gate closed while the persistent-failure
coordinator remained Armed with no failed-home generation.

Focused nextest run `b0048d4d-53a3-4eec-b621-10687c08b22b` failed in 12.030 seconds. Its retained
output, summary and samples use prefix
`C:/Users/user/AppData/Local/Temp/beryl-build-memory-20260908/process-compaction-lifetime-projection-20260913`.
The run used one build job and ordinary debug execution without debug symbols or stack overrides.
The supervisor confirmed root reaping and no remaining job PIDs; its temporary directory was
removed. Production code was not changed for this investigation.

Independent source review and root validation establish the first causal mismatch:

- [Lifecycle settlement](../../crates/syndic-storage/src/mutation/compaction/continuation.rs)
  creates `TurnKind::BerylLifecycleContinuation`.
- [Native projection](../../crates/syndic-storage/src/native_projection.rs)
  requires `TurnKind::OrdinaryUser`, returning `CurrentTailNotPendingOrdinaryUser` otherwise.
- [Projection execution](../../crates/beryl-app/src/cas_projection/execute.rs)
  performs that check before issuing projection wire requests.
- [Pending execution](../../crates/beryl-app/src/cas_projection/accepted_input_scheduler/next_turn/worker/execution.rs)
  classifies the error as `ProjectionRefused`; recovered-pending execution converts it to Fatal,
  and scheduler fatal handling closes master command admission.

Two further production exclusions remain visible in
[ordinary preflight](../../crates/beryl-app/src/cas_projection/ordinary/preflight.rs) and
[terminal item convergence](../../crates/beryl-app/src/cas_projection/ordinary/converge/item.rs).
Changing native planning alone cannot complete continuation execution and history capture.

## Fixture Corrections And Evidence Limits

The initial compaction fixture placed `threadId` before `item` in lifecycle envelopes. The
streaming parser requires `item` first; malformed ingress correctly retired the connection before
marker capture. The corrected fixture preserves the required order and proves completed marker
and terminal capture. That earlier rejection is not evidence of a production marker defect.

The initial cleanup assertion also incorrectly required `thread/unsubscribe`. Compaction drops
its terminal projection implicitly, triggering connection retirement; the final fixture accepts
that path and waits directly for continuation dispatch. A 200-millisecond sample of absent
dispatch did not prove a stalled successor. The final failure uses the full bounded successor
wait and preserves scheduler and command-gate diagnostics without unwrapping a closed live command.

Managed terminal-history retention passed at both `BeforeGateRelease` and `AfterGateRelease` in
run `b3a41ca0-48cf-4137-88ba-b7e8cce62bb2` (4.117 seconds), retaining the same session and process
through view detachment and reattachment, then releasing runtime, workers and home ownership.
Its evidence prefix is `process-terminal-history-release-20260913` in the same evidence directory.
That result predates the added compaction fixture and is not acceptance of the complete
composition phase. Continuation dispatch, its final history, and joined cleanup-to-successor
acceptance remain unverified.

## Required Course Correction

Implementation stopped under AGENTS.md after the production prerequisite was established. The
controlling [process and continuation contract](../systems/cas-live-syndic-transcript/design.md)
already requires admitted continuations to execute independently of views. Establish a separate
prerequisite acceptance boundary for legitimate lifecycle-continuation projection, execution and
terminal-history eligibility before resuming [the root plan](../plan.md).

Preserve the distinct continuation kind and exact generation, binding, gate and content authority.
Do not relabel it as an Operator submission or broaden unrelated replacement/edit eligibility.
Retain the managed failing test as the end-to-end acceptance case, add focused tests at the
affected eligibility boundaries, and obtain independent semantic review before accepting the
prerequisite. No production workaround or scheduler-failure downgrade has been applied.

## Accepted Continuation Correction

The Operator authorized the prerequisite on 2026-09-13. Native pending projection, recovery
prefix selection and cursor validation, completed recovery history, ordinary execution preflight,
and terminal item convergence now admit both `OrdinaryUser` and `BerylLifecycleContinuation`.
The continuation retains its distinct kind. All existing generation, binding, pending gate,
content and capture checks remain; provider-operation turns remain excluded. The focused audit
left unrelated user-edit, replacement, admission and title restrictions unchanged.

The strengthened managed case now passes through durable compaction settlement, retirement of
the original connection, replacement runtime preparation and continuation dispatch with no views.
It verifies the exact fixed continuation text, distinct turn identity and kind, complete finalized
history with no open or history-blocking items, idle input gate, healthy scheduler and released
compaction custody, sessions, runtime tokens and workers. Storage tests cover the actual settled
continuation before and after reopen, pending-prefix replay excluding pending content, completed
continuation replay, and unchanged provider-operation rejection.

Acceptance verification used `cargo +stable --config .cargo/local.toml` with one build job,
nonincremental ordinary debug execution, debug symbols disabled and no stack overrides:

- `nextest run -p syndic-storage --features test-faults` for `native_projection`,
  `recovery_projection`, `recovery_projection_faults`, `compaction_storage`,
  `lifecycle_content_publication`, `thread_properties`, `replacement` and `accepted_promotion`,
  with one test thread: 103 passed in 388.681 seconds, run
  `6ba57867-0cc8-4479-8983-be4c56aaa32a`.
- `nextest run -p beryl-app --features test-faults` for `runtime_session_preparation`,
  `normal_terminal`, `context_compaction`, `lifecycle_yield` and `terminal_compaction`, with one
  test thread: 83 passed in 152.476 seconds, run `c94622e7-a7be-4ed4-b6c5-2d2dc1d32f37`.
- `check -p beryl-app --lib` without test features passed in 12.95 seconds. Existing warnings
  remain; this was not a warning-clean acceptance boundary.

Evidence prefixes are `continuation-storage-boundaries-20260913`,
`continuation-app-boundaries-20260913` and `continuation-production-check-20260913` in the
evidence directory above. Supervisors report root reaping and no remaining job PIDs. Independent
semantic review found no blocking issue in the six eligibility guards or the dispatch/capture
boundary. Root review checked the diff and raw verification results. Storage reopen and live
replacement evidence do not establish full application restart scheduling or executable bootstrap.

## Accepted Process Lifetime Composition

With the continuation prerequisite accepted, the same 83-test app run closes the remaining
process-lifetime composition boundary. Managed direct execution and accepted-input successor
dispatch pass alongside terminal-history retention and lifecycle-compaction replacement.
The terminal-history case pauses at both `BeforeGateRelease` and `AfterGateRelease`, after item
and transcript convergence. Each cut retains the same checked-out session and live process after
view detachment, appears in process work inventory, and allows immediate reattachment to the same
readiness. Releasing the barrier completes exact session, process, token, worker and home disposal.

The compaction case retains the original process and custody after consumed durable settlement,
with a pending continuation but no early successor. Release joins implicit connection retirement,
prepares the replacement through the production configured runtime provider, and completes the
distinct continuation and its terminal history without installing a test-side projection.
The existing direct and accepted-input tests prove the complementary execution paths.

Independent composition review and root validation found no blocking issue. This acceptance
composes previously accepted request and approval routing; the change neither introduces a new
routing path nor claims a new joined managed approval test. GUI claims and close workflows,
Running threads mounting, full application restart/bootstrap and coordinated graceful shutdown
remain their separate acceptance boundaries. This outcome closes process execution ownership,
not those presentation or shutdown obligations.
