# Syndic Phase 66 Stop Transition Witness

## Scope

Durable exact reconciliation for monotonic stop-cause joins and the one dispatch claim.

## Invalidated Approach

The first V1 shape used only the stop record's current revision, aggregate cause bitset, optional
attempt nonce, and live-or-consumed state to authenticate every earlier cause-join or
dispatch-claim successor.

## Decisive Evidence

Two histories could produce the same retained live record. A cause could be present from admission
while another cause advanced the record, or it could be the cause that advanced the record.
Likewise, a claim could precede a later cause join or follow an earlier one while retaining the same
attempt, aggregate causes, and final revision. Aggregate-only fields could not distinguish those
histories.

This contradicts the exact retained-successor requirements in
`crates/syndic-storage/doc/design.md` and Phase 66 of `doc/plan.md`. Treating any later matching
revision as exact is unsound; restricting exactness to the immediate successor conservatively loses
the accepted compatible-descendant guarantee.

## Accepted Correction

The clean V1 replacement persists four cause-first-revision slots plus one optional dispatch-claim
source revision and attempt. Admission occupies revision one; every later revision is accounted for
exactly once by a new cause, the claim, or consumption. Exact reconciliation reads those immutable
witnesses across later compatible descendants. The aggregate-only shape was replaced directly
without a predecessor decoder, migration, or compatibility record.

## Resolution Evidence

`crates/syndic-storage/src/record/stop.rs`, `codec/primary/stop.rs`, and
`read/stop/reconciliation.rs` implement the canonical bounded ledger, direct V1 codec, and exact
transition authentication. Phase 66 tests cover admission sets, join/claim orderings, invalid
ledgers, consumed provenance, recovery exposure, and aggregate-predecessor rejection. The full
package suite passed 422 tests, all 13 doctests passed, and independent completion review reported
no findings.

## Unresolved Risk

None within Phase 66. Backend interruption and app coordination remain deliberately unmounted
future phases and must consume the accepted fixed-provenance boundary without reintroducing
aggregate inference.
