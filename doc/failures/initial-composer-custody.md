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

The prerequisite remains unimplemented. Production activation, post-open failure, reused-thread
preservation, retry and reconciliation, and repeated resource release need focused evidence.
This diagnosis ran no builds or tests and created no temporary resources or source changes.
