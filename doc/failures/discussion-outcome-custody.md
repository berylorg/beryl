# Discussion Outcome Custody

## Invalidated Assumption

A caller-owned audit and its operation flight do not by themselves keep an uncertain handoff
unavailable. Dropping the last caller copy releases the flight even though the home registry still
retains the unresolved command. A repeated tool request can then see the complete job records and
incorrectly acknowledge them before registry reconciliation.

## Evidence And Correction

Independent review of production resolution admission identified this path in
`discussion_settlement/admission.rs` and `flight.rs`. The regression
`uncertain_admission_retains_job_custody_and_blocks_duplicate_acknowledgment` drops the returned
audit after an injected post-commit persistence failure and checks that repeated delivery remains
unavailable.

The process operation owner retains uncertain audits in the already configured reconciliation
slots. Each retained audit continues to occupy its exact job flight, and exact-job lookup recovers
it after caller disposal. A weak back-reference prevents a custody cycle. Only agreement between
natural State/Syndic evidence and the home registry releases the retained copy; collision remains
unavailable. This implements the existing
[app custody contract](../../crates/beryl-app/doc/design-feature-adapters.md#discussion-resolution-admission)
without a durable backlog map or another reconciliation registry. Coordinator factory mounting
must preserve this process owner across home-service replacement.
