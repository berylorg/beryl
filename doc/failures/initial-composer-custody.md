# Initial Composer Activation Custody

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
root received a finite height, but the separate sparse-index release regression remains unresolved;
the [release-fence record](composer-release-fence.md) preserves that gate for Phase 296. The aggregate
is not reported as fully passing. Review confirmed that the failing dispatch/fence production files
are unchanged by Phase 295 and that its shared activation-tail extraction does not alter their path.

The Phase 186 fixture also includes the previously uncommitted `SyndicStorage::clone()` adjustments
required by the accepted non-`Copy` storage handles. Test processes used `RUST_MIN_STACK=16777216`
locally and restored the prior environment in `finally`. All processes finished and the exact
task-owned log was removed; no task resources or permanent environment changes remain.
