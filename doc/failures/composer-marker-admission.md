# Composer Marker Admission

## Invalidated App-Only Assumption

GUI marker insertion originally reached draft staging without marker writer admission. This
predated the recovery-view switching change and could not be resolved by waiting for more frames.

The composer slot dispatched through ordinary admission, while marker readiness was available only
with `test-faults`. Translation emitted an insertion marker effect, which normal staging rejected
when writer admission was absent
(`syndic-storage/src/draft_piece/staging/prepare_begin_page.rs`).

Three mounted tests reported invalid staging requests. Suppressing their owner errors, changing
expected marker counts, or extending fixed frame loops would not correct that authority gap.

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

## Remaining Staged Build Settlement Boundary

Production composition exposed another unsupported handoff. The app's
`composer_host/mutation/drive.rs::run_build_command` reads terminal operation status and discards
the HomeStore command outcome's local-finalization capability. That status read does not resolve
writer progress or release and reclaim settled writer authority.

Syndic's public `draft_piece/read.rs::reconcile_draft_piece_command_outcome` performs those actions
but unconditionally requests original fragment pages from ordinal one. The bounded app owns
durable staged authority, not a full fragment inventory. `draft_piece_operation_status_page`
requires actual fragments while fragments remain; an empty callback cannot satisfy it. The
durable staging window exposes fragments only under `test-faults`, and no production authenticated
fragment reader or endpoint-based reconciliation boundary was found.

The proposed correction is a bounded Syndic outcome reconciliation API authenticated by durable
staging/build authority and its exact endpoint. It must consume local finalization, resolve writer
progress, release successful settlement, and establish noncommit cleanup while retaining unresolved
custody. Do not substitute status reads, fabricate fragments, buffer the full edit, or add unrelated
global cleanup to compensate. Independent review and root inspection confirmed the public gap;
implementation stopped under the Operator's technical-plan rule.

The Operator subsequently authorized that API. Its partial implementation captures actual serialized
outcomes and retains finalization/reconciliation/cleanup custody behind an opaque command flight.
Independent semantic review found no demonstrated blocker in that boundary, but acceptance is now
blocked by the [persistent sequence split-height defect](syndic-draft-piece-split-height.md)
exposed by its required long-operation test. The outcome API and app integration remain unaccepted.

## Evidence And Status

The typed-refusal prerequisite is accepted in `a50186a`, with 113 prior marker regressions and 41
final submission, assignment and cleanup cases passing. The resumed app implementation remains
uncommitted and unaccepted. It adds bounded evidence/restart, AssetId-only fresh metadata, target
resolution, shared typed diagnostics, and exact-operation cleanup. Existing marker assertions and
the 256-byte fixture limit remain intact; compact fresh metadata fixed the surface-capacity failure.

Combined run `30310164` passed 35 of 37 tests across `composer_marker_evidence`,
`main_window_composer_mount`, `main_window_composer_slot`, and `pending_composer_activation`.
The latest corrected-fixture run `083f9ade` passed one of three focused tests. Evidence cancellation
passes; the public success case fails with `Restoration(InvalidRoot)`, and mounted coherence remains
at `Retained(Progress(CaptureRequired))`. These remain acceptance failures to investigate after the
public settlement boundary is corrected; they do not replace the independent API-gap evidence.
All runs used locked local Cargo configuration and serial test execution. Task processes exited.
