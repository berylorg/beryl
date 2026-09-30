# Recovery Mount Fixture GUI Lifetime

The selected recovery fixture holds candidate-backed editor resources without publishing its
replacement service graph. Its successful local release and terminal widget disposal must occur
within one outer GUI update. Returning to GPUI with that resident live allows additional editor
work before the fixture's disposal assertion.

On 2026-09-30, phase 835's new settlement-refusal test passed its refusal, retry and ticket-consumption
assertions but failed the later `input.dispose(...).is_empty()` assertion in `recovery_shell.rs`.
Run `2ca84b04-b5a9-4f33-8f91-0a371b1e5a57` passed the other nine shell cases. The test had introduced
an outer `cx.update` boundary between local release and cleanup, unlike the existing fixture path.
The correction keeps both in one outer update; production recovery still requires its complete
published graph and final synchronous process/shell/request transition. This fixture failure does
not establish a production admission or ticket-retention defect.

The failed fixture did not print its random temporary-home path. Any residue has ambiguous ownership
and must not be swept from the shared temporary directory. The test process exited normally through
nextest's failure handling; no task-owned test process remains from that run.

Corrected run `9ec99559-ff13-4bef-9067-845b1f883c5e` passed all ten shell tests, including the
selected refusal and successful retry case. Independent review accepted the correction; production
compilation and formatting also passed. The earlier twenty-five publication tests remained valid
because only this shell fixture changed after their successful run.

## Published Owner Completion Cleanup

Phase 836's full threadless completion test passed its recovery assertions, then reused disposal
that assumed the original shutdown gate remained installed. Run
`c47c3e5f-e4e5-4e0c-8b49-2a8be9aeac2b` aborted at `begin_shutdown_draft` with
`shutdown draft requires the running shutdown interaction gate`. Successful completion releases
that gate; fixture disposal must explicitly reinstall it before its terminal retirement pass.
The printed task-owned home was `C:\Users\user\AppData\Local\Temp\.tmpMHx81p`.
The exact printed directory was removed with `cleanup-dir.exe` after the test process exited and
verified absent. Corrected focused run `791310ad-eb92-4a31-9f5c-cc3c8b50e8c0` passed; final regression
run `39d969a6-7964-4ca5-84fe-2d8163473ab1` passed all 38 publication, shell-recovery and admission
cases. Production compilation, formatting and independent review also passed. No task-owned test
or build process remains.
