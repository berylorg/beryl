# Accepted-Delivery Reconciliation Needs Complete Durable Operation Identity

## Scope

`syndic-storage` accepted-input claim and disposition reconciliation in root-plan Phase 53.

## Invalidated Approach

The first invalid approach classified an ambiguously surfaced leaf transition as `Exact` whenever
the input's current leaf had the expected next revision, state, and lifecycle. It treated that
successor shape as sufficient evidence after unrelated gate or route-head advancement.

The second invalid approach strengthened active-abandonment reconciliation with the successor
binding, gate, route, projection-loss target, aggregates, and optional named leaf, but persisted no
operation identity in the projection-loss generation itself. Generic reconciliation read no leaf
and therefore could not distinguish its successor from a named exact-rejection abandonment.

## Decisive Evidence

Two requests can have the same input, source leaf revision, and transition kind while naming
different input-gate revisions. Request A can lose before commit, unrelated admission can advance
the gate, and request B can then perform the same leaf transition under the newer gate. Both
requests predict the same successor leaf.

The leaf-only classifier would therefore report request A as `Exact` even though A could not have
committed. For a delivery claim, that false result can authorize duplicate provider dispatch.

Generic and named abandonment use the same source binding, gate, route, stale provenance, and
successor revisions. A named commit preserves one rejected delivering leaf as projection-lost
next-turn work, while a generic commit terminalizes every delivering leaf as delivery-unknown.
Without a persisted mode and named-input identity, the named successor satisfied the generic
classifier and could be falsely reported as the generic command's commit.

## Accepted Correction

Every delivery request explicitly names its expected selected-route proof as well as gate and leaf
revisions. The atomic transition persists a bounded last-transition proof in the successor leaf,
including the exact source gate, route, leaf revision, and transition kind. Reconciliation compares
that durable proof to the complete request.

The proof remains valid when unrelated inputs later advance the shared gate or route head. A later
transition of the same leaf replaces it with a different successor revision and proof. Initial
admission and non-delivery leaf rewrites carry no delivery-transition proof.

Atomic active abandonment separately persists a bounded proof in the projection-loss generation:
the exact source binding, gate, selected route, and either generic mode or the exact rejected input
and source leaf revision. Reconciliation compares that complete proof before returning `Exact`;
generic and named modes never authenticate each other.

Reopen validation checks each leaf witness against immutable admission order and the strictly later
current gate and generation revisions. It also validates the abandonment proof against its prior
target, binding retirement, route generation, and named leaf successor when present.

## Affected Authority And Evidence

The correction belongs in `crates/syndic-storage/doc/design.md` and root-plan Phase 53. Regression
coverage must include same-leaf and same-kind requests that differ only in gate or route authority,
generic-versus-named abandonment cross-classification, impossible future-authority witnesses,
commit fault cuts, and reopen validation.

No app-side serialization or immediate-current-head check is an acceptable substitute for the
durable operation identity.
