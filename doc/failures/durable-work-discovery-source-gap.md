# Durable Work Discovery Needs A Declared Compact Source

## Scope

Architecture readiness for process work inventory after managed runtime/session preparation.

## Invalidated Approach

Compose complete inventory from existing live sessions, attention and durable work readers,
assuming the pending-turn reader already supplies authority-compliant discovery.

## Decisive Evidence

- [Storage authority](../../crates/syndic-storage/doc/design.md#failure-recovery-and-privacy)
  requires compact recovery source families and explicitly prohibits broad input-gate/history
  sweeps; bounded cursor pages alone do not satisfy that restriction.
- `SyndicStorage::recovered_pending_page` and `delivery_recovery_startup_page` in
  `crates/syndic-storage/src/read/delivery_recovery/pages.rs` scan `InputGatesFamily` across the
  complete thread-key range, then filter rows. The app's recovered-pending scheduler and startup
  recovery call these readers.
- The [sole V7 schema authority](../../crates/syndic-storage/doc/design-schema-v7.md#v7-domain-schema)
  closes the complete inventory at 91 families. Its accepted-ready and accepted-next sources name
  accepted-route generations; they do not represent a route-free directly submitted pending turn.
- `thread-executions` records immutable runtime/root binding rather than work membership. No
  declared compact source can replace the pending reader merely by implementing a missing codec.

Independent readiness review confirmed the missing persisted-source prerequisite. The app/system
inventory contract is otherwise sufficient; composite revisions, merge mechanics and locking need
no additional architectural prescription if they satisfy its observable guarantees.

## Required Resolution

The Operator must resolve the owning storage contract before implementation. Membership, atomic
maintenance, revision-bound discovery and validation belong to storage authority; the exact new
family, natural key, value encoding, record version and revised inventory belong exclusively to
`design-schema-v7.md`.

Recommended proposal for review: one compact source per non-idle input gate, keyed by exact thread
and carrying the selected gate revision. Every gate transition atomically inserts, updates or
removes its source. Source pages discover candidates; bounded exact gate/turn/binding reads retain
existing eligibility and recovery checks. The source grants no execution capability and contains
no content or request payload. Existing accepted-route sources continue to own queued work whose
gate is idle. This shared non-idle source would replace both pending-turn and startup gate sweeps.
This paragraph is a proposal, not accepted schema or implementation authority.

After authority is resolved, implement and independently accept the compact source and its atomic
maintenance before replacing discovery readers and composing complete inventory. Do not omit
pre-session pending work, repurpose accepted-route records, add an undeclared family, or treat the
current broad scan as an approved exception. Phase 325 remains pending; phase 355's accepted managed
preparation does not establish acceptance of this pre-existing discovery path.
