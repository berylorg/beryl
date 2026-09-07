# Composer Marker Admission

## Confirmed Gap

GUI marker insertion reaches draft staging without the required marker writer admission. This
predates the recovery-view switching change and is not resolved by waiting for more GUI frames.

The composer slot dispatches through `SyndicComposerHost::begin_mutation`, whose production path
selects ordinary admission. The marker-readiness branch and marker-specific entry point in
`composer_host/mutation/execution.rs` are available only with `test-faults`. Translation emits an
insertion marker effect, but normal staging rejects marker effects when writer admission is absent
(`syndic-storage/src/draft_piece/staging/prepare_begin_page.rs`).

All three affected mounted tests report the same owner error: draft mutation staging failed with
an invalid staging request. They retain their expected marker counts and now report the owner
error at the failing assertion. Their fixtures are not repaired by suppressing the error or
extending fixed frame loops.

## Suggested Correction

Connect marker-affecting GUI edits to the existing exact marker-readiness lifecycle before
`MutationBegin`, then consume its proof through marker-aware admission. Preserve selection and
operation identity, bounded preparation, cancellation, failure and move-only custody. The
[existing proof-composition boundary](syndic-draft-marker-cross-domain-proof-composition.md) and
[app contract](../../crates/beryl-app/doc/design-catalog-and-composer.md) already define the required
mechanism. Do not bypass storage validation, fabricate readiness in the app, or add a new policy.

This is a proposal, not an implemented correction. The Operator requested that non-GUI flaws be
identified and clean solutions suggested separately from the GUI switching work.

## Evidence

After correcting four stale flush-test drivers, the final serial GUI run passed 32 of 35 tests,
with zero skipped. The remaining failures are:

- `mount_retains_one_coherent_contribution_until_exact_publish_and_disposal`
- `mounted_terminal_anchor_marker_run_remains_proven_for_successive_edits`
- `recoverable_mounted_autosave_releases_rearms_and_does_not_spin`

Run `b5a20fd7-9b8f-4adc-bb5f-3d9d29723a9e` used `cargo +stable --config .cargo/local.toml nextest run
--locked -p beryl-app --features test-faults --test main_window_composer_mount
--test pending_composer_activation --test main_window_composer_slot --test-threads 1 --no-fail-fast`
with process-local `RUST_MIN_STACK=16777216` and TMP/TEMP in the ignored task directory. Independent
semantic review confirmed both the unchanged assertions and the admission gap. No production
marker or execution code changed during this verification.
