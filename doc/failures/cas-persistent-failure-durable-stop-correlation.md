# Persistent-Failure Interruption Requires Command-Bound No-Commit Provenance

## Scope

Checkpoint 3 exact soft interruption during a persistent Beryl-home failure.

## Invalidated Approach

Two related shortcuts were invalid:

- Reuse durable `StopOperationCorrelation`, `StopAttemptCorrelation`, or durable interruption
  authorization for a volatile request that owns no durable stop operation.
- Infer `FailedBeforeWriter` later from broad persistent-home failure plus `NoLocalStop`,
  `AdmittedNotClaimed`, or `ClaimedNotDispatched` process-local state.

## Why It Failed

The durable correlation family explicitly claims an admitted stop operation and its sole claimed
attempt. A volatile request owns neither fact.

The later local-state inference was also unsound. A missing local stop does not prove that one exact
admission failed, while admitted or claimed state proves durable authority may already exist. The
old path nevertheless converted those states into backend `FailedBeforeWriter`, permitting an
unrelated target or an existing durable stop to obtain a second interruption request.

The decisive source evidence was the gap between `StopCoordinator::execute_admission`, which
discarded the store result, and the persistent-failure driver, which constructed
`FailedBeforeWriter` without receiving command-bound evidence. Focused completion review exposed
the mismatch even though backend transport tests and broad local-state tests passed independently.

## Required Course Correction

- Keep durable and volatile backend authorization families nominally separate.
- Create app-side `FailedBeforeWriter` provenance only while the original exact stop election is
  held and the same command-gate permit transfers specifically to persistent failure before any
  home-store writer call.
- Make that transfer non-cloneable, exact-target and same-driver bound, single-use, and a replacement
  for ordinary stop-election ownership. It must not leave a second election record that can block
  exact target close.
- Cancel the exact turn's process-local continuation before preserving or consuming the transfer.
- Let absent proof, durable stop state, generic store failure, ordinary shutdown, local failure,
  target drift, or driver drift authorize no volatile request.
- Defer `WriterReturnedNotCommitted` until Phase 100 preserves the exact typed home-store outcome;
  generic `CommandError` or a later reread cannot reconstruct it.

## Affected Authority

- `doc/plan.md` Phase 98 and Phase 100
- `doc/systems/backend-runtime/design.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-app/doc/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/beryl-app/src/cas_projection/connection/router/stop.rs`
- `crates/beryl-app/src/cas_projection/connection/router/persistent_failure.rs`
- `crates/beryl-app/src/cas_projection/stop.rs`
