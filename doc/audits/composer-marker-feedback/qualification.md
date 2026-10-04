# Mounted Marker Refusal Qualification

## Boundary

This record qualifies direct marker replacement through the existing mounted composer owner and
Notifications contribution. It follows the [readiness review](readiness.md#derivable-next-boundary)
and the [composer acceptance contract](../../features/composer/design.md#engineering-rigor).
The source baseline is `b79fbe51`; qualification changes test fixtures and evidence only.
Clipboard paste mounting and the complete marker-feedback tracker item remain pending.

## Coupled Large Draft Witness

The four cases in
[large_draft_feedback.rs](../../../crates/beryl-app/tests/notice_mount/large_draft_feedback.rs)
use a patterned 3 MiB draft built from 96 bounded chunks by the existing scale fixture.
Its extracted candidate seed leaves the durable draft pristine for fresh shell acquisition and
resumes the exact candidate session/open-operation identity. The mounted editor uses bounded
page and geometry allowances already exercised by the scale tests; no production setting changes.

A real published Asset supplies a marker at the origin and another at EOF. The EOF marker is
selected backward, with the anchor after the atom and the head before it. This follows the actual
inline-marker replacement API; replacing a nonempty text selection belongs to another consumer.
The origin marker is outside the viewport. Its absence from every resident cache is not asserted.

Each refusal compares the complete selection identity, immutable root, candidate history authority,
widget history frontier, directed selection and realized marker count with the predecessor.
The same immutable root preserves both marker associations and all draft text, including ranges
outside the bounded resident pages. No complete draft string is read into the mounted witness.

Size and capacity cases exercise four distinct operation keys and dismissal cycles. Each settled
refusal releases host settlement custody; retained terminal identities stay within their fixed bound.
Widget high-water bytes, items, pages and geometry remain within configured budgets, and resident
text stays smaller than the draft. The notice has no command and dismissal does not recreate it.

After healthy refusal, Home/End navigation remains usable on the range-backed draft. A one-byte
text edit preserves both markers; undo restores the exact predecessor root and redo restores the
edited root. The size override remains in place. In the capacity case, the zero-head test override
is restored to the production allowance before healthy edit/history commands; this restores the
fixture's ability to admit readiness heads and is not a real concurrent reservation-release witness.

## Failure And Admission Evidence

Operation size and capacity use test-only `DraftMarkerAdmissionLimitsV1` retained-limit overrides.
They qualify exact owner dispatch and visible typed feedback. They do not simulate an actual
production oversized marker population, real concurrent shared-capacity release, or a 3 MiB text
draft exceeding the production marker-charge allowance. Text length and marker charge are distinct.

Actual production isolated-before-aggregate classification remains controlled by Syndic's fixed
V1 profile and the existing
[admission refusal cases](../../../crates/syndic-storage/tests/draft_marker_admission_submission/refusal.rs).
The [readiness coverage](readiness.md#evidence-coverage-and-remaining-gaps) separates those component
witnesses from mounted transport overrides. No production admission limit or profile is increased.

Storage refusal uses the real Home-store `BeforeCommit` fault, preserves the exact editor and
releases settled custody. The test does not clear the resulting health gate or claim same-home
recovery, healthy editing or history after storage failure.

The real `AfterPersist` fault produces persistent admitted-work-unavailable feedback. Exact host
binding and one retained settlement custody remain unchanged; retry is rejected. Dismissal is
rejected, and notice-owner retirement removes presentation without changing editor identity,
feedback key or retained ambiguous custody. It does not fabricate proven noncommit or retry.
Existing [completion witnesses](../../../crates/beryl-app/tests/notice_mount/mutation_completion.rs)
continue to cover actual retained build/finalization and postcommit presentation failures.

Every case retires the notice owner, removes the window through established teardown and checks
that weak composer, input and service handles no longer upgrade. Healthy settled custody release
and ambiguous custody retention before teardown are distinct assertions.

## Verification

Accepted on 2026-10-04 with 31 distinct successful focused cases:

- All 26 existing notice cases and the large determinate storage-refusal case passed together.
- The large size-refusal case passed, including four refusal cycles and healthy edit/history.
- The large ambiguous-storage case and the affected shared EOF scale regression passed.
- The corrected large capacity-refusal case passed, including restored healthy edit/history.

Independent semantic review inspected the raw passing records and accepted reuse of unaffected
cases. Extending the test-local finite wait from 512 to 4096 rounds and adding milestone logging
preserved their successful predicates; restoring the capacity override runs only in that case.
The final capacity result uses the frozen corrected source. Earlier failed attempts are excluded.
Formatting and documentation checks passed. Clipboard acceptance remains open.

Local qualification uses locked stable Cargo, LLVM linking, one build job, serial nextest execution,
zero normal debug information and no incremental compilation. Process-local stack and Windows
error-mode settings are restored by the launcher. No native application entry point is launched.
The local widget checkouts match the unchanged formal Git revision pins; existing accepted canonical
production composition evidence is reused. This record does not claim a new canonical build.

Fixture failures and their final correction are preserved in the
[failure record](../../failures/composer-marker-admission.md#large-draft-mounted-refusal-fixture).
Machine-local logs and cleanup ownership are recorded in ignored `ENV.md`; unidentified fixture
residue is left untouched. No Temp-directory sweep is authorized or performed.
