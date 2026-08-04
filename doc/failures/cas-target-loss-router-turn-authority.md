# CAS Target-Loss Router Turn Authority

## Invalidated Approach

Build a durable Syndic target-loss request with the CAS turn identity retained by the in-memory
connection router.

## Evidence

The locked `beryl-app` feature suite's
`exact_router_target_mismatch_converges_the_production_target_loss` regression durably published
`steering-turn-187` while deliberately binding the router target to
`wrong-steering-turn-187`. The provider broker copied the router value into
`AcceptedRouteLostTarget::Steering`; Syndic correctly rejected it against the durable active-turn
record and route target, and reconciliation returned `Collision`.

## Why It Failed

The router is revocable process state and may be the component whose identity drift caused target
loss. It cannot redefine the durable CAS turn while publishing that loss. The authoritative turn
is the exact `ActiveCasTurnRecord` correlated with the binding snapshot, gate, and selected route.

## Course Correction

Use router turn identity only when publishing a genuinely absent activation. Before abandonment,
read and recheck the durable binding, snapshot, turn state, gate, and active CAS turn. Derive
`AcceptedRouteLostTarget` from that stable durable frontier, then let the serialized Syndic
mutation and fixed-work reconciliation fence any later race.

The router-mismatch regression remains unchanged and must converge to durable projection loss
rather than weakening storage equality or treating the mismatch as an ordinary collision.
