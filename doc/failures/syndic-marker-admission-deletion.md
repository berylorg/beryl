# Marker Admission Deletion

## Invalidated Publishing Assumption

Marker-continuation readiness proposed a separate Publishing command that consumes the assigned
admission target after the sequence, identity and order tree changes finish. Its resource proof
assumed admission deletion needs one authenticated path and one replacement internal node per level.

Independent source review invalidated that assumption. In
`crates/syndic-storage/src/draft_piece/admission/index/tree_edit.rs`, `rewrite_tree` removes the
selected leaf and passes a remaining one-child vector to `make_internal_level`.
`DraftMarkerAdmissionNodeV1::internal` in `admission/tree.rs` accepts that vector, but
`authenticate_path` rejects internal nodes with fewer than two children when they are nonroot.
A valid height-three admission tree with a binary internal child can therefore publish an
unreadable retained branch after consuming one target. This is a source counterexample; no new
runtime reproduction was executed during this readiness review.

Canonical deletion requires a defined normalization algorithm and accounting for its sibling
reads, replacement nodes, deleted records, target-absence probes and replay closure. Consequently,
the proposed Publishing totals of 3,580,864 encoded bytes and 3,655,920 peak reserved bytes are
unverified. Separating Publishing from marker insertion alone does not establish that bound.
The same review confirmed that a marker leaf plus a new two-child sequence root costs 671 bytes,
not the proposed 500. The corrected insertion estimate also remains provisional until complete
review. The proposed staged-outcome closure increase from 124 to 128 reads was not verified.

## Required Readiness Boundary

Stop relying on the one-path deletion inventory. Establish canonical admission-tree deletion and
its complete command bound before marker publication can use it. Preserve the existing atomic
target/head/capacity transition, exact replay, cancellation and writer custody. Recheck whether
the Publishing command fits its existing limits once normalization is specified; do not raise
limits or retain invalid nonroot nodes to fit the proposal.

The Operator's technical-plan rule stopped implementation. Incomplete V5 authority edits were
restored; the accepted V4 sequence contract and source remain intact. No marker-continuation
source or tests were changed. The pending boundary is recorded in [the plan](../plan.md).
The Operator subsequently approved admission normalization as a prerequisite, followed by renewed
marker readiness. Independent readiness accepted the canonical algorithm and complete accounting
in the owning [draft contract](../../crates/syndic-storage/doc/design-draft-storage.md#canonical-admission-deletion)
and [schema bounds](../../crates/syndic-storage/doc/design-schema-v7.md#canonical-admission-deletion-bounds).
The 65,536-association profile constrains valid admission height to 18; superseded siblings fit
the existing receipt closure. Complete assignment has a conservative 3,969,402-byte reservation
peak, including public preparation and postcommit readiness. Whole-command reconciliation retains
its captured authority; compact receipt bytes do not recreate deleted cleanup keys. Canonical
deletion and complete assignment passed independent implementation review and isolated runtime
acceptance, including height 18 and the 65,410-association weighted shape. The earlier
marker-continuation estimates remain provisional. Builder composition must still reuse the sealed
result under captured revision and mutable head/capacity fences with one complete quantum ledger.

## Retained Receipt Cleanup

Completion review rejected the first cleanup check, which authenticated retained records and
assignment path membership without proving the complete recorded transition. An ingestion receipt
could contain a valid same-owner descriptor for a still-reachable target subtree, and membership
checks alone did not bind substituted before-roots to the selected after-roots. Cleaning that list
could remove a node the current tree still referenced.

The correction parses the existing receipt metadata and streams canonical transition summaries
from the charged retained records and mandatory current-root cache. Both complete after-root
descriptors and the exact ordered predecessor list must agree before cleanup. This adds integrity
hashing without fresh storage acquisition, put construction or a durable format change. Regression
fixtures must prove the substituted node is reachable and outside the next assignment's path;
adding an unreachable record does not reproduce the failure.
