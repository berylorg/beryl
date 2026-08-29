# Defensive Runtime Proof Overreached Its Consequences

## Scope

Live Beryl-home publication checks, HomeStore mutation participants, draft-marker admission design,
and the remaining Beryl-home rework plan.

## Invalidated Approach

Persistence, verification, and overlapping rigor modifiers were treated as reasons to add runtime
proof around whole operations even when trusted dependency guarantees or a narrower affected-slice
check already covered the concrete failure.

That assumption produced a global writer lock around read-only proof composition, a global
mutation-health canary after coherent ordinary reads, repeated validation and contribution reads
over one immutable writer snapshot, retained immutable draft-marker paths and transitions for an
entire operation, and repeated adversarial or exhaustive completion gates in the durable plan.

## Evidence

Fjall already supplies coherent cross-keyspace snapshots and atomic batch publication with the
selected durability mode. A proof snapshot can race with a later mutation and become stale, but it
cannot authorize publication by itself. A coherent read can remain valid for its affected slice
even if unrelated future mutation availability has failed. Mutation preparation and contribution
observe the same serialized immutable snapshot, so repeating identical reads does not close a new
race. Draft-marker source-order and target-id indexes need bounded current authority and replay
identity, not every superseded path retained until terminal compaction.

The updated engineering-rigor contract also makes explicit that `verify` means objective evidence,
not necessarily per-operation runtime validation; dependency guarantees count as evidence; the
persistent-state modifier protects affected valid state; and adversarial review is reserved for a
named high-consequence boundary that remains weakly verifiable.

## Why It Failed

The design described mechanisms before identifying the exact supported-envelope fault and
consequence they prevented. Overlapping integrity, persistence, and verification language was then
interpreted cumulatively, so each layer re-proved facts already owned by another layer. This added
serialization, repeated storage work, retained durable state, review work, and plan surface without
improving correctness for a concrete uncovered failure.

## Course Correction

- Credit exact Fjall snapshot, atomicity, and durability guarantees at the HomeStore boundary.
- Keep read-only proof composition off the writer lane and let later revision checks classify a
  raced proof as stale or conflicting.
- Publish ordinary coherent reads from affected-slice, generation, and actual access evidence rather
  than an unrelated global mutation-health canary.
- Let one bounded mutation preparation pass return package-owned validated state for contribution;
  retain validation-only cross-domain participants where they protect a distinct atomic boundary.
- Retain only current draft-marker index authority and the minimum replay receipt, reclaim
  superseded operation-local nodes incrementally, and enforce explicit per-operation plus home-wide
  cross-restart retained-resource bounds.
- Require independent semantic review for production work, adding adversarial review only when a
  concrete high-consequence boundary is still weakly verifiable.
- Keep later rework detail in the tracker until it enters the bounded execution window; do not add
  duplicate checkpoint gates, blanket reconciliation, arbitrary-scale claims, or close-every-
  finding rules.

Exact acknowledgement-loss reconciliation, reservation-versus-actual enforcement, cross-domain
Asset proof, Syndic/CAS identity fencing, external-effect custody, and bounded GUI/data streaming
remain required because each protects a distinct supported-envelope consequence.

## Affected Authority

- `doc/systems/beryl-home-storage/design.md`
- `doc/systems/syndic-conversation-history/design.md`
- `crates/beryl-home-store/doc/design.md`
- `crates/syndic-storage/doc/design.md`
- `crates/beryl-state/doc/design.md`
- `crates/beryl-app/doc/design.md`
- `doc/rework/beryl-home/REWORK.md`
- Root `doc/plan.md` Phases 196 through 202
