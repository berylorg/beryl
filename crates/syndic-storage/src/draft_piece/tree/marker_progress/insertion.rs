use super::*;

pub(super) fn transition(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    left: DraftPieceActiveMarkerEffectV1,
    right: DraftPieceActiveMarkerEffectV1,
) -> bool {
    if previous.base_frontier() != current.base_frontier()
        || current.next_record_ordinal() < previous.next_record_ordinal()
    {
        return false;
    }
    match left.pending() {
        Pending::Proof { purpose, .. } => {
            if previous.frontier() != current.frontier()
                || previous.next_record_ordinal() != current.next_record_ordinal()
                || left.working_roots() != right.working_roots()
            {
                return false;
            }
            match purpose {
                Purpose::InsertIdentityAbsent => right.pending() == primary(Purpose::InsertAnchor),
                Purpose::InsertAnchor => {
                    right.pending() == primary(Purpose::InsertOrder)
                        || right.pending() == Pending::None && right.insertion_site().is_some()
                }
                Purpose::InsertOrder => {
                    right.pending() == primary(Purpose::InsertAfter)
                        || right.pending() == Pending::None && right.insertion_site().is_some()
                }
                Purpose::InsertAfter => {
                    right.pending() == Pending::None && right.insertion_site().is_some()
                }
                _ => false,
            }
        }
        Pending::InsertSequence => {
            let Some(site) = left.insertion_site() else {
                return false;
            };
            let insertion = match left.effect() {
                DraftPieceMarkerEffectV1::Insert(insertion)
                | DraftPieceMarkerEffectV1::Move { insertion, .. }
                | DraftPieceMarkerEffectV1::SameIdReplacement { insertion, .. } => insertion,
                DraftPieceMarkerEffectV1::Remove { .. } => return false,
            };
            let expected_digest = leaf_digest(
                &DraftPieceLeafValueV1::Marker(insertion.marker()),
                DraftPieceTextSummaryV1::empty(),
            );
            let key = previous.key();
            let expected_id = record_id(
                key.draft_id(),
                key.session_id(),
                key.operation_id(),
                previous.next_record_ordinal(),
                expected_digest,
            );
            matches!(right.pending(), Pending::InsertIdentity { sequence_target, new_leaf_id, new_leaf_digest } if new_leaf_id == expected_id && new_leaf_digest == expected_digest && inserted_sequence(left.working_roots(), sequence_target, site.boundary.inner() != 0))
                && current.next_record_ordinal() > previous.next_record_ordinal()
                && previous.frontier() == current.frontier()
                && left.insertion_site() == right.insertion_site()
                && left.working_roots() == right.working_roots()
        }
        Pending::InsertIdentity {
            sequence_target,
            new_leaf_id,
            new_leaf_digest,
        } => {
            matches!(right.pending(), Pending::InsertOrder { sequence_target: next_sequence, identity_target, new_leaf_id: next_leaf, new_leaf_digest: next_digest } if next_sequence == sequence_target && next_leaf == new_leaf_id && next_digest == new_leaf_digest && left.working_roots().marker_index_summary().record_count().checked_add(1) == Some(identity_target.summary.record_count()))
                && previous.frontier() == current.frontier()
                && left.insertion_site() == right.insertion_site()
                && left.working_roots() == right.working_roots()
        }
        Pending::InsertOrder {
            sequence_target,
            identity_target,
            ..
        } => {
            let Some(site) = left.insertion_site() else {
                return false;
            };
            let DraftPieceBuildFrontierV1::Inserting {
                fragment_ordinal,
                base_end,
                successor_end,
                ..
            } = previous.frontier()
            else {
                return false;
            };
            installed_targets(right.working_roots(), sequence_target, identity_target)
                && left
                    .working_roots()
                    .marker_commitment()
                    .marker_count()
                    .checked_add(1)
                    == Some(right.working_roots().marker_commitment().marker_count())
                && right.phase() == Phase::Publishing
                && right.pending() == Pending::None
                && right.insertion_site().is_none()
                && current.frontier()
                    == DraftPieceBuildFrontierV1::Inserting {
                        fragment_ordinal,
                        next_piece: 1,
                        next_byte: 0,
                        base_end,
                        successor_end,
                    }
        }
        _ => false,
    }
}

fn inserted_sequence(
    roots: DraftPieceBuildRootsV1,
    target: DraftPieceSequenceDescriptorV1,
    split: bool,
) -> bool {
    let source = roots.sequence_summary();
    let target = target.summary;
    source.text_summary() == target.text_summary()
        && source.piece_count().checked_add(1 + u64::from(split)) == Some(target.piece_count())
        && source.marker_count().checked_add(1) == Some(target.marker_count())
}
