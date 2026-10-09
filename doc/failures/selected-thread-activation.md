# Selected-thread activation

## Mounted Selector Focus Was Missing From Recovery Capture

The picker returns focus to its owning Thread Switcher trigger when it closes. Same-home
ordinary recovery captured only composer-input or shell focus, so it omitted this still-live
trigger handle. Native run `d3e0fe28-7f48-400d-b83d-f9f94f0401da` aborts the original save-noncommit
preservation assertion after fresh prior recovery: the selected editor and original Page custody
survive, but focus returns to the input instead of the selector.

Recognize the exact currently focused selector in the existing recovery focus capture. Preserve
the same handle through the existing switcher reset and original recovery completion; do not
introduce another focus owner or broaden resource custody. The native scenario retains its exact
owner-focus expectation and original durable, selection, editor and positive Page-release checks.
The corrected original two-round native scenario passes in 7.391 seconds in canonical run
`1a007691-24e7-4fdd-905a-47f664266cfc`; the broader run's separate Current fixture baseline
correction is not a production focus defect.

## Completed Pure Wake Retained Its Task Slot

The activation timer returned for an absent or suspended operation before clearing its own task
slot. Render-driven completion could finish the operation while its scheduled pure wake remained;
the later callback then retained a completed task indefinitely. Canonical run
`b1f3cb93-c313-4636-8a52-0ccf22671915` exposes this through six actual mounted activation cases:
the operation and failure are absent, but terminal activation custody never clears.

The callback now authenticates its exact wake identity first, clears only that matching wake and
task, then checks whether the same live operation may resume. A stale callback cannot clear a
replacement task. Worker, source, lease and disposal ownership are unchanged. Corrected mounted
activation passes all eleven focused cases in canonical run
`4a1b15f4-be77-486a-acbb-b4f2d97a43e7`, including cancellation and indeterminate reconciliation.

The shared New Thread fixture also treated a settled durable lease and runtime phase as complete
GUI readiness. Consecutive creation/reuse tests could inject their next fixture lease while the
matching terminal wake still owned activation custody; the real command correctly refused before
consuming that lease. Diagnostic run `40198238-7b16-4bbc-9027-3963a89a360a` retained the injected
lease with no original operation. The fixture was strengthened to wait for its real read/activation
drain predicate under the unchanged deadline. Run `377a641a-b573-4c96-928b-36f56d6b01cb` records
the precise incomplete-ready cut and passes original pristine-root reuse without a command retry
or production admission change. All 55 original runtime-setup and history cases pass in that run.

That predicate also counted unrelated periodic inspection workers. Navigation regression run
`dfe3fad9-14c8-424d-9dea-11d905f418a5` exposed a completed creation with no original operation,
lease or activation task, while continuous inspection prevented an all-reader idle sample.
Confirmation readiness now uses the exact original activation-custody predicate, including its
terminal GUI task, alongside the unchanged process/runtime predicates and twenty-second deadline.
Read drainage remains a separate retirement requirement. Run
`62145818-6e6a-4d10-a983-ec5d4a5e717f` passes all sixty runtime/setup and ordinary-control cases
and records the incomplete original-custody cut before subsequent commands become eligible.

## Simulated Pointer Input Outran Its Fixture Lease

The isolated navigation fixture installed its original selection lease after simulated pointer
input. GPUI drains background work between mouse events, allowing original preparation to reach
admitted startup before that assignment. The same regression run failed the first pointer move;
the late fixture lease then retained custody without an operation.

Reserve one exact target-qualified lease before input in the existing test-only fixture reader.
Original admitted startup consumes it and still validates the invoking window. Refusal, failure,
stale completion, retirement and teardown release unused reservations. The reservation is excluded
from production custody; production admission is unchanged. Normal frame drawing precedes actual
pointer/Enter/Space input. The corrected round trip passes in 2.80 seconds in the sixty-case run,
retaining exact selected claim, title, transcript, history, geometry and focus assertions without
retrying commands or extending the original deadline.

## Renderer-owned admission boundary failure

- Scope: startup restore and explicit existing-thread activation after sliding-window transcript residency, prepublication preparation, and media admission were introduced.
- Invalid assumption: a selected thread could remain staged until renderer-owned Markdown/media admission reported every completed media candidate settled.
- Evidence: live testing on June 12, 2026 against copied real Beryl home `real-home-copy-20260612-181411` and thread `019eba02-fba7-72e2-8e4e-18ab69e5e978` showed the backend fetch completed and staged 52 turns while the GUI either stayed on `New conversation` at startup or kept rendering the previous selected thread during explicit switch. Diagnostics showed `backendWorkReceivers = 0`, staged target id present, structural readiness settled, prepublication settled, but media admission still pending.
- Why it failed: selected-thread publication was an app-state transition, but liveness depended on renderer-owned Markdown parsing, media admission, and cache scope belonging to the currently visible transcript. The previous selected thread could continue driving renderer cache work while the hidden staged thread waited for admission state that was not required to select coherent resident text rows.
- Course correction: selected-thread publication now depends on activation-owned readiness: fetched full-detail resident rows, structural row presentation readiness, and bounded prepublication preparation. Renderer-owned Markdown/media admission remains as post-publication warming and stable row-owned media placeholder/fallback work, but it cannot veto selecting the fetched thread.
- Affected design docs: `doc/features/conversation-threads/design.md` and `doc/features/transcript/design.md` now distinguish activation-owned publication readiness from renderer-owned media admission.
- Affected tests: keep `selected_thread_publication_is_not_gated_by_renderer_media_admission` in `conversation_shell_source` and retain live diagnostic coverage through `read_ui_state.pendingActivation` and `read_ui_state.markdownCache`.

## Build-target verification failure

- Scope: live diagnostic verification of selected-thread activation fixes.
- Invalid assumption: `cargo build -p beryl-app` was sufficient before launching `target\debug\beryl.exe`.
- Evidence: after rebuilding only `beryl-app`, the diagnostic child still exhibited the old media-gated publication behavior. Rebuilding the actual root binary with `cargo build -p beryl` made the same copied-home test publish the target thread and restore it on restart.
- Why it failed: the diagnostic child runs the root `beryl.exe` binary, not a crate-local `beryl-app` executable.
- Course correction: when live-testing Beryl GUI behavior through the diagnostic child, rebuild the binary package that owns the executable path being launched. For `target\debug\beryl.exe`, run `cargo build -p beryl`.
