use super::*;
use crate::draft_piece::deferred_prefix::{is_deferred, prefix_ordinal};

pub(super) fn control_transition(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> Option<bool> {
    let before = previous.marker_effect_continuation();
    let after = current.marker_effect_continuation();
    let old_mode = is_deferred(previous.frontier(), before);
    let new_mode = is_deferred(current.frontier(), after);
    let left = previous.mapping()?;
    let right = current.mapping()?;
    let shared = previous.lifecycle() == DraftPieceBuildLifecycleV1::Open
        && current.lifecycle() == DraftPieceBuildLifecycleV1::Open
        && previous.fragment_endpoint() == current.fragment_endpoint()
        && previous.durable_continuation() == current.durable_continuation()
        && previous.writer_admission() == current.writer_admission()
        && previous.successor() == current.successor()
        && previous.build_digest() == current.build_digest()
        && previous.base_frontier() == current.base_frontier()
        && previous.successor_frontier() == current.successor_frontier()
        && previous.next_record_ordinal() == current.next_record_ordinal()
        && left.current_map == right.current_map
        && left.completed_source_unit == right.completed_source_unit
        && left.fragment_source_end_unit == right.fragment_source_end_unit
        && before.source_logical_frontier() == after.source_logical_frontier()
        && before.successor_logical_frontier() == after.successor_logical_frontier();
    if !old_mode && new_mode {
        let fragment = fragment?;
        return Some(
            shared
                && previous.frontier() == current.frontier()
                && previous.working_roots() == current.working_roots()
                && before.active().is_none()
                && after.active().is_none()
                && matches!(previous.frontier(), Frontier::Applying { fragment_ordinal, successor_start, successor_end, .. } if fragment_ordinal == fragment.key().ordinal() && successor_start != successor_end)
                && crate::draft_piece::deferred_prefix::is_prefix(fragment)
                && matches!(
                    left.mapping_stage,
                    Stage::Idle
                        | Stage::Ready(DraftPieceMappingReadySpliceV1 {
                            kind: Kind::TextDelete,
                            ..
                        })
                )
                && right.mapping_stage == Stage::Idle
                && before.scan().next_fragment_ordinal().checked_add(1)
                    == Some(after.scan().next_fragment_ordinal())
                && after.scan().scanned_endpoint() == Some(canonical_fragment_endpoint(fragment))
                && before.scan().completed_effect_count() == after.scan().completed_effect_count()
                && before.scan().effect_chain() == after.scan().effect_chain(),
        );
    }
    if !old_mode || !new_mode {
        return None;
    }
    let Some(active) = before.active() else {
        return None;
    };
    if left.mapping_stage == Stage::MarkerMapBoundary && right.mapping_stage == Stage::Idle {
        if active.phase() != Phase::Removing
            || active.pending() != Pending::None
            || active.removal_site().is_some()
        {
            return Some(false);
        }
        if let Some(next) = after.active() {
            return Some(
                shared
                    && previous.frontier() == current.frontier()
                    && previous.working_roots() == current.working_roots()
                    && before.scan() == after.scan()
                    && next
                        == DraftPieceActiveMarkerEffectV1::new(
                            active.fragment_key(),
                            active.fragment_digest(),
                            active.effect(),
                            active.source_roots(),
                            active.working_roots(),
                            active.source_frontier(),
                            active.successor_frontier(),
                            Phase::DerivingInsertionGap,
                        )
                        .with_program(
                            None,
                            active.planning(),
                            None,
                            Pending::None,
                        ),
            );
        }
        let fragment = fragment?;
        let count = before.scan().completed_effect_count().checked_add(1)?;
        let endpoint = canonical_fragment_endpoint(fragment);
        return Some(
            shared
                && previous.frontier() == current.frontier()
                && fragment.key() == active.fragment_key()
                && endpoint.digest() == active.fragment_digest()
                && current.working_roots() == active.working_roots()
                && after.scan().next_fragment_ordinal()
                    == active.fragment_key().ordinal().checked_add(1)?
                && after.scan().scanned_endpoint() == Some(endpoint)
                && after.scan().completed_effect_count() == count
                && after.scan().effect_chain()
                    == draft_piece_marker_effect_chain_link_v1(
                        before.scan().effect_chain(),
                        fragment.key(),
                        endpoint.digest(),
                        count,
                        active.working_roots(),
                    ),
        );
    }
    if left.mapping_stage == Stage::Idle
        && right.mapping_stage == Stage::RefreshMap
        && active.phase() == Phase::DerivingInsertionGap
    {
        let next = after.active()?;
        let boundary = active.planning()?.source_boundary?;
        return Some(
            shared
                && before.scan() == after.scan()
                && previous.working_roots() == current.working_roots()
                && matches!(previous.frontier(), Frontier::Applying { successor_start, successor_end, .. } if successor_start == successor_end)
                && matches!(current.frontier(), Frontier::Inserting { fragment_ordinal, next_piece: 0, next_byte: 0, base_end, successor_end } if Some(fragment_ordinal) == prefix_ordinal(previous.frontier()) && base_end == boundary && matches!(previous.frontier(), Frontier::Applying { successor_start, .. } if successor_start == successor_end))
                && next
                    == DraftPieceActiveMarkerEffectV1::new(
                        active.fragment_key(),
                        active.fragment_digest(),
                        active.effect(),
                        active.source_roots(),
                        active.working_roots(),
                        active.source_frontier(),
                        active.successor_frontier(),
                        Phase::Publishing,
                    )
                    .with_program(None, None, None, Pending::None),
        );
    }
    None
}
