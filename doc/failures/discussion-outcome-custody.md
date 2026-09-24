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

## Pre-Command Nondispatch Disposal

Independent ordinary-dispatch integration review on 2026-09-24 invalidated treating a reserved
slot plus caller-owned rejection proof as sufficient shutdown custody. In
`cas_projection/ordinary/execute/handoff.rs::settle`, cancellation or a fenced/non-conflict
preparation failure drops `ReservedDiscussionNondispatch` before the atomic cancellation and
retryable failure commit. The same gap remains after a healthy writer conflict.

`LiveEventTarget::drop` retains or releases projection ownership but never receives the exact
nondispatch proof. The durable parent can remain activated. Later
`discussion_settlement/prepare/execution.rs` maps a starting parent's incomplete outcome to
`UnrecoverablePostAppend`, losing retry eligibility despite exact CAS rejection. This violates
the [handoff rejection contract](../systems/branch-discussion-handoff/design.md#parent-input-and-turn)
and the app's promised disposal custody. Submitted-command indeterminate audit retention does
not cover this earlier state.

Operator authorized the correction on 2026-09-24: retain the exact pre-command proof within its existing
bounded settlement slot, with an explicit disposal and recovery lifecycle. Fresh candidate
authority must settle that proof before generic incomplete convergence consumes the activation.
Do not label an unsubmitted command indeterminate, fabricate registry evidence, introduce an
unbounded queue, or relax known-rejection retry semantics. The owning system and app contracts now
define this boundary. The retained-proof component and exact-old/new candidate recovery passed
fourteen distinct focused cases, dependent checks and independent review. Complete graph mounting
must run the prefix before CAS-live recovery and reject successful shutdown while proofs remain.
