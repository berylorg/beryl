# Persistent Sequence Split Heights

## Invalidated Assumption

The staged-build outcome implementation assumed that the existing persistent piece-tree builder
could advance a valid operation through more than 256 canonical fragments. Its new public-boundary
test stages 257 one-byte replacements over a 257-byte text root and crosses two durable windows.
Construction fails at fragment 129, when the working sequence first reaches height two.

## Evidence And Cause

The test is `staged_build_outcomes::outcomes::more_than_256_fragments_continue_across_windows_with_bounded_outcome_work`.
Run it through locked local Cargo nextest with package `syndic-storage`, feature `test-faults`, and
target `staged_build_outcomes`. The observed error is `Build(Absent)` during construction advance,
before the final text/root/history assertions can run.

In `crates/syndic-storage/src/draft_piece/persistent.rs`, `make_sequence_tree` collapses a singleton
child and returns a lower-height `SequenceRef`. Recursive `split_sequence` then retains only
`child.link` from that result and reconstructs the parent using the original `tree.height`.
The returned child height is lost. A subsequent split treats a leaf identity as a height-one node
and attempts to read it from `DraftPieceNodesFamily`, where it is absent.

The source worker traced the first failing construction frontier and the root independently
inspected both functions. The new command wrapper still contributes the underlying mutation's
emitted leaves and nodes. This is a pre-existing tree invariant defect, not a consumed-fragment
scan or a missing outcome-finalization effect. Temporary diagnostic probes were removed; the
persistent-tree source has no behavioral changes from this investigation.

## Proposed Correction And Status

Preserve or rebalance recursive split results using their actual subtree heights while retaining
the existing canonical tree and bounded-construction contract. Establish repair readiness, cover
height transitions around the 128-child boundary, and rerun the unchanged long-operation test.
Do not increase traversal limits, flatten the entire draft, or weaken the operation requirement.

Implementation stopped under the Operator's technical-plan rule. The Operator subsequently
authorized the bounded repair and continued outcome/app completion. Readiness then established
that whole-fragment surgery cannot meet the full declared
[range-repair budget](syndic-draft-piece-range-budget.md). The proposed resumable repair and blocked
integration remain in [the implementation plan](../plan.md). The staged outcome source and tests
are preserved but unaccepted.
