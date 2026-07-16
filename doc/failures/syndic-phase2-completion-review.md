# Syndic Phase 2 Completion Review

## Invalidated Approach

The first Phase 2 implementation registered all 28 Syndic V1 families and passed its focused suites, but that was not sufficient evidence of an authoritative schema boundary.

Independent review found that branch context was not closed from thread and owner records back to the exact envelope and source parent; a `Current` transcript head did not prove complete visible current projections; steering and active binding records did not prove same-thread ownership; stale bindings could omit the CAS thread whose provenance they represented; and current drafts stored a replacement target without the separately required selected-path proof.

The first public read surface also returned several ordered index records without getters and offered no coherent current binding-head plus binding read. Its semantic corruption tests covered only a subset of the family matrix, did not validate every seed before corruption, and sometimes created more than one contradiction in a case, so an unrelated rejection could keep a test green.

## Why It Failed

Declaring every family and scanning every keyspace is not equivalent to proving every forward and reverse invariant. Rebuildable projections still need exact `Current` semantics, historical binding records must be distinguished from live authorization, and a test named for one invariant must isolate that invariant before it can serve as completion evidence.

## Course Correction

Phase 2 remains incomplete until the accepted schema is corrected without adding compatibility paths: replacement intent carries its selected-path proof; stale bindings always retain and reserve their CAS thread id; branch context, transcript visibility, CAS correlation, and same-thread execution ownership are validated bidirectionally with bounded traversal; current binding reads prove a stable head/binding pair; and the test-fault matrix isolates every V1 family in registration, verification, and same-home recovery.

The bounded physical corruption seam remains test-only and exact-codec-owned. No raw storage API, extra record family, migration adapter, or weakened validation rule is an acceptable substitute.

## Resolution

Completion review exposed a further contradiction between exact immutable context provenance and package wording that permitted same-id projection revision advance or rebuild without a finalization boundary. The Operator confirmed that finished turns are not rewritten.

The target contract now confines canonical-item and projection revision advance, invalidation, and deterministic rebuild to live, stale, or incomplete work before finalization. A current projection under a proven-terminal turn is finalized immutable history. Branch creation may reference only that state, and reopen continues to resolve the envelope's exact projection revision, range, and selected bytes. No projection-history family, detached-snapshot tolerance, pinning adapter, or post-finalization rewrite was introduced.

The same review distinguished creation-time selected-path proof from reopen authority. Branch admission proves the source was on the parent thread's selected path, but later parent or child replacement may move a mutable thread tail without invalidating the immutable context source or its stable submitted-turn owner. Reopen therefore validates exact immutable record closure, not later mutable tail membership.

The final completion audit found one validator termination defect and three evidence defects. Replacement ancestry traversal could encounter malformed cyclic topology before the general turn-topology pass, so it now requires every traversed parent to decrease depth exactly and has a regression proving bounded rejection. All six malformed context shapes now prove same-home recovery rejection as well as live verification and reopen rejection; every public reverse-index correlation getter is asserted; and the two positive temporal-context fixtures append immutable binding revision two instead of overwriting revision one.

A fresh root-owned architectural review then found two omitted reopen invariants. A second pending, active, or unknown-terminal turn could exist for one origin thread, and replacement-edit intent could target a provider-operation turn. Reopen now requires every execution-blocking turn to be its origin thread's unique committed tail and requires every replacement target to be an ordinary user turn. Both regressions exercise registration, live verification, and same-home recovery. A narrow re-review also rejected the first provider-operation fixture because its changed draft timestamp independently made the history summary stale; the fixture now preserves exact activity agreement so the target-kind rejection is isolated.

Final package verification passes 15 default-feature and 38 all-feature `syndic-storage` nextest cases, locked all-target checks, warnings-denied Clippy and Rustdoc, the public Cargo example, formatting, source-size and dependency-boundary checks, and whitespace validation. Elevated foundation regressions pass 206 default-feature and 231 all-feature cases across `beryl-model`, `beryl-home-store`, `beryl-state`, and `syndic-storage`, including the physical corruption, crash, recovery, concurrency, and Windows filesystem suites.
