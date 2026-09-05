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

Readiness also found no target per-window notice arbiter or widget, so their independent acceptance
precedes the close-failure contribution. Exact stop and typed session removal exist, but the full
noninterruptible-operation wait and close-time continuation cancellation still require inspection.
The [plan](../plan.md) records these prerequisites; ordinary close remains unimplemented.
