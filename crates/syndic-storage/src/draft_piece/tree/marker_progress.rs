use super::*;
use DraftPieceActiveMarkerPhaseV1 as Phase;
use DraftPieceMarkerPendingV1 as Pending;
use DraftPieceMarkerProofComponentV1 as Component;
use DraftPieceMarkerProofPurposeV1 as Purpose;

mod insertion;
mod planning;

fn insertion_count(effect: DraftPieceMarkerEffectV1) -> u64 {
    u64::from(!matches!(effect, DraftPieceMarkerEffectV1::Remove { .. }))
}

fn has_removal(effect: DraftPieceMarkerEffectV1) -> bool {
    !matches!(effect, DraftPieceMarkerEffectV1::Insert(_))
}

fn same_control(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
) -> bool {
    previous.fragment_endpoint() == current.fragment_endpoint()
        && previous.durable_continuation() == current.durable_continuation()
        && previous.writer_admission() == current.writer_admission()
        && previous.working_roots() == current.working_roots()
        && previous.successor_frontier() == current.successor_frontier()
}

pub(super) fn endpoint_is_exact(
    active: Option<DraftPieceActiveMarkerEffectV1>,
    frontier: DraftPieceBuildFrontierV1,
    roots: DraftPieceBuildRootsV1,
) -> bool {
    let Some(active) = active else {
        return true;
    };
    if !active.is_program_locally_exact() || active.source_roots() != roots {
        return false;
    }
    let ordinal = active.fragment_key().ordinal();
    match frontier {
        DraftPieceBuildFrontierV1::Planning { fragment_ordinal } => {
            fragment_ordinal == ordinal
                && active.planning().is_some()
                && active.phase() == Phase::Removing
        }
        DraftPieceBuildFrontierV1::Removing {
            fragment_ordinal,
            next_rank,
            end_rank,
            removed_markers,
            successor_start,
            successor_end,
            ..
        } => {
            fragment_ordinal == ordinal
                && next_rank == end_rank
                && removed_markers == 0
                && successor_start == successor_end
                && active.phase() == Phase::Removing
                && active.planning().is_none()
        }
        DraftPieceBuildFrontierV1::Applying {
            fragment_ordinal,
            successor_start,
            successor_end,
            ..
        } => {
            fragment_ordinal == ordinal
                && successor_start == successor_end
                && active.phase() == Phase::DerivingInsertionGap
                && active.pending() == Pending::None
                && active.planning().is_none()
        }
        DraftPieceBuildFrontierV1::Inserting {
            fragment_ordinal,
            next_piece,
            next_byte,
            successor_end,
            ..
        } => {
            if fragment_ordinal != ordinal || next_byte != 0 || active.planning().is_some() {
                return false;
            }
            match active.phase() {
                Phase::Publishing => next_piece == insertion_count(active.effect()),
                Phase::DerivingInsertionGap => {
                    next_piece == 0
                        && active.pending() != Pending::None
                        && insertion_count(active.effect()) == 1
                }
                Phase::Inserting => {
                    let Some(site) = active.insertion_site() else {
                        return false;
                    };
                    next_piece == 0
                        && site.marker_ordinal
                            <= active.working_roots().sequence_summary().marker_count()
                        && site.boundary.rank()
                            <= active.working_roots().sequence_summary().piece_count()
                }
                Phase::Removing => false,
            }
        }
        _ => false,
    }
}

fn fragment_matches(
    active: DraftPieceActiveMarkerEffectV1,
    fragment: &DraftPieceBuildFragmentV1,
) -> bool {
    let inserted_marker = match active.effect() {
        DraftPieceMarkerEffectV1::Insert(insertion)
        | DraftPieceMarkerEffectV1::Move { insertion, .. }
        | DraftPieceMarkerEffectV1::SameIdReplacement { insertion, .. } => Some(insertion.marker()),
        DraftPieceMarkerEffectV1::Remove { .. } => None,
    };
    active.fragment_key() == fragment.key()
        && active.fragment_digest() == draft_piece_fragment_digest_v1(fragment)
        && fragment.replacement().marker_effect() == Some(active.effect())
        && fragment.replacement().start() == fragment.replacement().end()
        && !fragment.replacement().is_continuation()
        && fragment.replacement().inserted().len() as u64 == insertion_count(active.effect())
        && fragment
            .replacement()
            .inserted()
            .iter()
            .all(|piece| matches!(piece, DraftPieceV1::Marker(marker) if Some(*marker) == inserted_marker))
}

fn frozen_active(
    left: DraftPieceActiveMarkerEffectV1,
    right: DraftPieceActiveMarkerEffectV1,
) -> bool {
    left.fragment_key() == right.fragment_key()
        && left.fragment_digest() == right.fragment_digest()
        && left.effect() == right.effect()
        && left.source_roots() == right.source_roots()
        && left.source_frontier() == right.source_frontier()
        && left.successor_frontier() == right.successor_frontier()
}

fn primary(purpose: Purpose) -> Pending {
    Pending::Proof {
        purpose,
        component: Component::Primary,
        primary_marker_rank: None,
    }
}

pub(super) fn transition_is_exact(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> bool {
    let left = previous.marker_effect_continuation().active();
    let right = current.marker_effect_continuation().active();
    if left.is_none() && right.is_none() {
        return true;
    }
    if !same_control(previous, current) || current.previous() != Some(previous.reference()) {
        return false;
    }
    if matches!(
        current.lifecycle(),
        DraftPieceBuildLifecycleV1::Rejected
            | DraftPieceBuildLifecycleV1::Cancelled
            | DraftPieceBuildLifecycleV1::Error
    ) {
        return left == right
            && previous.frontier() == current.frontier()
            && previous.base_frontier() == current.base_frontier()
            && previous.next_record_ordinal() == current.next_record_ordinal();
    }
    if previous.lifecycle() != DraftPieceBuildLifecycleV1::Open
        || current.lifecycle() != DraftPieceBuildLifecycleV1::Open
    {
        return false;
    }
    let Some(right) = right else {
        return false;
    };
    if !endpoint_is_exact(Some(right), current.frontier(), current.working_roots()) {
        return false;
    }
    let Some(fragment) = fragment else {
        return false;
    };
    if !fragment_matches(right, fragment) {
        return false;
    }
    let Some(left) = left else {
        return matches!(previous.frontier(), DraftPieceBuildFrontierV1::Planning { fragment_ordinal } if fragment_ordinal == right.fragment_key().ordinal())
            && previous.frontier() == current.frontier()
            && previous.base_frontier() == current.base_frontier()
            && previous.next_record_ordinal() == current.next_record_ordinal()
            && right.working_roots() == right.source_roots()
            && right.phase() == Phase::Removing
            && right.planning()
                == Some(DraftPieceMarkerPlanningV1 {
                    source_boundary: None,
                    previous_start: None,
                })
            && right.removal_site().is_none()
            && right.insertion_site().is_none()
            && right.pending() == primary(Purpose::SourceBounds);
    };
    if !frozen_active(left, right)
        || !endpoint_is_exact(Some(left), previous.frontier(), previous.working_roots())
    {
        return false;
    }
    match previous.frontier() {
        DraftPieceBuildFrontierV1::Planning { .. } => {
            planning::transition(previous, current, left, right, fragment)
        }
        DraftPieceBuildFrontierV1::Removing { .. } => {
            matches!(
                current.frontier(),
                DraftPieceBuildFrontierV1::Applying { .. }
            ) && right
                == DraftPieceActiveMarkerEffectV1::new(
                    left.fragment_key(),
                    left.fragment_digest(),
                    left.effect(),
                    left.source_roots(),
                    left.working_roots(),
                    left.source_frontier(),
                    left.successor_frontier(),
                    Phase::DerivingInsertionGap,
                )
                && previous.next_record_ordinal() == current.next_record_ordinal()
        }
        DraftPieceBuildFrontierV1::Applying { .. } => {
            matches!(
                current.frontier(),
                DraftPieceBuildFrontierV1::Inserting { .. }
            ) && right.working_roots() == left.working_roots()
                && right.phase()
                    == if insertion_count(left.effect()) == 0 {
                        Phase::Publishing
                    } else {
                        Phase::DerivingInsertionGap
                    }
                && right.pending()
                    == if insertion_count(left.effect()) == 0 {
                        Pending::None
                    } else {
                        primary(Purpose::InsertIdentityAbsent)
                    }
                && previous.next_record_ordinal() == current.next_record_ordinal()
        }
        DraftPieceBuildFrontierV1::Inserting { .. } => {
            insertion::transition(previous, current, left, right)
        }
        _ => false,
    }
}

fn installed_targets(
    roots: DraftPieceBuildRootsV1,
    sequence: DraftPieceSequenceDescriptorV1,
    identity: DraftPieceIdentityDescriptorV1,
) -> bool {
    roots.sequence_root() == sequence.root_node_id
        && roots.sequence_summary() == sequence.summary
        && roots.marker_index_root() == identity.root_node_id
        && roots.marker_index_summary() == identity.summary
}
