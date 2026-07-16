# CAS Phase 13 After-Commit Durability Classification

## Scope

Checkpoint 3 Phase 13 ordinary-execution publication and terminal-convergence fault tests.

## Invalidated Assumption

The first app-level fault matrix treated `AfterCommitBeforePersist` as though same-home health
verification must always retain the newly committed mutation.

## Why It Failed

That fault occurs after Fjall accepted the commit but before the required persistence barrier
completed. The surfaced outcome is therefore physically ambiguous. Depending on what reached
durable storage and what same-home verification can validate, recovery may expose either the
complete prior state or the complete new state.

A terminal-convergence test observed the complete prior frontier after this cut. Requiring the new
frontier would contradict the Beryl-home durability contract and would encourage treating commit
acceptance as persistence proof.

The first terminal fault fixture also used visibility of the durable `TurnActivated` record as
proof that its writer had already crossed the post-persist fault hook. Fjall publishes the batch to
its in-memory snapshot before that outer hook returns, so the next blocker could still attach to
the tail of activation and move the intended cut one command earlier.

## Course Correction

Phase 13 retains the cut and accepts exactly the two authoritative outcomes: the coherent complete
prior snapshot or the coherent complete new mutation. Tests reject every mixed binding, gate,
source-event, canonical, projection, and frontier state. `BeforeCommit` remains prior-only and
`AfterPersist` remains new-only.

The terminal fixture now walks the three exact pre-terminal post-persist barriers before queuing
the terminal-event blocker and convergence fault. Record visibility is not used as writer-drain
authority; the corrected `AfterPersist` convergence proof remains new-only under repeated stress.

No retry, forced publication, fixture rewrite, or alternate storage path is introduced.

## Affected Authority

- `doc/systems/beryl-home-storage/design.md`.
- `crates/beryl-home-store/doc/design.md`.
- `doc/plan.md`, Phase 13.
- Phase 13 ordinary-execution persistence-cut tests.
