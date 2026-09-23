# Discussion Command Composition

## Invalidated Assumption

Phase 515 assumed independently exposed Syndic gate admission, parent-frontier validation, gate
release and archive contributions could be added to one home command. The history-storage contract
explicitly called the parent observation a separate validation participant.

## Evidence

The [atomic command contract](../../crates/beryl-home-store/doc/design-atomic-commands.md) forbids
duplicate same-domain participation. Both `HomeCommand::add` and `add_validation` in
`crates/beryl-home-store/src/command.rs` enforce it, and the existing
`SyndicStorage::archive_branch_discussion` already produces a Syndic mutation contribution.
Consequently admission plus a separate parent validator, or release plus separate archive,
cannot be composed as proposed. Independent readiness review confirmed the conflict on 2026-09-23.

## Proposed Correction

Preserve the store rule and atomic command. Validate a separately typed parent-frontier proof
inside the single Syndic admission mutation. Expose a single Syndic success mutation that releases
the gate and archives the discussion together; terminal failure releases only the gate. Compose
that one Syndic contribution with the State job transition. Neither sequential commits nor a
generic participant multiplexer is needed.

The owning history-storage and branch-handoff contracts must be reconciled before implementation.
Phase 515 is blocked pending Operator direction under the supplied instruction to stop when a
planned step cannot technically work. No gate admission/release implementation has begun. The
accepted discussion creation and catalog custody work is unaffected.
