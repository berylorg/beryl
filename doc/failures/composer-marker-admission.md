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

The Operator authorized the missing prerequisite mechanisms. Bounded widget evidence/replay,
fresh Asset metadata witnesses, mixed Syndic label assignment, and read-only admitted target
resolution are now implemented and independently reviewed in the local checkouts. These correct
the earlier assumption that app-only wiring could authenticate fresh images and inspect complete
edit evidence before storage begin.

## Typed Refusal And Custody Boundary

App integration review exposed another reason app-only wiring cannot satisfy the existing
contract: public Syndic readiness and assignment APIs erased the required distinction between an
isolated operation exceeding its profile, aggregate temporary capacity saturation, and storage
failure. The source, page-submission and assignment APIs exposed no `OperationTooLarge` or
`CapacityUnavailable` result, and publication submission reduced all refusal causes to `Rejected`.

The app contract requires preserving those typed outcomes; the app cannot reconstruct them from
generic rejection without inspecting private state or inventing policy. Implementation stopped
before app edits under the Operator's technical-plan rule. The authorized correction now preserves
typed refusal causes through Syndic preparation, submission, assignment and reconciliation.
Public-boundary tests verify isolated versus aggregate limits, unchanged prior authority, and exact
cleanup and ambiguous custody. Do not weaken app outcomes or replace source classification with
app-side guesses.

The correction must preserve custody as well as error variants. Actual storage-fault tests showed
that a failed HomeStore health check during local cleanup could overwrite the original failure,
and that a committed command could lose its receipt and later failure through an unavailable
local-finalization result. Preserve those facts without claiming cleanup success or recreating a
retired capability. Independent review also found that retaining a postcommit readiness retry
while releasing its assignment attempt allowed the retry to observe a later assignment and issue
a second proof. Keep the retry's exact attempt exclusive and bind proof issuance to its selected
command. Verify no-mutation retry, competing-attempt rejection, drop-to-cleanup, and real journal
failure reconciliation through the public API.

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
