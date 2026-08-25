# Syndic Composer Mutation Stack Layout

## Scope

Range-backed composer `MutationCommit` settlement through `syndic-storage`, HomeStore, and the
Phase 180 GPUI owner on Windows.

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

## Affected Work

Root plan Phase 179 owns the local stack-layout correction and must pass the default-development
GPUI harness plus the release harness on an explicit 1 MiB stack. Phase 180 resumes the preserved
composer mount only after that correction passes focused durable-outcome and bounded-memory review.
