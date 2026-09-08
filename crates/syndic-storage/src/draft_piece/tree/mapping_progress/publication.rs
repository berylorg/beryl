use super::*;

fn exhausted(fragment: &DraftPieceBuildFragmentV1, frontier: Frontier) -> bool {
    matches!(frontier, Frontier::Inserting { next_piece, next_byte: 0, .. } if next_piece == fragment.replacement().inserted().len() as u64)
}

pub(super) fn transition(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    left: DraftPieceBuildMappingV1,
    right: DraftPieceBuildMappingV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> bool {
    if left.current_map != right.current_map {
        return false;
    }
    let Some(fragment) = fragment else {
        return false;
    };
    match (left.mapping_stage, right.mapping_stage) {
        (Stage::Idle, Stage::Idle) => false,
        (Stage::Idle, Stage::RefreshMap) => {
            if left.fragment_source_end_unit != right.fragment_source_end_unit {
                return false;
            }
            if control_frozen(previous, current) {
                return exhausted(fragment, previous.frontier());
            }
            matches!(
                fragment.replacement().marker_effect(),
                Some(DraftPieceMarkerEffectV1::Remove { .. })
            ) && super::super::sequence_progress::transition_is_exact(previous, current)
                && super::super::marker_progress::transition_is_exact(
                    previous,
                    current,
                    Some(fragment),
                )
        }
        (Stage::RefreshMap, Stage::RefreshSequence { .. })
        | (Stage::RefreshSequence { .. }, Stage::PublishReady { .. }) => {
            left.fragment_source_end_unit == right.fragment_source_end_unit
                && control_frozen(previous, current)
                && exhausted(fragment, previous.frontier())
        }
        (
            Stage::PublishReady {
                successor_boundary,
                logical_offset,
            },
            Stage::Idle,
        ) => {
            let Frontier::Inserting { base_end, .. } = previous.frontier() else {
                return false;
            };
            let before = previous.marker_effect_continuation();
            let after = current.marker_effect_continuation();
            let scan = before.scan();
            let next = after.scan();
            let endpoint = canonical_fragment_endpoint(fragment);
            let source_logical = if fragment.replacement().is_continuation() {
                before.source_logical_frontier()
            } else {
                fragment.replacement().end().utf8_offset()
            };
            if !exhausted(fragment, previous.frontier())
                || after.active().is_some()
                || right.completed_source_unit != left.fragment_source_end_unit.unwrap_or(u128::MAX)
                || right.fragment_source_end_unit.is_some()
                || previous.next_record_ordinal() != current.next_record_ordinal()
                || current.base_frontier() != base_end
                || current.successor_frontier() != successor_boundary
                || current.working_roots() != actual_roots(previous)
                || after.source_logical_frontier() != source_logical
                || after.successor_logical_frontier() != logical_offset
                || scan.next_fragment_ordinal() != fragment.key().ordinal()
                || next.scanned_endpoint() != Some(endpoint)
                || next.next_fragment_ordinal()
                    != fragment.key().ordinal().checked_add(1).unwrap_or(0)
            {
                return false;
            }
            let frontier = match current.frontier() {
                Frontier::Planning { fragment_ordinal } => {
                    fragment.key().ordinal().checked_add(1) == Some(fragment_ordinal)
                        && current
                            .fragment_endpoint()
                            .is_some_and(|end| end.key().ordinal() >= fragment_ordinal)
                }
                Frontier::CrossValidating => current
                    .fragment_endpoint()
                    .is_some_and(|end| end.key() == fragment.key()),
                _ => false,
            };
            if !frontier {
                return false;
            }
            match before.active() {
                Some(active) => {
                    active.phase() == Phase::Publishing
                        && active.fragment_key() == fragment.key()
                        && active.fragment_digest() == endpoint.digest()
                        && fragment.replacement().marker_effect() == Some(active.effect())
                        && scan.completed_effect_count().checked_add(1)
                            == Some(next.completed_effect_count())
                        && next.effect_chain()
                            == draft_piece_marker_effect_chain_link_v1(
                                scan.effect_chain(),
                                fragment.key(),
                                endpoint.digest(),
                                next.completed_effect_count(),
                                current.working_roots(),
                            )
                        && (!matches!(active.effect(), DraftPieceMarkerEffectV1::Remove { .. })
                            || previous.writer_admission() == current.writer_admission())
                }
                None => {
                    fragment.replacement().marker_effect().is_none()
                        && scan.completed_effect_count() == next.completed_effect_count()
                        && scan.effect_chain() == next.effect_chain()
                        && previous.writer_admission() == current.writer_admission()
                }
            }
        }
        _ => false,
    }
}

pub(super) fn continuation(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    left: DraftPieceBuildMappingV1,
    right: DraftPieceBuildMappingV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> bool {
    let Some(fragment) = fragment else {
        return false;
    };
    fragment.replacement().is_continuation()
        && fragment.replacement().marker_effect().is_none()
        && matches!(previous.frontier(), Frontier::Planning { fragment_ordinal } if fragment_ordinal == fragment.key().ordinal())
        && current.frontier()
            == Frontier::Inserting {
                fragment_ordinal: fragment.key().ordinal(),
                next_piece: 0,
                next_byte: 0,
                base_end: previous.base_frontier(),
                successor_end: previous.successor_frontier(),
            }
        && previous.working_roots() == current.working_roots()
        && previous.base_frontier() == current.base_frontier()
        && previous.successor_frontier() == current.successor_frontier()
        && previous.marker_effect_continuation() == current.marker_effect_continuation()
        && previous.next_record_ordinal() == current.next_record_ordinal()
        && left.current_map == right.current_map
        && left.completed_source_unit == right.completed_source_unit
        && left.fragment_source_end_unit.is_none()
        && right.fragment_source_end_unit == Some(left.completed_source_unit)
}
