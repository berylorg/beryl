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

Reconcile the fixture with failed-generation lifecycle and typed disposal evidence. First verify
whether existing failed-service retirement settles the waiting execution while retaining the
required evidence. The captured run does not exercise retirement and does not establish that
production recovery cannot settle the target. Any production completion correction requires its
own demonstrated gap and acceptance boundary; do not infer one solely from this fixture's join.

Independent semantic review confirmed the fixture/authority contradiction and traced existing
failed-service close through terminal connection shutdown. Whether that disposal releases the
last target sender and settles this execution remains a focused qualification requirement.

App qualification remains pending. The post-persist ambiguity case was not reached in this
diagnostic run and must be qualified separately against the same failed-home contracts.
