# Initial Composer Activation Custody

## Interrupted Exit Retirement Ownership

On 2026-09-29, phase 706 integration disproved the assumption that retiring the mount/resident
bundle alone releases the selected composer's complete service ownership. The real acquired-shell
test `initial_composer::resident_retirement::shutdown_retirement_retains_refused_service_and_preserves_exact_resident`
reaches an exact ready close and widget quiescence, fails the home, detaches the bundle and drops
its intentionally retained service reference, but exclusive retirement still returns pending.
Nextest run `fbc6ab0c-9ff9-40f6-8434-b7d2c5443512` reproduces this; the app library check passes.

`InitialComposerCandidate::prepare_selection` in `initial_composer/activation.rs` retains a service
clone. `SelectedShellPrepared::from_acquired` in `shell/host/selected.rs` carries that candidate into
the shell's acquired custody, and `MainWindowShell::publish` does not release it. This is additional
ownership beyond the detached mount/resident bundle. `retire_clean_window_close` correctly refuses
while that clone exists. The retained candidate also owns failed-generation source handles.
Independent lifecycle review confirmed this blocker and withdrew its preliminary source-only acceptance.

The Operator authorized the correction. Phase 706 now checks exact service identity and settled
activation/preparation state after resident fencing and mount detachment, then releases only the
construction owner's duplicate service reference. Both acquired and restored shells use this
handoff. Opening, claim and source records remain under shell custody for phase 702 whole-shell
recovery. No initial-candidate abandonment runs against Failed storage and exclusivity remains strict.

Independent lifecycle review accepted the corrected boundary. Nextest run
`e680888e-6737-4612-9951-243b4e4766a1` passed eight focused shell tests, including acquired/restored
failed-store retirement, retained worker/service refusal and retry, focus/editor preservation and
transferred-facts readiness. Run `626fa238-923b-481a-a1a5-ebb0a7b72734` passed four native request-owner
tests; the app library check passed. This evidence accepts resident service retirement, not complete
old-generation retirement or interaction reopening.

The untouched empty acquired-shell variant has a separate widget ownership defect, qualified on
2026-09-29. Removing the helper's text insertion reproduces Ready draft flush with one pending
index intent after 512 bounded drive passes (run `b54350be-af18-4ef2-b789-1ebc09bb62e3`). Repeated
input notifications do not resolve it. The restored fixture contains saved text and is not empty
coverage; replacing empty text explicitly also mutates the binding and is not a valid workaround.

Temporary instrumentation captured `Geometry(Busy)` from index preparation and then confirmed
the geometry owner still holds an Index job while the widget has no active geometry job (runs
`77dd7651-e162-4ac6-a94b-e037f1968c3f` and `20288098-d350-4d2f-8dcb-e968d87b9636`). The inspected
immediate empty-target path retains the geometry owner's ongoing index but clears the widget's
job tracking. The stranded Index state is observed; this originating transition is inferred from
source and was not directly traced. See the [source diagnosis](../memory/github.com/berylorg/gpui-text-input/commit/45c200a5e78f299f2f6436cda75097c75253eca7/empty-target-index-custody.md).
The probes and fixture changes were removed. Correct the widget's index ownership transition and
verify untouched empty acquired/restored residents before claiming complete running-shell coverage;
keep exact retirement and quiescence checks unchanged.

Phase 290 assumed the accepted composer activation, selected-editor preparation, hidden shell,
and window-abandonment components could be connected directly for New Window. Source inspection
and independent review on 2026-09-05 invalidated that integration assumption before source edits.

## Decisive Evidence

- `SyndicComposerHost::activate_unpublished` in
  `crates/beryl-app/src/composer_host/activation.rs` can retain an opened durable candidate after
  cancellation, restoration mismatch, or initial seed failure without returning an activated editor.
- `dispose_composer_service` in
  `crates/beryl-app/src/composer_host/lifecycle/service.rs` treats an absent publication lane as
  complete and clears `active`; it does not abandon that ordinary fresh candidate session.
- `MainWindowComposerSlot::drive_retirement` in
  `crates/beryl-app/src/main_window/composer_slot/retirement.rs` performs typed fresh abandonment
  for pending replacement custody. Slot construction requires an already bound selected host,
  and replacement activation requires a prior selection, so this does not cover initial activation.
- Phase 238 window abandonment cannot substitute for candidate retirement: the reused-thread
  Syndic participant is validation-only. Clearing the host first can lose the cleanup obligation.

## Required Correction

The existing app [shell lifecycle](../../crates/beryl-app/doc/design-shell-lifecycle.md), under
Startup And Activation and Prepublication Window Abandonment, requires typed abandonment of an
unmutated unpublished target and continued custody for ambiguous outcomes. No target-design
change is required.

Implement and independently accept initial-activation custody as Phase 295 before resuming
Phase 290. Its boundary is exact selected-editor transfer or settled typed candidate retirement,
with bounded custody preserved across cancellation, preparation failure, stale completion, and
pending reconciliation before window cleanup is released. Reuse the existing typed Syndic
primitives; do not substitute ordinary service disposal or manufacture replacement selection.

## Accepted Implementation And Verification

Phase 295 implements `MainWindowInitialComposer` with exact acquisition and reservation ownership,
prepared-open and retirement commands, and retained reconciliation handles. Its prepared result
transfers directly into the accepted hidden shell with the existing reservation. Hidden construction
failure and close before publication return candidate retirement custody; window abandonment is
admitted only after candidate retirement settles. Pending or nonfresh outcomes retain custody.

Review and tests identified three details necessary to make that boundary usable: a disposed open
classification must remain terminal across retries; prepared-editor handoff must transfer cleanup
custody into the shell; and fresh retirement must use the opened candidate's forked root/history,
not the durable selector's history. Retained handles use the store's ordinary reconciliation retry.

Independent semantic review accepted production ownership transfers and all 11 new test cases.
The final `cargo check -p beryl-app --lib --features test-faults --locked` passed. The affected run was:

```text
cargo nextest run -p beryl-app --test phase295_initial_composer --test phase289_main_window_shell --test phase285_selected_composer_preparation --test phase238_window_abandonment --test phase236_window_acquisition --test phase141_syndic_composer_host --test phase177_main_window_composer_slot --test phase186_pending_composer_activation --features test-faults --locked --test-threads 1 --no-fail-fast --status-level pass
```

Run `bfba9dfd-55ef-42e5-91ef-3c429d5ec247` completed in 123.224 seconds with 71 passed, two failed,
zero skipped, exit 100. All 11 final Phase 295 cases passed, including real hidden-host transfer,
post-native failure, close-before-publication, cancellation before/after open, seed/configuration
failure, exact retry and acknowledgement loss, disposed retry, mutated-candidate retention, stale
claim rejection, foreign-service isolation, and repeated reused-thread preservation and release.

Both failures were in the existing Phase 186 target. Its obsolete offscreen geometry assertion was
corrected to the accepted full-size hidden overlay and passed targeted verification. Its zero-height
root received a finite height; Phase 296 subsequently corrected and accepted the separate sparse-index
release regression, as recorded in the [release-fence record](composer-release-fence.md). The Phase 295
aggregate is not reported as fully passing. Review confirmed that the failing dispatch/fence production files
are unchanged by Phase 295 and that its shared activation-tail extraction does not alter their path.

The Phase 186 fixture also includes the previously uncommitted `SyndicStorage::clone()` adjustments
required by the accepted non-`Copy` storage handles. Test processes used `RUST_MIN_STACK=16777216`
locally and restored the prior environment in `finally`. All processes finished and the exact
task-owned log was removed; no task resources or permanent environment changes remain.
