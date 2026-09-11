# Process Session Retirement Completion

## Invalidated Assumption

Waking on retirement request and worker capacity release does not establish notification after
ordinary connection disposal. The runtime owner can later join the driver and ingester and detach
the connection without another scheduler wake. A retired scheduled-session slot then blocks
accepted successor acquisition until an unrelated observer reaps it.

## Evidence And Correction

The managed accepted-successor reproduction completes its first turn and leaves one accepted-next
input durably queued. Checkout encounters a retiring session. The fixture now permits either reuse
or a fresh process after idle retirement; requiring process reuse was not part of the contract.
Run `successor-retirement-natural-20260911` still times out without successor dispatch. The bounded
control `successor-retirement-reap-control-20260911` adds one diagnostic reap after one second and
passes. That diagnostic call was removed and is not acceptance evidence for autonomous progress.

`ScheduledExecutionSessions::diagnostics` calls `reap`, explaining why its failure snapshot reports
zero retained slots: observing the failure changes the state. `ProjectionRuntimeRetirement` already
polls completed ordinary retirements and consumes them, but `execute_ordinary_shutdown` publishes
detachment without a scheduler notification. Reaping a session slot already emits execution readiness.

Notify the originating scheduler's idle-maintenance lane after clean joined connection disposal.
Use existing runtime maintenance and session reclamation; do not add polling, preserve an idle
session merely for queued input, or treat an idle wake as direct dispatch or retry authority.
Verify the original natural successor path, generation isolation, resource release and relevant
idle/session regressions before acceptance. The Operator authorizes this correction in phase 386.

## Accepted Evidence

The correction adds only the post-disposal `IdleRecheck` to the originating attachment's signal.
It follows both joins and observable detachment; the exact session reaper's detached fallback
remains valid while the shutdown-settlement lock is held. Failed disposal suppresses the wake,
persistent-failure disposal is unchanged, and no new notification opens execution or retry directly.

Both natural managed execution cases pass in `de33d932-bd54-4ead-9038-f9da800b8849` after removing
the diagnostic reap and temporary source/server tracing. The 35-case scheduler/session/preparation
run `2db603f1-6849-40ca-b887-802239fb7c65` and 13-case managed-interest/connection-disposal run
`83da4d8f-ff2f-4aa9-8567-4f912b790803` pass. They include quiet retirement, final-view release,
late admitted work, contended settlement, poisoned cleanup and foreign runtime/process exclusion.
The locked production app check and independent semantic review pass. The new composition fixture
is retained with the separately pending submission-handoff acceptance.
