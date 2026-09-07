# Recovery Prompt View Lifetime

The recovery coordinator already owns the exact request and parked execution independently of its
view. The GUI nevertheless blocked activation while a recovery prompt was present and cancelled
the process route when disposing or dropping that view. Existing tests cancelled the route before
switching, so they did not prove the required view-independent behavior.

The accepted correction uses the existing thread lookup and composer activation boundary. A
suspended host flushes before publication using its saved exact widget-release proof. Failure
retains the prompt, draft and route; successful publication releases only prior GUI state. New
recovery presentation waits while target activation is pending. Switching back binds the current
host to the retained route. No execution registry or coordinator API was added.

Autosave stays suspended while the prompt has no editor contribution, including after failed dirty
flush. Actual editor restoration resumes autosave without requiring another edit. Treating the
prompt flag alone as suspension was insufficient because restoration initializes the editor before
clearing that flag.

## Verification

- Ten recovery cases in `main_window_composer_mount` passed, including both new `recovery_switch`
  cases, with `--features test-faults --locked`, one test thread and process-local
  `RUST_MIN_STACK=16777216` (nextest run `4b75589e-e0d8-4bfe-b302-84cdd772f911`).
- Evidence covers dirty-root nonpublication on forced noncommit, retained prompt past the autosave
  deadline, successful retry, stale receipt rejection, late route-update isolation, same-key return,
  autosave after restoration, and route preservation through actual GUI disposal and Drop.
- Default and test-faults app checks, scoped formatting, whitespace validation and independent
  semantic review passed. The review traced successful detached recovery through current route
  lookup; the explicit detached round trip tests late failure, while existing cases cover success.
- Seven ordinary GUI failures reproduced identically with only the five changed GUI production
  files replaced by HEAD. Exact working bytes were restored and all five hashes matched. This
  comparison retained the deferred provider changes in both runs; it was not a pristine checkout
  test. Four were stale capture drivers, corrected without relaxing assertions or adding waits.
  The final three-target run passed 32/35; the remaining three expose the separately recorded
  [marker-admission gap](composer-marker-admission.md). They are not claimed as passing.

Acceptance is limited to the tested composer mount boundary. Complete shell claim, session and
transcript switching is not yet composed above it and has no end-to-end verification here.

## Retained Test Resources

Automatic approval review rejected cleanup with `blocked by policy`, including separately resolved
absolute targets. Six stack-aborted fixtures remain under the ignored
`localtest/background-execution-work` directory, totaling 458,490,566 bytes:

- `pending-predispatch-WuGHWs`
- `pending-promote-ZgcGgU`
- `pending-terminal-r4bJ3E`
- `predecessor-index-release-Qn1pgj`
- `predecessor-index-release-TSJAPE`
- `single-pending-authority-ogkKyS`

No test process remains from these runs. Do not repeat the rejected cleanup without resolving the
policy block; these fixtures are not required source or evidence for the correction.
