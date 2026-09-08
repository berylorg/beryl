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
