# Phase 72 Compaction Terminal Authority Conflict

## Scope

Context-compaction provider-operation terminal publication, settlement, and durable-domain
validation.

## Invalidated Approach

The compaction mount retained ordered CAS status, turn, marker, and terminal evidence exclusively
in the dedicated compaction-operation record while reusing the global turn-state validator without
giving that validator a provider-operation authority model.

This allowed the mutation path to complete a source-free provider-operation turn but made the same
durable state invalid on full domain validation.

## Evidence

- Compaction admission intentionally creates a source-free provider-operation turn state.
- Exact provider terminal status and the turn-state revision are retained by the matching durable
  compaction-operation record.
- A successful terminal changes the provider-operation turn state to `Complete` without adding an
  ordinary source-event row.
- `validation/projections/events.rs` rejects every complete turn whose source-event count is zero
  with `successful turn completion lacks exact source authority`.
- The dedicated manual and lifecycle settlement fixture completes its mutations and then fails
  `validate_registered_domains` at that invariant.

## Why It Failed

The generic invariant assumes ordinary source-event history is the only possible exact successful
terminal authority. Context compaction introduced a distinct provider-operation observation ledger
without reconciling that global assumption.

Adding an unvalidated exception would hide corruption, while copying terminal evidence into a
second family would create dual authority unless target documentation explicitly chose and
cross-validated that model.

## Course Correction

The accepted target keeps one authority model:

- the exact matching compaction-operation terminal witness is canonical provider-operation source
  authority;
- a successful compaction turn retains zero ordinary source events;
- validation requires exact agreement among the record target, terminal status, recorded turn-state
  revision, and complete turn state; and
- missing, duplicate, or disagreeing authority remains corruption.

No duplicate terminal source event, local validation bypass, or compatibility path is authorized.

## Resolution

The Operator accepted this single-authority correction. Target system, package, tracker, and plan
authority were reconciled before implementation resumed. The dedicated Phase 72 fixture owns proof
of the valid exact case and the missing, duplicate, and mismatch corruption cases.

## Affected Authority

- `doc/systems/syndic-conversation-history/design.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/syndic-storage/doc/design.md`
- `doc/rework/beryl-home/REWORK.md`
- `doc/plan.md`
