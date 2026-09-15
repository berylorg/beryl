# Ordinary Capture Polling After Persistent Home Failure

## Scope

The submitted-input terminal-publication fault in `beryl-app` and its blocked app candidate
qualification. This resolves the unknown coordinator exit in the
[earlier wait snapshot](target-bootstrap-composition.md#qualification-after-marker-authority-correction).

## Invalidated Assumption

The fixture treats a failed-home terminal commit as ordinary source loss: it waits for execution
to return `SourcePublicationFailed`, then expects a newly durable stream-loss terminal before
closing the service. Persistent home failure instead fences durable publication and preserves
the target for failed-generation disposal and recovery.

## Evidence

On 2026-09-15, focused nextest run `01b9448e-a651-4286-8422-02463e569744` reached original
terminal-publication case 35 with its original `BeforeCommit` fault and both fixture barriers
released. A temporary callback sampled the existing public coordinator and router snapshots ten
times, 200 milliseconds apart, then deliberately exited with code 86 after 63.947 seconds. This
was diagnostic evidence, not a passing qualification test.

Every sample showed coordinator `Finished`, service generation 6, failure generation 1, one target,
one proven nondispatch result, zero possible dispatches and zero disposed projections. The router
remained `Active` at revision 10 with one target, zero queued operations and zero outstanding
dynamic tools. The nondispatch count classifies the failure-time interrupt obligation; it does
not prove that the original ordinary turn was never dispatched.

The source explains the retained wait:

- `Ingester::failed_normal_terminal_permit` detects exact persistent failure, drops its permit
  and returns a terminal broker acknowledgement without ordinary `permit.fail()` closure.
- `freeze_persistent_failure_targets` preserves target records and finishes the failure capture;
  its publication-condition notification does not disconnect the target operation queue.
- `TargetRegistration::poll` maps a receive timeout to `Quiet`. Ordinary `run_capture` repeats on
  `Quiet` and has no separate persistent-failure branch.
- `PreparedExecution::execute` joins ordinary execution before returning to the fixture's later
  service close. The observed coordinator exit was successful, not incomplete or stopped.

The probe was removed with no remaining diff in the publication fixture. The restored integration
target compiled with nextest `--no-run` in 2.75 seconds. The diagnostic process exited naturally;
its exact task-owned temporary directory was reclaimed with `cleanup-dir.exe`.

## Correction Boundary

The [app failure-capture contract](../../crates/beryl-app/doc/design-live-projection-and-scheduling.md)
requires preservation of target records and failure evidence. The
[home-storage failure contract](../systems/beryl-home-storage/design.md) prohibits durable Syndic
publication while the failed gate is closed and makes the last committed lifecycle record the
recovery starting point. Thus forcing ordinary source-loss closure and a new durable stream-loss
terminal is not an acceptable way to satisfy this fixture.

The corrected fixture retains only a test home owner while execution runs and consumes the real
service close before joining execution. It asserts exact home/service identity and finished failure
evidence, retired and detached connection state, execution return, and released broker/page custody.
All old home owners leave scope before a fresh candidate registers its domains and publishes the
storage-only fixture. No failed authority is reused and no recovery convergence is implied.

## Disposal Qualification

The original-order disposal probe `b9e989e1-9419-4c62-adaf-c9eeaf00d927` returned successful
persistent-failure close evidence and retired/detached connection state. Ordinary execution then
returned `AfterActivation` with `Coordinator(ProjectionWorkerStopped)`. Terminal shutdown removes
the forwarding attachment; its router and target sender can then drop. The disconnected receiver
enters loss handling, whose first router lookup fails before attempting durable publication.
Thus the earlier polling did not demonstrate a production recovery deadlock.

The accepted full `submitted_input_failures_preserve_taxonomy_and_release` run
`6e737584-da24-4f18-ba46-779a709c7c28` passed in 79.386 seconds, including both terminal fault cases
and the later target-abandonment case, under a 120-second nextest process bound. Fresh reads prove
three source events and an active turn without a terminal after `BeforeCommit`, versus four events
and a source-backed completed terminal after `AfterPersist`. Neither has a fifth event. Both retain
one captured item, zero open items and zero finalized items. The first revised assertion incorrectly
expected finalized count one; source review confirmed that provider completion only closes capture,
while canonical finalization requires a separate later mutation. The final assertion preserves this
distinction rather than claiming a terminal-history fixed point.

Normal app compilation, focused formatting and diff checks passed. Independent semantic review
accepted the test topology, exact failure taxonomy, fresh-handle persistence checks and finalization
distinction. No production runtime behavior changed. Temporary probes and diagnostic resources were
removed. Aggregate app construction acceptance and later candidate recovery remain separate gates.
