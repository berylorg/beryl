# Ordinary Close Requires A Resident-Preserving Flush

The proposed Phase 298 integration cannot use the existing WindowClose draft flush unchanged.
Ordinary close must preserve the resident editor when a later session obligation fails, as required
by [main-window behavior](../features/main-windows/design.md).

At the readiness inspection, `ComposerHostFlushPurpose::disposes_session` in
`crates/beryl-app/src/composer_host/lifecycle/mod.rs` classified every purpose except Submission as
disposing. `advance_flush_disposal` in the sibling `flush.rs` cleared the active editor and its
mutation/history state before returning Satisfied; the composer mount subsequently removed the
contribution. A successful draft flush therefore could destroy state that a subsequent failed
session removal must preserve. Reusing Submission would misstate purpose and does not establish
the required continuing close mutation gate.

The correction is a separate resident-preserving close-flush prerequisite before mounting ordinary
close. It must preserve selection, scrolling, and copying while mutations are frozen, then support
the authoritative success or failure settlement. Existing flush publication and reconciliation
remain reusable; this record does not prescribe the replacement API.

The per-window notice prerequisite is now accepted. Exact stop and typed session removal exist,
but the full noninterruptible-operation wait and close-time continuation cancellation still require
inspection. The [plan](../plan.md) records these prerequisites; ordinary close remains unimplemented.

## Resident Close Verification

Phase 302 separates retained draft readiness from final disposal and keeps an exact close gate
through later obligations. Directly calling existing synchronous capture and disposal helpers from
the mounted coordinator was invalid: those helpers perform storage work while holding the slot
mutex. The corrected coordinator uses background work, nonblocking foreground admission, and exact
attempt completions. Final failure must also restore the independent enabled state and clear only
the failed disposal stage; otherwise it enables an unrelated disabled input or permanently blocks
the next close. Native-lineage refresh must check the close gate before applying its input fence.

Teardown does not manufacture a widget-release receipt. Normal success uses the actual released
widget; bounded supplemental cleanup stops at that boundary. Admitted ambiguous HomeCommands
already belong to the HomeStore reconciliation registry: `ReconciliationSlot::install` retains the
descriptor and registry core independently of the composer-service Arc. This established owner
avoids inventing another retention mechanism.

Mounted verification exposed a remaining request-sequence defect. `publish_capture` in
`crates/beryl-app/src/main_window/composer_slot/lifecycle.rs` calls `replace_binding` in sibling
`dispatch.rs`, which resets dispatcher request IDs to zero. Publication keeps the same resident
host and its request high-water mark, so later selection or copying can fail with a duplicate,
stale, or out-of-order request identity. Same-session publication must preserve that sequence;
this correction remains unfinished.

The last complete locked `phase302_resident_close_flush` run passed three of eight tests.
A later focused run passed the long-text resident selection/copy/scroll, mutation rejection,
history, and failure-release case but still failed the same-update admitted-edit case on request
identity. The locked app library check with `test-faults`, targeted formatting, and semantic source
review passed before those test findings; they do not establish phase acceptance. Shared lifecycle,
mount, and submission regression targets have not run. Implementation stopped at the separate
[pristine-publication prerequisite](pristine-editor-publication.md); temporary diagnostics were
removed, and all eight meaningful tests remain for resumption.
