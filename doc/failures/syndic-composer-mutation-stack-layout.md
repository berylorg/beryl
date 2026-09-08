# Syndic Composer Mutation Stack Layout

## Scope

Range-backed composer settlement and candidate-session authentication through `syndic-storage`,
HomeStore, and the GPUI owner on Windows.

## Invalidated Approach

Treat the default-development stack overflow as proof that composer durable commands require a new
lifecycle-owned worker solely for stack isolation.

## Evidence

The real GPUI harness aborts without recursion on the default development stack while executing the
bounded synchronous mutation, settlement, session-decode, and history-authentication chain. Debug
frames reach roughly 601 KiB in `execute_active_mutation`, 310 KiB in
`SettleMutation::contribute`, and 255 KiB in session-record decoding.

The exact release harness passes normally and on an explicit 1 MiB diagnostic stack. Its effective
minimum is 576 KiB, leaving about 44 percent reserve headroom. Optimized frames fall to roughly
158 KiB, 57 KiB, and 45 KiB respectively. Compiler layout evidence attributes the inflation to
several bounded by-value records multiplied through nested result/control-flow output slots and
development spills, not recursion, an unbounded draft value, or one giant future.

V5 marker continuation exposed the same development-layout hazard in ordinary range advancement.
Two isolated regression cases passed at the accepted V4 baseline, aborted on the default stack
after a genuine V5 rebuild, and passed with the same binaries on a diagnostic 4 MiB stack.
Checkpoints traced the failing ordinary `Inserting` submission through successful build and
session custody checks to candidate-history authentication. The same authentication completed
when reached from shallower calls. This evidence establishes a development regression; it does
not establish an optimized production stack requirement.

## Why It Failed

The worker conclusion treated development frame layout as a production execution-ownership
requirement before measuring optimized code. Existing data and operation bounds remain valid, and
release codegen does not require a new durable lifecycle boundary for this path.

## Course Correction

Split the active-mutation and settlement functions into focused out-of-line phase helpers and
narrow decoded session and history-transition lifetimes so branch-specific bounded result slots do
not coexist. Re-measure before allocating. If splitting remains insufficient, box only the proven
large authenticated-history, typed session-record, or settlement-closure value boundary; keep each
allocation bounded and short-lived and retain no draft page or whole value.

For candidate-session authentication, isolate head decoding, open-receipt decoding, active-custody
validation and candidate-history validation in out-of-line helpers borrowing the decoded head.
Preserve read order, typed errors and historical early-success behavior. Both isolated default-stack
regressions pass with this split. A controlled reinlining comparison also passes with the original
advance-preparation body, so the provisional preparer extraction was removed. The correction adds
no worker lifecycle or allocation boundary; broader continuation acceptance remains in its plan.

The complete isolated continuation run passed 73 tests on default stacks, including all 17 new
cases (run `bb127107-2b8a-4eeb-aa48-adf43571a806`). Two older durable-builder fixtures also
overflowed on the untouched accepted baseline `4235eb9`: staged marker effects and atomic marker
writer cuts. Their failures occur during nested fixture staging. A test-helper extraction did not
resolve them and was discarded. Both semantic cases passed separately with process-local
`RUST_MIN_STACK=4194304` (run `5b38f4cb-bf26-44a7-86b6-82d3c128631a`); no stack override or fixture
refactor was committed. This is not evidence that those two fixtures pass on default stacks.

## Affected Work

The original correction belonged to Phases 179 and 180 and required the default-development GPUI
harness plus release evidence on an explicit 1 MiB stack. Phase 338 owns the candidate-session
recurrence and requires the default-stack continuation and ordinary-range regressions together
with isolated marker semantics. Its new regression is corrected and its independent acceptance
review passed; the two pre-existing fixture stack limitations remain outside that correction.

## Postpromotion Settlement Recurrence

The final app run `1eedff7c-51f1-4224-ae58-e339c0e63324` passed 125 of 126 cases on ordinary
stacks. `pending_composer_activation::predispatch_pending_flight_loss_settles_custody_and_keeps_promoted_editor_usable`
aborted with `0xc00000fd` while processing the promoted editor's next ordinary text edit. Target
activation, pending-surface priming, the blocked dispatch, publication, late-flight release and all
custody-zero assertions completed first.

Moving setup into `promote_pending_with_blocked_dispatch` preserved the assertions but still
aborted in run `63d2b870-a3ed-4504-aab0-88830a78cbe8`. Do not continue treating this recurrence
as large fixture setup or weaken the subsequent-edit assertion. Selection identity alone is also
insufficient to prove a committed edit after noncommit generation refresh; the fixture now requires
candidate generation and root advancement.

A CDB capture on 2026-09-08 used the same debug test executable with its exact test filter, ordinary
thread stack, no stack override, and a first-chance stack-overflow break. The failing path is:

```text
history::codec::decode_transition
history::retention::authentication::authenticate_draft_edit_history_frontier_v1
checkpoint::candidate_is_exact
publication::validate_publication_receipt_history
mutation::settlement::{read_and_authenticate, prepare}
staged_command::capture::{preparation::settle, CommandMutation::prepare}
HomeStore::execute_current
PreparedStagedDraftPieceCommandV1::submit
SyndicComposerHost::run_build_command
composer_slot::dispatch_quantum
conversation_composer_owner::pump_dispatch
```

The remaining test frame is about `0x1610` bytes (5.5 KiB). Roughly 1.9 MiB is consumed in the
nested production chain before the transition decoder completes stack probing. Independent review
confirms this is a concrete ordinary-debug execution failure, not evidence of recursion or an
unbounded draft, and not proof that optimized release code fails.

Independent frame attribution identifies approximately 327 KiB in settlement
`read_and_authenticate`, 322 KiB in staged capture `preparation::settle`, 275 KiB in frontier
authentication, 170 KiB in `candidate_is_exact`, and 159 KiB in settlement `prepare`.
The narrow correction is to reduce overlapping bounded result/value lifetimes with the already
accepted borrowing and out-of-line mechanics. Preserve read order, identity/proof checks, typed
failures and public command custody. A larger test stack cannot be credited as this correction.

The app's 29-file production scope remained frozen at SHA-256
`8577942819609264D864DDC2A895A4A3E437CDB15427122421B25CF06CDD91FF`
while the Operator reviewed and then authorized the narrow correction below.

## Accepted Settlement Layout Correction

The Operator authorized borrowing first and targeted boxing when measurements justified it.
The initial four-file borrowing correction made the previously failing ordinary-stack witness pass,
but the measured decoder entry plus its prologue still consumed about 1,912,128 bytes of a 2 MiB
thread stack. That left only about 185 KB at that probe. A passing witness alone was insufficient
reason to stop reducing the demonstrated frame pressure.

The final five-file correction borrows build and session values through settlement authentication,
shares the exact writer-generation preparation entry, and separates checkpoint root/frontier
authentication from transition-specific decoding. It boxes the bounded prepared settlement
contribution once at construction and carries `Option<Box<...>>` through preparation and contribution.
Staged capture reuses its existing allocation boundary instead of boxing that option again.
Exact replay remains `None` and avoids the old outer allocation; direct settlement gains one
bounded box. PDB type information gives the contribution payload as 32,432 bytes.

A final owned build clone also duplicates its bounded canonical-header vector. The header contains
fixed identities, references, positions, counts and a digest, not edit payload or a logical
collection. The clone owns durable value data and duplicates no move-only capability. No new
worker, public API, schema, operation identity, read ordering or custody protocol was introduced.

Live debug probes on the same postpromotion witness measured a maximum of 1,607,808 bytes after
the history decoder's prologue and 966,832 bytes after the settlement constructor's prologue.
The measured decoder point leaves approximately 489 KB of the ordinary 2 MiB stack. Construction
still has a bounded 113,664-byte frame; boxing does not establish direct in-place construction or
the absence of a stack temporary. These are observed points on the exercised path, not an
exhaustive runtime stack upper bound.

Final debug frame allocations include 49,680 bytes for settlement authentication, 147,744 for
staged settlement capture, 88,416 for settlement preparation, 744 for its shared generation entry,
113,664 for contribution construction and 19,632 for contribution. The large prepared value no
longer propagates through all caller result slots.

Final ordinary-stack runs passed the strengthened postpromotion witness, all ten pending activation
cases (`bccb3b61-f807-47ef-bdbf-f61ec0184f48`), and all 72 focused storage cases
(`30890891-c500-4ae2-b81f-2db8f2d983dc`). The storage cases cover candidate publication, edit
history and retention, publication evidence, editor checkpoints, and staged outcomes, including
nonzero-generation openings, substituted forks, historical replay, ambiguous cleanup, dynamic
history refusal and more than 256 fragments.

The two existing storage release witnesses passed in ordinary optimized release
(`bc87fe23-8aba-4151-84e7-cda01e965449`) and with debug symbols enabled for frame attribution
(`6ff1c05d-3723-4d0d-beca-214462bddb19`). They exercise editing after immutable publication and
prepared historical-settlement replay through the direct and staged preparation routes.
Optimized probes measured decoder/constructor depths of 470,944/358,256 bytes in staged replay and
536,864/342,048 bytes in postpublication editing. Frame allocations include 24,856 bytes for
settlement authentication, 61,776 for staged settlement capture, 36,056 for the shared preparation
entry with inlined preparation, and 47,720 for contribution construction. Checkpoint transition
helpers use approximately 61 KB; the writer-generation check uses 9,512 bytes. These are measured
optimized paths and frame allocations, not exhaustive runtime bounds.
This evidence does not claim that the optimized GPUI witness was run.

An isolated checkout of `35ebfdf` containing only the five production changes passed
`cargo +stable --config .cargo/local.toml check --locked -p syndic-storage -p beryl-app --lib --no-default-features`.
The documented local dependency configuration and ignored local lockfile were used; the canonical
manifest and lockfile were unchanged. Independent semantic and adversarial review found no blocking
difference in generation checks, ordered reads, replay classification, contribution writes,
reservation inventory or single-owner command custody.

The accepted five-file source fingerprint is SHA-256
`8B941AA3172260437688C4F2A5D403FBCED69E4C0002D9575F380592243D9D03`, computed from sorted
repo-relative `path=SHA256` rows joined by LF without a trailing LF.
App marker-integration acceptance resumes separately with its final full regression and production
checks; canonical widget publication and revision pinning remain a later boundary.
