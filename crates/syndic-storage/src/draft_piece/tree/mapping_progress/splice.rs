use super::*;

fn frozen(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
) -> bool {
    previous.working_roots() == current.working_roots()
        && previous.frontier() == current.frontier()
        && previous.base_frontier() == current.base_frontier()
        && previous.successor_frontier() == current.successor_frontier()
        && previous.marker_effect_continuation() == current.marker_effect_continuation()
}

fn emitted(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    maximum: u64,
) -> bool {
    current
        .next_record_ordinal()
        .checked_sub(previous.next_record_ordinal())
        .is_some_and(|n| n <= maximum)
}

fn chunk(
    fragment: &DraftPieceBuildFragmentV1,
    frontier: Frontier,
) -> Option<(u64, u64, u128, u64)> {
    let Frontier::Inserting {
        next_piece,
        next_byte,
        ..
    } = frontier
    else {
        return None;
    };
    let DraftPieceV1::Text(text) = fragment
        .replacement()
        .inserted()
        .get(usize::try_from(next_piece).ok()?)?
    else {
        return None;
    };
    let start = usize::try_from(next_byte).ok()?;
    if start >= text.len() || !text.is_char_boundary(start) {
        return None;
    }
    let mut end = start
        .checked_add(DRAFT_PIECE_TEXT_LEAF_MAX_BYTES)?
        .min(text.len());
    while !text.is_char_boundary(end) {
        end = end.checked_sub(1)?;
    }
    let length = (end - start) as u128;
    let newlines = text.as_bytes()[start..end]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count() as u64;
    if end == text.len() {
        Some((next_piece.checked_add(1)?, 0, length, newlines))
    } else {
        Some((next_piece, u64::try_from(end).ok()?, length, newlines))
    }
}

pub(super) fn transition(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    left: DraftPieceBuildMappingV1,
    right: DraftPieceBuildMappingV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> bool {
    if left.fragment_source_end_unit != right.fragment_source_end_unit {
        return false;
    }
    let same_map = left.current_map == right.current_map;
    let control = control_frozen(previous, current);
    match (left.mapping_stage, right.mapping_stage) {
        (Stage::Idle, Stage::TextDeleteProof) => control && same_map,
        (Stage::Idle, Stage::TextInsertProof) => {
            control
                && same_map
                && fragment.is_some_and(|fragment| chunk(fragment, previous.frontier()).is_some())
        }
        (
            Stage::TextDeleteProof,
            Stage::DeleteMap {
                splice,
                target,
                remaining_end,
            },
        ) => {
            control
                && same_map
                && splice.kind == Kind::TextDelete
                && target == left.current_map
                && splice.a.checked_add(splice.removed) == Some(remaining_end)
        }
        (
            Stage::MarkerWorkingIdentity { mapped_unit },
            Stage::DeleteMap {
                splice,
                target,
                remaining_end,
            },
        ) => {
            control
                && same_map
                && splice.kind == Kind::MarkerDelete
                && target == left.current_map
                && splice.a == mapped_unit
                && splice.a.checked_add(1) == Some(remaining_end)
        }
        (Stage::TextInsertProof, Stage::InsertMap { splice }) => {
            control
                && same_map
                && splice.kind == Kind::TextInsert
                && fragment
                    .and_then(|fragment| chunk(fragment, previous.frontier()))
                    .is_some_and(|(_, _, n, _)| n == splice.inserted)
        }
        (Stage::Idle, Stage::InsertMap { splice }) => {
            same_map
                && splice.kind == Kind::MarkerInsert
                && super::super::marker_progress::transition_is_exact(previous, current, fragment)
        }
        (
            Stage::DeleteMap {
                splice: a,
                target: old_target,
                remaining_end: old_end,
            },
            Stage::DeleteMap {
                splice: b,
                target,
                remaining_end,
            },
        ) => {
            frozen(previous, current)
                && same_map
                && a == b
                && remaining_end < old_end
                && remaining_end > a.a
                && old_target
                    .measure()
                    .target
                    .checked_sub(old_end - remaining_end)
                    == Some(target.measure().target)
                && target != old_target
                && emitted(previous, current, 45)
        }
        (
            Stage::DeleteMap {
                splice: a,
                target: old_target,
                remaining_end,
            },
            Stage::MapComplete { splice: b, target },
        ) => {
            frozen(previous, current)
                && same_map
                && a == b
                && remaining_end > a.a
                && old_target.measure().target.checked_sub(remaining_end - a.a)
                    == Some(target.measure().target)
                && target != old_target
                && emitted(previous, current, 45)
        }
        (Stage::InsertMap { splice: a }, Stage::MapComplete { splice: b, target }) => {
            frozen(previous, current)
                && same_map
                && a == b
                && target != left.current_map
                && emitted(previous, current, 45)
        }
        (Stage::MapComplete { splice, target }, Stage::Ready(ready)) => {
            if !same_map
                || ready
                    != (DraftPieceMappingReadySpliceV1 {
                        kind: splice.kind,
                        a: splice.a,
                        removed: splice.removed,
                        inserted: splice.inserted,
                        target,
                    })
            {
                return false;
            }
            if matches!(splice.kind, Kind::TextDelete | Kind::TextInsert) {
                return control;
            }
            let (Some(a), Some(b)) = (
                previous.marker_effect_continuation().active(),
                current.marker_effect_continuation().active(),
            ) else {
                return false;
            };
            let pending = if splice.kind == Kind::MarkerDelete {
                Pending::RemoveSequence
            } else {
                Pending::InsertSequence
            };
            previous.working_roots() == current.working_roots()
                && previous.frontier() == current.frontier()
                && previous.base_frontier() == current.base_frontier()
                && previous.successor_frontier() == current.successor_frontier()
                && previous.next_record_ordinal() == current.next_record_ordinal()
                && a.with_program(a.removal_site(), a.planning(), a.insertion_site(), pending) == b
                && previous.marker_effect_continuation().scan()
                    == current.marker_effect_continuation().scan()
        }
        (Stage::Ready(ready), Stage::TextDeleteProof | Stage::Idle)
            if ready.kind == Kind::TextDelete =>
        {
            right.current_map == ready.target
                && super::super::sequence_progress::transition_is_exact(previous, current)
                && previous.marker_effect_continuation() == current.marker_effect_continuation()
                && u128::from(
                    actual_roots(previous)
                        .sequence_summary()
                        .logical_utf8_bytes(),
                )
                .checked_sub(ready.removed)
                    == Some(u128::from(
                        actual_roots(current)
                            .sequence_summary()
                            .logical_utf8_bytes(),
                    ))
                && matches!(current.frontier(), Frontier::Applying { successor_start, successor_end, .. } if (successor_start == successor_end) == (right.mapping_stage == Stage::Idle))
        }
        (Stage::Ready(ready), Stage::Idle) if ready.kind == Kind::TextInsert => {
            text_insertion(previous, current, ready, fragment) && right.current_map == ready.target
        }
        (Stage::Ready(a), Stage::Ready(b)) => {
            same_map
                && a == b
                && matches!(a.kind, Kind::MarkerDelete | Kind::MarkerInsert)
                && super::super::marker_progress::transition_is_exact(previous, current, fragment)
        }
        (Stage::Ready(ready), Stage::MarkerMapBoundary) => {
            ready.kind == Kind::MarkerDelete
                && right.current_map == ready.target
                && super::super::marker_progress::transition_is_exact(previous, current, fragment)
        }
        (Stage::Ready(ready), Stage::RefreshMap) => {
            ready.kind == Kind::MarkerInsert
                && right.current_map == ready.target
                && super::super::marker_progress::transition_is_exact(previous, current, fragment)
        }
        _ => false,
    }
}

fn text_insertion(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    ready: DraftPieceMappingReadySpliceV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> bool {
    let Some(fragment) = fragment else {
        return false;
    };
    let Some((next_piece, next_byte, inserted, newlines)) = chunk(fragment, previous.frontier())
    else {
        return false;
    };
    let Frontier::Inserting {
        fragment_ordinal,
        base_end,
        successor_end,
        ..
    } = previous.frontier()
    else {
        return false;
    };
    let Some(rank) = successor_end
        .rank()
        .checked_add(1 + u64::from(successor_end.inner() != 0))
    else {
        return false;
    };
    let before = previous.working_roots();
    let after = current.working_roots();
    inserted == ready.inserted
        && current.frontier()
            == Frontier::Inserting {
                fragment_ordinal,
                next_piece,
                next_byte,
                base_end,
                successor_end: DraftPieceBuildBoundaryV1::new(rank, 0),
            }
        && previous.base_frontier() == current.base_frontier()
        && previous.successor_frontier() == current.successor_frontier()
        && previous.marker_effect_continuation() == current.marker_effect_continuation()
        && current.next_record_ordinal() > previous.next_record_ordinal()
        && before
            .sequence_summary()
            .logical_utf8_bytes()
            .checked_add(u64::try_from(inserted).unwrap_or(u64::MAX))
            == Some(after.sequence_summary().logical_utf8_bytes())
        && before
            .sequence_summary()
            .piece_count()
            .checked_add(1 + u64::from(successor_end.inner() != 0))
            == Some(after.sequence_summary().piece_count())
        && before
            .sequence_summary()
            .newline_count()
            .checked_add(newlines)
            == Some(after.sequence_summary().newline_count())
        && before.marker_index_root() == after.marker_index_root()
        && before.marker_index_summary() == after.marker_index_summary()
        && before.marker_order_root() == after.marker_order_root()
        && before.marker_order_height() == after.marker_order_height()
        && before.marker_commitment() == after.marker_commitment()
        && before.sequence_summary().marker_count() == after.sequence_summary().marker_count()
        && before.sequence_summary().marker_digest() == after.sequence_summary().marker_digest()
}
