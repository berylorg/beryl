# Boundary And Status

Independent complete-boundary review accepted Checkpoint 7's composed asset/resource and
deferred-cleanup gate on 2026-10-22. The owning
[image feature](../features/image-assets/design.md),
[image system](../systems/image-assets/design.md) and App lifecycle contracts govern behavior.
Automatic titles are an accepted [known limitation](../known-limitations.md#automatic-thread-titles);
CAS modification and historical repair dispatch are outside this work.

# Reusable Consumer Evidence

The historical [readiness inventory](asset-processing-readiness.md) predates the accepted consumers.
Its broad missing-admission/rendition rows are superseded by
[bounded renditions](bounded-image-rendition-qualification.md),
[original-image commands](original-image-command-qualification.md),
[authenticated generated sources](generated-output-source-qualification.md) and
[complete generated finalization](generated-media-finalization-qualification.md).
Their exact unchanged source/native receipts retain their declared limits. This gate adds composed
ordinary retirement evidence; it does not implement another asset reader, decoder/cache or worker.

# Durable Assets And Deferred Cleanup

The six focused State/Home targets cover 72 distinct cases: `assets_v3`,
`generated_media_admissions`, `sidecar_durability`, `sidecar_streaming`,
`sidecar_publication_barriers` and `verified_sidecar_scope`.

Initial run `e9f485ab-140d-4a67-8875-1da39a3ae236` passed 42 cases; 30 fixtures refused to run
without their required explicit ledger environment. With task-owned ledgers supplied, run
`1a9ab82f-f95f-46c4-a1c6-19a7b6c23ba4` passed all 31 selected cases in 11.107 seconds, including
one initially passing case. No code or assertion was changed for this setup correction.

`paged_reference_set_v3_seals_binds_reopens_and_becomes_unreachable` establishes metadata-head
removal after reopen while retaining the sealed set and admitted sidecar. Publication/crash-cut
and streaming cases establish exact admitted bytes and failure/cancellation outcomes. Static
inspection of State asset ownership and Home sidecar modules found no final-sidecar deletion
operation; sidecar publication is create/verify/rename, and owner removal updates metadata.
No garbage collector is introduced. Forty-one ledgered completed fixture Homes were reclaimed
after their successful runs; the evidence logs remain through completion review.

# App Composition Checkpoint

Baseline run `18182e02-3a53-45da-a7d7-7b0d919b00de` passed 76 of 78 selected App/media cases.
The two failures exposed stale fixture authority/source preconditions, corrected below. Neither
failure was waived as a known limitation.

Run `4f87acee-6260-4806-9c04-a0a1f0a54402` separately passed bounded hostile-header refusal and
all three unavailable-repair startup/reconciliation cases; the steering failure reproduced.
These checks use the real existing incomplete-recovery disposition without registering repair
families or dispatching backend history repair.

Run `b1eae81c-526f-4aa4-99cf-2bebab4b2e1b` passed the new native ordinary-Exit fixture in 3.399
seconds: authenticated Ready pixels, repeated-inspection cache plateau, held original Save picker,
retired-admission refusal, joined media/native ledger release and fresh durable asset/reference
reopen. It uses an injected picker result and does not touch the Operator's clipboard or show a
destination dialog. Direct media drain is not its acceptance event. Independent source review
found no blocking architectural issue in this composed fixture.

Those initial native runs still exposed disappearing inspections. Diagnosis established that the
fixture's unnecessary initial wheel detached Tail; later placement adoption legitimately renewed
activation/presentation authority and closed the stale ordinary preview. The final three native
fixtures retain initial Tail and wait for settled delivery/source/geometry before opening.
The original strict recovery tuple, exact currentness and pixel assertions are preserved. The
[fixture authority lesson](../failures/asset-fixture-source-authority.md) records the correction.

Steering diagnosis established `Activation(Publication(ActivityAuthorityUnavailable))` before
image work, also reproduced by a text-only control. Existing runtime-interest test support now
provides exact process-ready Activity enrollment/session publication, retains that owner through
projection close and explicitly joins it. Its two visibility guards admit tests or `test-faults`
only. No production behavior or authority check is weakened.

Run `9a85ef0a-5b39-4173-8dbc-c5e1e3454afa` passes all 25 active-steering cases plus ordinary Exit
and original commands. Final native run `2525b48c-a19c-4cd4-8cee-b557759f4430` passes all three
mounted image workflows in 12.241 seconds: rendition recovery/readback, original commands and
ordinary Exit/reopen. This closes the earlier rendition failure with the original strict predicate.
The combined focused receipts establish 107 distinct passing App cases, not one 107-case run.
Unaffected baseline checks retain their exact source/configuration assumptions.

The normal no-default-features production App library check passes in 18.88 seconds. An auxiliary
unit-test build without `test-faults` fails on 29 unrelated existing fault-support imports and
test-only constructors in untouched modules; that broader test mode is not qualified here.
Owning functional receipts use the established `--features test-faults` verification mode.

# Inputs And Evidence Limits

The serial canonical checkout uses the accepted manifests/lockfile and shared build target.
Comparison covered 4,222 tracked Rust/manifest/test inputs, excluding the two unassigned concurrent
composer edits. Two formatting-only canonical differences were reconciled to accepted source
before testing. New owned test inputs are synchronized explicitly; unrelated work is preserved.

All seven modified/new Rust inputs match canonical after final native execution. Their input
identity is `40B9B098E78F24A4BC811F453AF37B35751AB8519F3B7A54E9346E96ED8059A8`:
SHA-256 of sorted UTF-8 Git paths, one space and uppercase file SHA-256 per line, joined by LF
without a final LF. The set is this acceptance's seven Rust paths over `c77f283c`; documents and
unassigned source changes are excluded. Temporary production diagnostics were removed exactly,
restoring the submitted-user source hash
`EFE37A7430960C653C174BA8F43D09ECE90DA04F24D4D8627A546C94965A58AD`.

Final native raw-log SHA-256 is
`A2B6D692D8DC8619C99080E22B4A0DF8093881478424A1300BB15268B0CC0428`;
the 25-case steering correction cohort's containing log is
`2CF1FDA4A1D97BB023C8D6AC352B13AF68FCAE6A5F52A044E89A0162124204C9`;
the normal production App check log is
`B169C7A6EB371BB26FB8403669D75A3E678B09D4AE5CCF4916494F3E550BD07F`.

This is bounded functional/resource qualification. Existing native two-window sharing and logical
device-loss evidence may be reused; there is no new physical-device-removal, whole-process RSS,
throughput or sustained-stress claim. Independent final review verified source/canonical identity,
the recorded hashes and composed acceptance with no blocking findings. Nineteen exact ledgered
native fixtures and the completed task evidence directory were reclaimed after that review;
the forty-one durable fixture Homes had already been reclaimed. No owned build/test process
remains. Shared build outputs/canonical checkout and unrelated Operator work are preserved.
