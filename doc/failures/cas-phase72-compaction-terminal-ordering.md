# Phase 72 Compaction Terminal Ordering

## Scope

Context-compaction terminal, router-handoff, and compact-start response reconciliation in the
mounted CAS projection coordinator.

## Invalidated Approach

The first coordinator draft treated completion of its local terminal result as proof that the
ordered router had already published the target terminal. It also treated every later compact-start
response or unbind failure as ordinary authority loss after the local terminal had won.

## Evidence

- Provider terminal handling durably settles the compaction and completes the local result before
  `permit.finish()` publishes the router terminal. A quiet poll may therefore observe the local
  result while `into_proven_terminal_projection()` still returns `TargetNotTerminal`.
- Pinned compact-start acknowledgement has no enforced order relative to lifecycle notifications.
  The durable compaction record may already be consumed by exact terminal evidence when the
  response path returns.
- The authoritative response matrix distinguishes a late matching acknowledgement, a late
  same-attempt completion-unknown outcome, and a contradictory rejection, proven nondispatch, or
  attempt. They cannot share one generic abandon or ignore branch.
- The first correction still sent late acknowledgement through the live-operation request
  mutation. Once terminal settlement consumed the record and released the gate, that mutation
  necessarily rejected the valid late acknowledgement and the app retired the successful
  connection.

## Why It Failed

Local settlement, ordered router publication, request disposition, and connection authority are
separate frontiers. Collapsing them can retire a successfully compacted target before its router
terminal is published, discard the terminal-chosen binding disposition, or preserve a connection
whose response delivery became unknown.

## Course Correction

- Local terminal completion no longer authorizes target handoff. The coordinator waits for
  `LiveEventPoll::ProvenTerminal` before consuming the router's proven-terminal projection.
- Exact terminal settlement remains first-result-wins for durable lifecycle and binding choice.
- Request disposition reconciliation reads either the live record or its immutable consumed
  terminal successor. A later matching empty acknowledgement reconciles as a no-op. A later same-attempt
  completion-unknown result preserves terminal settlement but retires the unusable foreground
  connection. A later rejection, proven nondispatch, or conflicting attempt is an invariant
  failure.
- Focused tests must cover terminal-before-response and local-terminal-before-router-publication
  orderings independently.

## Affected Authority

- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-app/doc/design.md`
- `doc/plan.md` Phase 72

## Resolution

The mounted coordinator now waits for proven router terminal publication, settles each consumed
operation with a canonical committed receipt, and reconciles late compact-start outcomes only after
revalidating the concrete settlement successor. The Phase 72 app matrix proves both independent
terminal-before-response orderings and all matching, unknown, and contradictory late outcomes.
