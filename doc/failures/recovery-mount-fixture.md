# Recovery Mount Fixture GUI Lifetime

## Resume Uncertainty Reconciliation

The initial selected resume-uncertainty fixture assumed `AfterCommitBeforePersist` would leave
the resume unresolved. Run `06233699-f56f-4040-84c1-413eead1c805` passed nine cases, but that
case prepared successfully: ordinary durability uncertainty leaves the candidate usable, and
the production convergence driver immediately reconciles the resume. To qualify unresolved
uncertainty, also arm the existing `BeforeReconciliationSnapshot` fault after retaining the
original committed Exit; assert the resume's distinct indeterminate outcome and retained failed
reconciliation. Do not weaken healthy reconciliation or expect uncertainty alone to fail recovery.
The failed test process exited; its exact printed directory `.tmp0U6zCW` was removed.

Failed snapshot reconciliation retains its registry scope. Independent review identified that
ordinary home close must refuse it; run `3160803d-489c-47b2-b176-a3266dbdaa7a` confirmed the
retained-outcome assertions passed before close refused one pending reconciliation scope. After
all unavailable-state assertions, teardown retries that exact resume handle, requires `ExactNew`,
then aborts the candidate and closes the home. This cleanup does not execute another resume or
qualify production later-attempt recovery. The exact failed directory `.tmpyaSNDy` was removed.
Corrected run `1a9917f9-13d8-4485-b161-d2d1a683c558` passed all ten selected native cases,
including exact-handle teardown and home reopen; independent lifecycle review passed.

## Resume Home-Command Fault Routing

The selected two-window resume-noncommit fixture initially armed a typed scoped `BeforeCommit`
fault for `ResumeSessionAfterExit`. Native run `a3155b2a-accc-4274-81a5-a4fdf8fd9114` passed the
seven existing cases, but the new case reached successful preparation instead of the expected
error. Scoped writer fault routing applies to `CurrentDomainCommand`; resume executes a
`HomeCommand`, whose writer fault context is unscoped. Use the unscoped fault after the original
Exit outcome is retained, and assert the distinct retained resume `NotCommitted` outcome to
prove the intended command failed. This was a fixture error, not evidence of recovery failure.
The exact printed failed fixture directory `.tmpoKeCbe` was removed after the test process exited.
Corrected run `8d7a596b-1f04-41cd-a9e9-5edb42208e79` passed all eight selected native cases;
independent review confirmed unscoped writer routing and the resume's position before service convergence.

## Selected Original Exit Outcomes

Phase 854 extends the real selected path beyond proven noncommit. The retained session envelope
must evolve from a committed original Exit into a separate Running-resume outcome; comparing the
entire envelope's Debug text rejects valid recovery. Compare immutable publication evidence,
preserve the original execution kind and receipt, and assert the distinct committed resume.
For pending original execution, assert no candidate resolution before recovery and exact-new
candidate reconciliation afterward. Run `2632036f-0e0f-45e0-8b72-3791b4e23dc9` exposed the obsolete
envelope-equality assertion after successful committed candidate settlement.

An indeterminate command alone deliberately does not fail the whole home. Run
`0b1962b9-2b6c-4735-9f75-6a14d324e180` passed committed recovery, then correctly refused healthy-home
graph retirement in the pending-outcome case. To qualify pending custody across home replacement,
inject a subsequent actual home read failure on a background worker while leaving that original
command unresolved. Do not weaken retirement admission or pre-reconcile the original through the
healthy graph. Both failed processes exited; their exact printed homes `.tmpwKrmT8` and `.tmpFrzGwg`
were checked, removed with `cleanup-dir.exe`, and confirmed absent.

Corrected focused run `22778192-304e-465e-8420-ec036fd6cca1` passed all four selected cases.
All 43 Exit-publication regressions passed in `0be9434e-7c64-4e2c-9909-1c26d0f83430`; scoped
formatting and independent lifecycle/persistence review passed. No production source changed.
This qualifies one selected window with committed-original and candidate-reconciled exact-new
outcomes, not every reconciliation result or the automatic recovery supervisor.

## Resident Disposal Timing

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

## Threadless Attachment Appearance Retirement

Phase 841's composed attachment fixture skipped the previous appearance retirement that its
former component helper performed. Run `6f6dc6d6-01f2-4f93-bc3e-e596a7988f45` correctly refused
binding with `window adapter rejected current appearance`, then aborted on the fixture unwrap.
Retire the previous appearance target before entering the composed attachment operation; retiring
the shell draft alone does not retire that registration. Production authority already requires
old appearance retirement. No production contract change is needed. The failed run did not print
its temporary-home path; possible residue has ambiguous ownership and must not be swept.

Corrected focused run `70429ed4-d6e8-4e9b-b1d1-81e9695c13c0` passed. Remaining lifecycle regression
run `8dca4692-5b74-433a-9fa5-d98553a26f5b` passed all 43 cases. Production compilation, formatting
and independent review passed. No task-owned build or test process remains.

## Selected Shell Publication Readiness

Phase 845 initially promoted the resident fixture to a published shell as soon as its composer
entity existed. Run `1640095b-a83b-4529-ac2a-e7dbe18050cc` aborted all six native driver cases
with `hidden main-window composer is not first-presentable`. Entity creation precedes completed
presentation preparation; the fixture must await the existing `selected_first_presentable`
check before shell publication. This corrects test setup and does not change production readiness.
The failed fixtures did not print their temporary-home paths before aborting; preserve ambiguous
temporary residue rather than sweeping the shared directory.

The replacement graph also reloads its persisted theme. Run
`65463441-e3fe-4d11-b880-51bc1797c482` passed publication but aborted in rendering with
`Inter font not found`. An in-memory system-font appearance on the predecessor is insufficient.
Reuse the existing native-test theme installer before failing the fixture home, so candidate
theme reads select the same installed system-font theme. The exact printed failed home
`.tmp1gnA5A` was already absent on cleanup inspection.

The composed driver fixture must also use the shell's safe focus target while the editor is
disabled. Run `9bc4474c-a12d-4dd0-9f91-e9db8eb8cba2` preserved focus after refused binding but
changed it when fresh binding replaced the old notices. `replace_recovered_notices` removes
the old record; the [notice interaction contract](../gui/widgets/main-window-notice/spec.md#interaction)
explicitly returns focus to the owner-supplied safe target on removal. Forcing focus into the
disabled editor and expecting it across notice replacement was an invalid fixture expectation.
Removing an attempted gate-release probe and retiring predecessor appearance earlier did not
change that outcome. Keep the exact original-focus assertion after refusal, then assert the
documented safe target after the separate binding-only continuation. Other driver scenarios keep
their original focus assertions. The differing same-pass behavior is not fully localized; no
deferred adoption refocus was established. This changes no production notice behavior. Printed failed homes
`.tmpMAVYqG`, `.tmpXtjgIC`, `.tmptKmy2V` and `.tmpjW5gnC` were already absent on exact cleanup checks.
The attempted safe-target initialization also failed the same-pass case; its printed `.tmpAPwLzZ`
was absent on inspection. Corrected focused run `a1e7cdec-8d6e-4bc1-9e07-2d5e81944210` passed all
six driver cases. Independent review accepted the authority-based assertion and ownership boundary.

Final lifecycle run `ad55850b-0a04-4d2f-9a85-09f5fd45f449` passed all 50 cases. Production compilation
and formatting passed. This accepts the selected attachment/binding boundary, not full same-home
selected-process publication. No task-owned build or test process remained at phase completion.

## Pending Completion Contention Fixture

Phase 859 initially held the composer service slot mutex around polling the asynchronous
completion driver. Run `2537bd7b-d01b-4412-b1dc-42aa46e5ecc9` passed the four single-window
cases, then stalled in the first two-window case. Instrumented run
`a2ea1fcd-c2c1-4c4a-95a3-4ff5d15ec3ac` confirmed that polling did not return while the slot was
held. This fixture spans `AsyncApp::update` and its deferred GUI effects, beyond the intended
nonblocking release check; GUI service readers can acquire the same slot. Do not hold a service
mutex across a whole native GUI update to force readiness pending. The exact blocking reader
was not localized. Both test processes and their runners were explicitly stopped.

An attempted autosave-readiness deferral also targeted the wrong stage: graph publication has
already released recovered service-close custody, so completion bypasses autosave preparation.
Run `e44bb2a1-2c91-4b5e-8636-6237617430fe` failed the pending assertion; diagnostic run
`4a4af447-9ec9-4a7d-a94e-b78a3d4a4cca` confirmed immediate successful completion. Their printed
homes `.tmpdMzROx` and `.tmpDEvzny` were removed and verified absent.

Use a per-mount, one-shot unit-test deferral at recovered mount readiness instead. Assert that
the deferral was consumed before testing cancellation or future drop, so the test proves entry
into the incomplete release pass. Normal production readiness is unchanged. The diagnostic
run's printed home `.tmpIq77Yn` was removed and verified absent; the first run did not print its
home, so ambiguously owned temporary directories were preserved.

Run `2a524531-b8a8-4e9c-b594-71d954c5ebb6` reached the pending mount boundary but invalidated
the fixture's assumption that every input remains disabled when the last mount is deferred.
Earlier mounts have already prepared enabled, read-only residents. Defer the first mount when
qualifying only wait cancellation with the stricter all-disabled assertion. This avoids conflating
prepared resident presentation with final interaction release. Its printed home `.tmplUcCq7`
was removed and verified absent.

Corrected run `4bb2f73a-82f3-458e-9809-f60202a80d8e` passed all seven native cases with the
first-mount deferral. Scoped formatting and independent lifecycle review passed. This proves
pending completion cancellation/drop, not real service-lock contention.

## Partially Prepared Completion Mutation Probe

Phase 860 run `d3597641-9fde-4001-abab-94533f7d0316` passed four single-window cases,
then invalidated a test probe requiring platform text replacement to return `ReadOnly` for
the prepared first resident. That API checks interactive surface readiness before the read-only
flag, so its refusal cannot reliably isolate that flag at this intermediate mount boundary.
Use inline-object insertion instead: it checks enabled/read-only state before surface readiness
or mutation work. With enabled state separately asserted, exact `ReadOnly` proves the prepared
editor remains fenced. This corrects the fixture, with no production behavior change. The printed
failed home `.tmptpmyOK` was removed and verified absent.

Corrected run `34634fc2-2f45-4a5c-b18b-c7b6a4b5cca4` passed all seven native cases.
Scoped formatting and independent lifecycle review passed; no production correction was needed.

## Reopening Retry Deadline

Run `a22f6d4c-776f-4c94-8819-50c8a69970b1` exposed a timing assumption in the existing
`recovery_reopen_delay_support` fixture: its single timer await returned while the retained
`Instant` deadline still refused construction. Recheck that same fixed deadline after each wake,
matching production admission, instead of treating wake-up as proof of elapsed time. The early
retry refusal assertion and real one-/two-second backoff remain intact; production is unchanged.
The printed failed home `.tmpiVbZk7` was removed and verified absent. The aborted test also creates
an unprinted foreign-candidate fixture; any residue from it has ambiguous ownership and must not
be swept. Independent review accepted the deadline-loop correction.

## Extended Preparation Retry Fixture Budget

Run `43092be0-1ea1-4ae8-848d-f9dc1c6cabc2` aborted the first native recovery case at
`startup_owner_support`'s fifteen-second watchdog after adding repeated complete preparation
attempts. The existing fixture already approached that total budget. Give this extended
session-publication fixture a thirty-second watchdog while retaining five-second per-scenario
retry bounds; other startup fixtures keep fifteen seconds. This changes no production timeout.
The printed failed home `.tmpBCD21U` was removed after verifying its exact temporary path.

Corrected run `0d160988-ac08-401d-8183-422ea887fa9f` passed all twenty-one cases; the extended
case completed in 17.617 seconds. Independent review accepted the bounded test-only adjustment.
