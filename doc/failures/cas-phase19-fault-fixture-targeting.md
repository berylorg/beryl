# Scope

Phase 19 persistence-fault coverage for broker-owned active-turn identity and terminal source
publication.

# Invalid Approach

The old ordinary-turn fault fixtures used two indirect strategies:

- block one unscoped writer cut and fail the next visit, assuming the next mutation was the active
  CAS-turn identity command;
- block terminal source publication after persistence and fail the next unscoped writer visit,
  assuming ordinary terminal convergence always required a second commit.

# Evidence

Repeated runs of `phase13_ordinary_turn_faults` produced two non-deterministic outcomes. The
identity test sometimes cut a different broker mutation and persisted `CompletionMismatch` instead
of the intended identity-failure outcome. The terminal test sometimes returned a successful exact
terminal because broker publication had already left derived history converged and the assumed
second commit was a legitimate no-op.

# Why It Failed

Phase 19 makes the ordered broker the live source authority. Multiple typed mutations can occur
between turn admission and request completion, so an unscoped "next writer" is not an identity.
Likewise, a post-terminal derived commit is conditional work and cannot serve as the durable
terminal source boundary.

# Course Correction

The fixtures now fault the exact typed authority they claim to test:

- active identity uses `active_cas_turn_fault_scope()`;
- provider and terminal source operations use `live_source_event_fault_scope()`;
- terminal cuts assert the two legal whole states directly: source-less loss for a prior write, or
  source-backed terminal for a durable write.

Six consecutive full fault-matrix runs passed after the correction. Future persistence tests must
target a typed mutation scope and must not rely on a conditional follow-up commit as a proxy for a
source operation.

An active-only loss fixture must also distinguish the pre-loss frontier from the converged history.
The broker first publishes the missing exact `TurnActivated`, then appends the source-less terminal;
"source-less loss" describes the terminal's absent external source, not a terminal-only history.

# Authority And Follow-Up

This correction does not change the target design in
`doc/systems/cas-live-syndic-transcript/design.md` or the Phase 19 acceptance boundary. Phase 20 may
delete the dormant materialized path, but it must preserve the typed broker fault boundaries.

## Phase 79 Recurrence

The first checkout-to-install publication fixture registered one process-global "next quarantine
checkout" observer. A parallel conversion could steal that observer and block on another test's
resume channel, reproducing the same unscoped-next-operation error at an in-memory authority
boundary.

The corrected fixture keys observers by exact persistent-failure cut identity. Conversion removes
only its matching observer and releases the observer mutex before notifying or waiting. Future
concurrency fixtures must scope even test-only synchronization to the identity whose ordering they
claim to prove.
