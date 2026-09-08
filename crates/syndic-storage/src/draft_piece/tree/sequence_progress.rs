use super::*;

fn actual_roots(receipt: &DraftPieceBuildProgressReceiptV1) -> DraftPieceBuildRootsV1 {
    receipt
        .marker_effect_continuation()
        .active()
        .map_or(receipt.working_roots(), |active| active.working_roots())
}

fn marker_state_is_unchanged(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
) -> bool {
    let before = previous.marker_effect_continuation();
    let after = current.marker_effect_continuation();
    if before.scan() != after.scan()
        || before.source_logical_frontier() != after.source_logical_frontier()
        || before.successor_logical_frontier() != after.successor_logical_frontier()
    {
        return false;
    }
    match (before.active(), after.active()) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.fragment_key() == right.fragment_key()
                && left.fragment_digest() == right.fragment_digest()
                && left.effect() == right.effect()
                && left.source_roots() == right.source_roots()
                && left.source_frontier() == right.source_frontier()
                && left.successor_frontier() == right.successor_frontier()
                && previous.working_roots() == current.working_roots()
        }
        _ => false,
    }
}

pub(super) fn transition_is_exact(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
) -> bool {
    let DraftPieceBuildFrontierV1::Applying {
        fragment_ordinal,
        base_end,
        successor_start: start,
        successor_end: end,
    } = previous.frontier()
    else {
        if let DraftPieceBuildFrontierV1::Applying {
            fragment_ordinal,
            base_end,
            successor_start,
            successor_end,
        } = current.frontier()
        {
            return matches!(previous.frontier(), DraftPieceBuildFrontierV1::Removing {
                fragment_ordinal: old_fragment, next_rank, end_rank, removed_markers: 0,
                base_end: old_base, successor_start: old_start, successor_end: old_end,
            } if old_fragment == fragment_ordinal && next_rank == end_rank
                && old_base == base_end && old_start == successor_start && old_end == successor_end)
                && actual_roots(previous) == actual_roots(current)
                && previous.base_frontier() == current.base_frontier()
                && previous.successor_frontier() == current.successor_frontier();
        }
        return true;
    };
    if current.previous() != Some(previous.reference())
        || current.fragment_endpoint() != previous.fragment_endpoint()
        || current.durable_continuation() != previous.durable_continuation()
        || current.writer_admission() != previous.writer_admission()
        || current.successor_frontier() != previous.successor_frontier()
        || !marker_state_is_unchanged(previous, current)
    {
        return false;
    }
    let before = actual_roots(previous);
    let after = actual_roots(current);
    if before.marker_index_root() != after.marker_index_root()
        || before.marker_index_summary() != after.marker_index_summary()
        || before.marker_order_root() != after.marker_order_root()
        || before.marker_order_height() != after.marker_order_height()
        || before.marker_commitment() != after.marker_commitment()
        || before.sequence_summary().marker_count() != after.sequence_summary().marker_count()
        || before.sequence_summary().marker_digest() != after.sequence_summary().marker_digest()
    {
        return false;
    }
    if matches!(
        current.lifecycle(),
        DraftPieceBuildLifecycleV1::Rejected
            | DraftPieceBuildLifecycleV1::Cancelled
            | DraftPieceBuildLifecycleV1::Error
    ) {
        return current.frontier() == previous.frontier()
            && before == after
            && current.base_frontier() == previous.base_frontier();
    }
    if start == end {
        return before == after
            && current.base_frontier() == base_end
            && current.next_record_ordinal() == previous.next_record_ordinal()
            && matches!(current.frontier(), DraftPieceBuildFrontierV1::Inserting {
                fragment_ordinal: next_fragment, next_piece: 0, next_byte: 0,
                base_end: next_base, successor_end: insertion,
            } if next_fragment == fragment_ordinal && next_base == base_end && insertion == start);
    }
    if (start.rank(), start.inner()) >= (end.rank(), end.inner())
        || current.base_frontier() != previous.base_frontier()
        || current.next_record_ordinal() < previous.next_record_ordinal()
        || (current.next_record_ordinal() == previous.next_record_ordinal()
            && after.sequence_root().is_some())
    {
        return false;
    }
    let DraftPieceBuildFrontierV1::Applying {
        fragment_ordinal: next_fragment,
        base_end: next_base,
        successor_start: next_start,
        successor_end: next_end,
    } = current.frontier()
    else {
        return false;
    };
    if fragment_ordinal != next_fragment || base_end != next_base {
        return false;
    }
    let Some(bytes) = before
        .sequence_summary()
        .logical_utf8_bytes()
        .checked_sub(after.sequence_summary().logical_utf8_bytes())
    else {
        return false;
    };
    let Some(pieces) = before
        .sequence_summary()
        .piece_count()
        .checked_sub(after.sequence_summary().piece_count())
    else {
        return false;
    };
    if bytes == 0 || bytes > DRAFT_PIECE_TEXT_LEAF_MAX_BYTES as u64 {
        return false;
    }
    if end.inner() > 0 {
        if start.rank() == end.rank() {
            pieces == 0
                && bytes == u64::from(end.inner() - start.inner())
                && next_start == start
                && next_end == start
        } else {
            pieces == 0
                && bytes == u64::from(end.inner())
                && next_start == start
                && next_end == DraftPieceBuildBoundaryV1::new(end.rank(), 0)
        }
    } else {
        let Some(rank) = end.rank().checked_sub(1) else {
            return false;
        };
        if rank < start.rank() {
            return false;
        }
        if rank == start.rank() && start.inner() > 0 {
            pieces == 0
                && u64::from(start.inner())
                    .checked_add(bytes)
                    .is_some_and(|total| total <= DRAFT_PIECE_TEXT_LEAF_MAX_BYTES as u64)
                && next_start == end
                && next_end == end
        } else {
            pieces == 1
                && next_start == start
                && next_end == DraftPieceBuildBoundaryV1::new(rank, 0)
        }
    }
}
