# CAS Recovery Decision Post-Retirement Replan

## Scope

Checkpoint 3 Phase 10 target-retirement followed by recovered projection, including explicit
`Recover from Syndic history` authority and automatic recovery after proven lineage loss.

## Invalidated Approach

After an exact target-owned source was retired, the coordinator replanned the target and accepted
whatever binding revision the later plan reported as the basis for fresh recovery. The pattern was
present in both the exact operator recovery command and automatic recovery after authoritative
source loss.

## Evidence And Failure

The source proof and its recovery path name one exact target binding revision. Its authorized
retirement advances that target binding by exactly one revision. A different mutation can race
between retirement publication and replanning. Adopting that later revision would silently widen
the operation so it could recover over authority that neither the source-loss proof nor the
Operator's explicit decision covered.

The process-local same-thread flight prevents another coordinator in the same process from doing
duplicate remote projection work, but it is not durable mutation authority and cannot replace the
binding revision check.

A reusable CAS source can legitimately come from an older binding revision than the target's
current binding head. Submitting the pending target turn can advance the head to an unbound
revision while the older usable binding remains the proven native source. Equating the source
revision with the target basis therefore rejects valid recovery and is not the required guard.

## Course Correction

Before remote retirement, prove that the binding head being mutated can advance. For a
target-owned source, that head is the target basis even when the stale provenance names an older
source binding. For a different parent source, it is the parent source binding and the target basis
must remain unchanged. The exact stale-publication receipt supplies the only admissible advanced
revision. After retirement, replan only to obtain fresh bounded storage facts, then require its
basis to carry the resulting exact target revision. Any exhaustion or different revision rejects
without starting another CAS thread.

Cross-thread recovery does not retire the parent source and continues to use the decision's
original target basis.

## Affected Authority

- `doc/plan.md`, Phase 10.
- `doc/rework/beryl-home/REWORK.md`, Checkpoint 3 projection completion.
- `crates/beryl-app/src/cas_projection/execute.rs`, `execute/decision.rs`, and publication helpers.
- Focused explicit-recovery verification.
