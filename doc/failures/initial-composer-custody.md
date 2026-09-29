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
The probes and fixture changes were removed. Phase 762 corrects local target replacement by
including the active index and its exact text/object request in the existing prepared release set.
Commit clears only released jobs. Deferred targets retain indexing and rejected preparation
preserves prior custody; no worker or quiescence exception was added.

Seven public-path widget regressions cover empty/nonterminal replacement, queued/dispatched text,
delayed object response, refusal and deferred-index preservation. Disabling local index retirement
fails five affected tests (run `7480c0a2-72d6-4474-8537-3eefe5edf9f8`). Correct retirement removes an
obsolete queued object response before delivery; its old delivered-response-counter expectation
was replaced with exact custody cleanup assertions. The marked-replacement fixture now verifies
bounded index response processing after successor target publication, rather than assuming the
superseded index arrives first. Its exact settlement, composition and selection checks remain.

Independent lifecycle review accepted the correction. All 550 widget tests pass in run
`ae089efa-40e9-447e-b12a-092e449cffc3`; default-feature library compilation also passes, using LLVM,
one job, no debug information and nonincremental builds.

Phase 763 published widget `2c3ae40ced71301a0ce5286aaef90ce9d594a5ac` and settings
`5b2e072b55e3ad99534926721f4aa892041cd869`. Root pins and the canonical lock now resolve one
widget, settings and GPUI package each. Isolated checkouts without `.cargo/local.toml` verified
the published Git graph; canonical settings and app library checks and locked metadata passed.
The root lock changes only the affected dependency identities and removes the older duplicate widget.

Native tests now cover untouched zero-byte acquired and restored residents alongside existing
edited cases. The empty restored fixture skips text mutation and requires the clean WindowClose
result `State(CloseReady)`; requiring a captured publication was invalid for an unchanged draft.
All 16 focused retirement/restored-shell tests pass against canonical dependencies in run
`2f66e478-bc05-4172-bd90-36be3086379f`. Exact held-worker/service refusal and retry, shell/editor/focus
identity, transferred-facts readiness, strict quiescence and empty disposal cleanup remain checked.
Independent review accepted. This does not establish complete running-shell recovery;
opening, claim and source custody still require phase 702 whole-shell handling.

Phase 764 releases that settled shell construction custody after exact resident retirement.
Acquired opening state, restored claim/source state and threadless source handles are replaced by
window identity and placement facts plus the existing process-owned native reservation. The latter
contains no home/storage handle and remains held until native cleanup. Refused resident retirement
retains construction custody; successful repetition is idempotent. Retired shells cannot authorize
startup publication/disposal or interaction release. No failed-store abandonment or claim write runs.

Nextest run `ba6bbbfc-aeae-4913-a086-58b437245fd5` passed 20 focused native tests, including empty
and edited acquired/restored editors, held worker/service refusal, exact editor/focus preservation,
threadless retirement, retained native capacity and unchanged threadless home revision. The broader
Exit fixtures initially assumed they could reopen a retired shell to test graph admission, then
reuse startup disposal for teardown. Runs `dbf0edf4-6309-4b41-9a52-7118c9e35b2f` and
`781f7927-2d76-4e38-a8c1-ae0fa54641e9` rejected those assumptions. The missing-gate check now runs
before retirement; later reopening is explicitly refused. Test teardown removes its retired,
editor-free threadless windows directly and closes the retained home through the applicable
healthy/failed path. Run `96237136-0f7f-4337-bbfc-e328c5062c33` identified the reconciled healthy
fixture's need for ordinary close instead of failed-home retirement.

Final run `bbdbd838-9445-4a7a-b75e-c96d86ef351e` passed all five native Exit cases, including
noncommit, postcommit failure, indeterminate reconciliation and settlement unwind. The app library
check passes and independent lifecycle review accepted. Fresh shell bindings, remaining graph
ownership and coherent recovery publication remain phase 702; this is not full recovery acceptance.

Phase 765 corrects an admission ordering gap found while continuing replacement composition:
the running owner previously retired residents and shell construction before the graph worker
checked the supplied generation and failed-home state. An invalid request could therefore alter
shell custody before being refused. The existing service-owner predicate is now shared with an
early read-only check, before resident retirement, pending-result retention or worker transfer.
The worker retains its own check. No new state, recovery capability or cleanup write is introduced.

Run `c85be3dd-8c12-43ab-90d5-9bd94edef6d9` passed 14 native Exit and service-retirement tests.
After review suggested directly checking healthy refusal on an otherwise gated, intact shell,
run `e6a9c4ab-3123-42a0-a0ba-f3fa6f4ddb36` passed all five affected native Exit cases. Both stale
and healthy requests preserve construction custody, session evidence and service availability.
Existing healthy reconciliation fixtures now expect synchronous admission refusal rather than a
worker-returned refusal. The app library check and independent lifecycle review passed. Complete
fresh binding and publication remain phase 702 work.

Phase 766 closes the corresponding resident-preparation admission gap. Preparation previously
excluded only pending graph retirement, allowing missing or failed retirement results to reach
candidate authentication. It now uses the same exact-request successful-retirement predicate as
candidate session settlement, before taking either input or creating preparation work. The
redundant request checks and weaker pending-only check were removed; no production state was added.

Native component fixtures exercise absent, pending and failed results in sequence, preserving
candidate generation, retired close custody, restoration facts and disabled input after each
refusal. A test-only result setter then supplies successful retirement for the existing preparation,
cancellation, stale-request, window-loss, capacity and attachment cases. This fixture injection is
component evidence; actual graph retirement remains covered by the native Exit fixtures.
Run `d18abfa9-8e53-451d-ba45-fde19bd9a91b` passed all 16 focused native tests. App library compilation
and independent lifecycle review passed. Fresh shell bindings and full recovery remain phase 702.

Phase 767 preserves the exact fixed-size session window record previously discarded by candidate
claim validation. Slot/service reconstruction returns it to the resident source, which checks the
complete record during validation and text/object servicing. Successful owner attachment returns
it with the candidate and fresh close ticket. This adds no GUI storage read or old-generation
handle; one bounded immutable record supplies target, placement, selection and revision for later
shell installation. Failed attachment preserves existing custody and interaction fences.

Run `f519aebd-aa66-4e0f-8a69-f5ff2d66499a` passed eight focused source/slot/service tests, including
full-record preservation and a changed-placement/revision refusal with an unchanged claim.
Run `cf3f65f6-d3a9-47df-ab21-b41c8db9f62b` passed all 16 focused native recovery/Exit tests.
App library compilation and independent lifecycle review passed. Native attachment checks record
identity, selection and target presence; complete-record equality is checked at the source boundary.
Shell installation, fresh draft settlement and complete recovery remain phase 702 work.

Phase 768 adds selected-shell adoption around the accepted mount boundary. Exact gated shell,
mount, resident, draft and authenticated candidate identities are checked before adoption. On
success, the shell synchronously replaces retired construction facts with one fixed-size window
record and fresh selection, transfers its existing reservation and renews the retained draft ticket.
No worker, queue or storage write is added. Old appearance prevents interaction release, and the
recovered shell remains excluded from startup publication/disposal.

The acquired-shell fixtures exercise edited-draft retirement, adoption, exact full-record
installation, unchanged editor/focus/reservation occupancy, duplicate refusal and foreign-root,
stale-ticket and capacity refusal. A widget capacity refusal cancels preparation: its resources are
explicitly drained, not immediately retried. Fixture corrections also moved drawing outside the
root update, captured predecessor identity after edit flush and explicitly released the controller
at teardown. That teardown is fixture cleanup, not evidence of production native destruction.

Run `39db29dc-02bd-43a3-8729-b331fa64653b` passed all 18 existing mount/owner regression cases;
its two new shell cases exposed the fixture corrections above. Final run
`1ea1a73e-153c-42b1-ad25-661bf1e54818` passed both corrected shell cases. Run
`f4eb3da3-b940-4d22-a15a-309764608e06` passed all 14 focused existing shell retirement,
shutdown-draft and startup-interaction cases. App library compilation and independent lifecycle
review passed. Restored-shell adoption, threadless binding, fresh appearance/notice ownership and
running-owner composition are not accepted by this component evidence; full recovery remains 702.

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
