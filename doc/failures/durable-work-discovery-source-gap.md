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
  `crates/syndic-storage/src/read/delivery_recovery/pages.rs` previously scanned `InputGatesFamily`
  across the complete thread-key range, then filtered rows. The app's recovered-pending scheduler
  and startup recovery used those readers.
- Before correction, the [sole V7 schema authority](../../crates/syndic-storage/doc/design-schema-v7.md#v7-domain-schema)
  closed the complete inventory at 91 families. Its accepted-ready and accepted-next sources name
  accepted-route generations; they do not represent a route-free directly submitted pending turn.
- `thread-executions` records immutable runtime/root binding rather than work membership. No
  declared compact source can replace the pending reader merely by implementing a missing codec.

Independent readiness review confirmed the missing persisted-source prerequisite. The app/system
inventory contract is otherwise sufficient; composite revisions, merge mechanics and locking need
no additional architectural prescription if they satisfy its observable guarantees.

## Accepted Course Correction

The Operator approved one shared non-idle gate source for pending-turn and startup discovery.
Independent architecture review accepted the owning
[storage contract](../../crates/syndic-storage/doc/design-history-storage.md#non-idle-gate-discovery)
and [canonical schema](../../crates/syndic-storage/doc/design-schema-v7.md#non-idle-gate-source-canonical-encoding).
Those documents now control membership, atomic maintenance, revision-bound discovery, validation,
encoding and the revised closed inventory; this record does not duplicate their authority.

Source maintenance and discovery replacement are independently accepted; complete inventory remains
subsequent work. Do not omit pre-session pending work, repurpose accepted-route records, or treat
broad scans as an approved exception. Accepted managed preparation did not establish acceptance
of that pre-existing discovery path.

## Reconciliation Closure

Paired mutation writes and reserved rollback effects alone do not complete a new index boundary.
Independent review found that `first_acceptance_status` and other natural reconciliation readers
could still classify exact success from an observation that omitted the new source. A removed
same-command index would therefore leave a false exact result.

Stabilize the current gate/source pair around the affected status read, classify scoped changes
before stable disagreement, and validate the source against the current gate. Historical receipt
gates remain evidence of their own transition and must not select today's source revision.
Direct acceptance, delivery and late compaction reconciliation now have focused corruption cases;
their verification passed with the source-maintenance acceptance boundary.

## Verification

Production storage/app compilation and 175 selected lifecycle, reconciliation, schema, corruption
and footprint checks passed with the configured LLVM and one-build-job settings. Independent
persistence review accepted the current-source correction and preserved registration order.
Peak guarded job memory was 1.45 GiB; owned test processes exited and temporary directories were
removed. Discovery replacement and full process inventory were not claimed by those checks.

## Discovery Replacement

Both recovery readers now consume compact source pages with home/generation/revision-bound
cursors and bounded exact gate resolution. Startup rebases its forward bookmark explicitly;
live scheduling retains fresh-scan wakes. Idle threads contribute no scanned bytes, and a malformed
unselected input-gate row does not affect routine discovery.

Independent semantic review, storage/app production compilation and 173 selected storage/app
checks passed. Coverage includes exact count/byte limits, foreign and old-generation cursors,
stale revisions, corrupt anchors, startup stop/compaction convergence, shutdown and dispatch of
the same retained pending turn after a fresh execution-ready wake. Peak guarded job memory was
1.65 GiB; all owned children exited and test temporary directories were removed. This evidence
accepts source discovery, not complete process work inventory.
