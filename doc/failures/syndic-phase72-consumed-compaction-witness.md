# Phase 72 Consumed Compaction Witness Validation

## Scope

Decoded/reopened consumed context-compaction records and late-response reconciliation.

## Invalidated Approach

The first validation pass accepted a consumed witness when its source revision was merely older
than the operation and its successor gate revision was nonzero.

## Evidence

- Generated consumption always records the operation's immediate predecessor revision and advances
  the operation exactly once.
- Reopen validation did not require that equality or prove the witness settlement and successor
  revisions against the actual durable gate, turn lifecycle, binding, and continuation records.
- Late-response reconciliation trusted terminal presence plus the decoded settlement enum after
  reopen, allowing forged predecessor provenance to appear terminal-settled.

## Why It Failed

Construction-time correctness is not reopen authority. Durable records can be corrupted or forged,
so every retained successor witness must be revalidated from the actual stored successor state
before it can authorize recovery or response reconciliation.

## Course Correction

- Require the consumed witness source revision to be the immediate predecessor of the record.
- Retain and cross-authenticate the exact historical gate transition rather than treating any later
  current gate revision as proof of an arbitrary claimed predecessor.
- Validate lifecycle, binding, continuation, and settlement claims against the actual durable
  successor selected by that settlement, including exact home/operation-derived continuation
  identities, thread ownership, and legitimate lifecycle descendants.
- Add corruption cases for noncontiguous source revision, forged lower historical successors,
  equivalent-success settlement swaps, mismatched continuation provenance, and invalid lifecycle
  descendants.

## Completion Review Follow-up

The first correction validated the gate state only while the current gate still equalled the
witness successor. Once accepted work advanced the gate, it accepted every lower claimed revision
without retaining independent historical evidence. That made a forged witness eligible for late
response reconciliation. The same shortcut let a continuation witness point across threads, while
requiring its state to remain `Pending` incorrectly rejected a legitimate later `Active` turn.

## Fresh Completion Review Follow-up

The independent receipt initially remained only self-authenticating: the consumed operation linked
its revisions and settlement but did not commit to the receipt's complete historical gate and
continuation payload. After current-gate progress, one corrupted receipt could therefore substitute
different internally coherent predecessor snapshots. The public late-response read also omitted
the settlement-specific lifecycle, binding, accepted-work, or continuation successor validation
performed during a full store audit. Finally, continuation parent and selected-path topology were
checked for internal agreement but not rederived from the immutable admission snapshot path.

## Affected Authority

- `doc/systems/syndic-conversation-history/design.md`
- `crates/syndic-storage/doc/design.md`
- `doc/plan.md` Phase 72

## Resolution

Every consumed operation now commits by typed SHA-256 digest to the complete canonical V1 settlement
receipt. Reads cross-check that exact receipt and its concrete gate, lifecycle, binding, accepted-work,
and continuation successor before recovery or late-response reconciliation. Continuation identity,
parent, depth, digest, ancestor skip, selected path, and legitimate lifecycle descendants are
rederived from immutable admission authority, with corruption coverage for coherent substitutions.
