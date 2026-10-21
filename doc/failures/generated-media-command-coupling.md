# Generated-Media Command Coupling

Generated-media implementation assumed HomeStore already supplied checked witness composition for
its mutation participants.
Source inspection invalidated that prerequisite. `HomeCommand::add` and
`add_validation` enforce unique domain participation, but they cannot require a peer participant or
compare its operation, target, source fingerprint and Asset facts. Each mutation/validator reads
only its own domain. Calling both factories from App cannot make that relationship mandatory.

The existing `HomeProofCommand` composes read-only roles on one snapshot; its receipt explicitly
grants no publication authority in the
[domain API contract](../../crates/beryl-home-store/doc/design-domain-api.md#read-only-proof-composition).
The [atomic command contract](../../crates/beryl-home-store/doc/design-atomic-commands.md)
provides ordinary participants and the separate fixed first-acceptance reconciliation successor;
neither supplies generated-media write pairing. Reusing a read-only receipt or the unrelated
successor shape would not enforce the accepted generated preparation/publication contract.

The consequence is a missing integrity boundary: one side could be omitted or paired with
different facts while the ordinary Home command remains admissible. This violates the required
one-Asset/one-Syndic composition, even though any mutations actually submitted share an atomic
batch. No generated-media participant implementation or App publication was added around this gap.

The recommended prerequisite is a separately designed and qualified HomeStore command-participant
witness capability. Contributions must seal their owning domain and operation-qualified target
facts; writer admission must require exactly the permitted matching roles before mutation.
Preparation, publication and unavailable absence validation need their distinct role shapes.
Missing, duplicate, stale and mismatched roles must refuse without writes; cancellation, bounded
descriptor accounting and original indeterminate custody retain their existing semantics. The
read-only proof receipt remains read-only. Operator subsequently authorized this prerequisite;
the target is now owned by HomeStore's
[required command pairs](../../crates/beryl-home-store/doc/design-atomic-commands.md#required-command-pairs)
and the image system's operation-specific correlation contract.

Dependent implementation stopped under the Operator's plan-feasibility instruction until the
prerequisite was authorized. The [active plan](../plan.md) orders it before the retained unqualified
State and Syndic partial work. Authenticated source qualification is unaffected.
