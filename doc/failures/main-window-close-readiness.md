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
this correction was subsequently accepted with the evidence below.

The last complete locked `phase302_resident_close_flush` run passed three of eight tests.
A later focused run passed the long-text resident selection/copy/scroll, mutation rejection,
history, and failure-release case but still failed the same-update admitted-edit case on request
identity. The locked app library check with `test-faults`, targeted formatting, and semantic source
review passed before those test findings; they do not establish phase acceptance. Shared lifecycle,
mount, and submission regression targets have not run. Implementation stopped at the separate
[pristine-publication prerequisite](pristine-editor-publication.md); temporary diagnostics were
removed, and all eight meaningful tests remain for resumption.

## Request Sequence Correction

Phase 305 now preserves the host high-water mark across binding advances and removes the mounted
dispatcher's separate counter. Direct transition, exhaustion and fresh-generation tests pass.
The focused resident close selection/copy/failure-release regression passes with a 32 MiB test
thread stack, as does the pre-autosaved submission collision case that previously stalled.
[The implementation record](../audits/code-simplification/implementation.md) preserves run IDs,
commands, independent review, unrelated fixture failures and cleanup limits. These focused results
do not accept the full resident-close suite or the remaining submission-quiescence work.

## Accepted Exact Close-Gate Release

Phase 306 was accepted on 2026-09-07 after the opening and submission-wait prerequisites.
`release_window_close_gate_in_slot` in
`crates/beryl-app/src/main_window/conversation_composer_owner/service/close.rs` now shares the
slot-release and matching service-reservation retirement decision with foreground, worker and
`close_cleanup.rs` callers. It revalidates home/service identity after obtaining the slot and holds
the short reservation mutex across storage-free slot release and exact retirement. Foreground
try-lock behavior, worker locking, lock order and bounded cleanup/backoff remain unchanged.
Pending and failed release cannot retire another attempt. Actual widget release retains its
separate proof; reaching `WidgetReleaseRequired` does not synthesize that proof.

The final `resident_close_flush` run `68525a1d-6cd6-48e3-a926-2ac00d42f7e0` passed all ten cases,
none skipped, in 9.421 seconds. Two new cases force real busy-slot foreground-to-worker release
and remove the actual mounted window. They verify independent disabled state, stale predecessor
isolation from a later reservation, exact unmounted reservation release, unchanged active session
and revision, and eventual weak-service release. The eight retained cases preserve stale,
idempotent, pending-publication, failed-disposal and resident-interaction assertions.

```powershell
cargo +stable --config .cargo/local.toml nextest run -p beryl-app --features test-faults --locked --config-file .cargo/local/close-release-nextest.toml --test resident_close_flush --test-threads 2 --no-fail-fast
cargo +stable --config .cargo/local.toml check -p beryl-app --lib --locked
```

Verification used process-scoped `RUST_MIN_STACK=33554432`, restored afterward, and a temporary
30-second per-test timeout. The locked library check, `test-faults` production/test compilation,
targeted formatting and diff checks passed. Independent semantic review traced all callers,
reservation/slot lock ordering, pending custody, distinct interaction and widget-release gates,
the repaired fixture and new regressions without a blocking finding.

The initial run `d9a3c182-8dc4-4ac2-96a9-dba910eac442` passed six of ten cases. Three retained
host fixtures still assumed that clean opening or publication directly yielded close readiness.
Their shared helper now publishes only dirty state, then authenticates saved state while proving
unchanged revision/binding and zero publication custody. The new drop test initially drove a
removed window; its post-drop pumping now uses the window-independent executor. No production
semantics changed to accommodate those test failures.

The temporary configuration was removed and named fixture/process scans were clean. The existing
[policy-blocked directory](../audits/code-simplification/implementation.md) was untouched.
This accepts the shared release decision, not the remaining integrated resident-close or ordinary
OS-window close boundaries.

## Accepted Resident-Preserving Close Lifecycle

Phase 302 was accepted on 2026-09-07. Fresh independent review traced host flush/close, mount
admission and settlement, service/slot fencing, worker/drop cleanup and actual final widget release.
WindowClose retains its exact barrier and close ticket after authenticated readiness. Already
admitted work settles first, read-only interaction remains coherent, and final disposal requires
explicit authorization and the exact widget-release fence. Failure releases only its own attempt;
stale settlement cannot dispose or re-enable a replacement editor.

Integrated run `e6889489-289a-48f7-8a64-9a345db9a3ac` passed 62 selected cases in 71.216 seconds:
ten resident-close, seven saved-opening, 28 lifecycle, 13 mounted-submission and four native-lineage
cases. Nine other mounted cases were intentionally outside this acceptance boundary. The run
combined the [opening integration selection](pristine-editor-publication.md) with every
`resident_close_flush` case using locked Cargo, `test-faults`, two test threads, a process-scoped
32 MiB stack and a temporary 30-second per-test timeout.

Direct assertions cover exact editor/candidate/history retention, caret/selection/scroll snapshots,
selection/copy/wheel interaction while mutations and submission are blocked, already-running save
and same-update admitted edits, repeated/stale attempts, failure restoration with independent
disable preserved, exact authorized final disposal, cancellation and subsequent explicit retry.
Ambiguous close publication remains unready and retains custody through release/reconciliation.
Busy-slot worker release and real mount-drop cleanup retain their distinct ownership boundaries.

Terminal-unavailability and proven-noncommit coverage additionally relies on shared-path tests and
source proof rather than dedicated WindowClose fault duplicates. Publication execution clears only
the proven-noncommit lane and leaves adopted binding/dirty state intact. Failure settlement retains
the WindowClose ticket until explicit gate release. Terminal publication retains its prepared
evidence and marks the active session unavailable; close release cannot rearm publication, and
`begin_flush` rejects unavailable sessions. Independent review found this evidence sufficient and
root confirmed the unavailable admission guard.

No source or test edit was needed for this final integration phase. Current `test-faults` test
compilation and diff checks passed; the Phase 306 locked library and formatting checks cover the
unchanged source at `daa1f3e`. Temporary `resident-close-integration-nextest.toml` was removed,
named fixture scans were empty and no selected test processes remained. The earlier policy-blocked
directory was untouched. This acceptance excludes OS close integration, backend active-work
coordination, durable window-session removal and application Exit.
