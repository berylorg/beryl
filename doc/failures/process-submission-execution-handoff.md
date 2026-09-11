# Missing Submission-To-Execution Handoff

## Invalidated Assumption

Process composition assumed that successful ordinary composer submission already notified the
process scheduler. Runtime readiness, session preparation and scheduler components exist, but
durable acceptance alone does not connect them. An unrelated later execution wake can conceal
the missing handoff.

## Decisive Evidence

On 2026-09-10, the managed-runtime composition fixture configured the real process session
provider and runtime preparation, acquired a ready view, and submitted through
`SyndicComposerHost::advance_submission`. Submission returned exact idle acceptance. No explicit
test wake was added. The fixture then waited for the first `turn/start`, before any view detachment.

Focused run `process-submission-wake-proof-20260910` failed after 11.848 seconds. The durable input
gate was `PendingTurn` at revision 2. Session diagnostics reported capacity 4, retained 0 and high
water 0. Scheduler diagnostics reported 38 idle passes, zero started workers, `stopped=false` and
`fatal=false`. Its two pending-turn scans had not consumed a candidate. The broader two-case
`process-execution-composition-20260910` run failed both new checks before their detach, capture or
successor assertions. Those later assertions and the extended server fixture remain unverified.

Logs are retained under `C:/Users/user/AppData/Local/Temp/beryl-build-memory-20260908` with those
prefixes. Both jobs finished; no managed-runtime fixture processes or temporary directories
carrying the new execution fixture modes remained after verification.

## Source Review

`composer_host/submission/acceptance.rs::advance_submission_acceptance` executes or reconciles
first acceptance and returns `ExactSuccess` without an execution notification. The command
construction in `input_admission.rs` adds no notification. The composer slot discards the acceptance
kind and opens the successor editor. The outer conversation-composer service, mounted completion
and unmounted drain likewise handle editor lifecycle rather than execution readiness.

`cas_projection/service/construction.rs::construct` observes home mutations with
`idle_recheck_waker`. That notification deliberately permits idle maintenance only.
`accepted_input_scheduler/signal/wake.rs::restarts_recovered_pending_pass` requires `Recovery` or
`ExecutionReady`. The public execution-readiness notifier has no production submission caller.
Independent bounded review confirmed no established production acceptance flow omitted by the
fixture, and no architectural impossibility in the required correction.

## Correction Boundary

The [CAS-live system](../systems/cas-live-syndic-transcript/design.md) requires durable admission
before scheduling and process-owned execution independent of view selection. Establish a separate
production handoff boundary after exact durable acceptance, independent of successor editor
activation or GUI callbacks. Cover committed, reconciled and idempotent success. Preserve pending
ordinary, active-steering and accepted-next eligibility; an idle-maintenance wake must not become
dispatch or retry authority. A synthetic test wake or reliance on later runtime activity would
hide this missing composition.

Production correction stopped under the Operator's technical-plan-failure rule. The uncommitted
composition tests preserve the reproduction. The preceding regression-reconciliation commit
remains accepted with all 19 baseline and 315 app/control checks passing.

On 2026-09-11 the Operator authorized the separate production correction and continuation.
Implementation and acceptance are tracked in the root plan; that approval does not establish
successful execution of the previously unreachable lifecycle assertions.

After resolving this handoff, composition still needs joined terminal-history, lifecycle-compaction
and compaction-cleanup-to-successor evidence. Existing component tests do not establish those
registered-session handoffs merely by retaining a separate local session.
