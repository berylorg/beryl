use super::*;

fn after_source(
    active: DraftPieceActiveMarkerEffectV1,
    base_frontier: DraftPieceBuildBoundaryV1,
) -> Pending {
    if active.fragment_key().ordinal() > 1
        && active.planning().and_then(|p| p.source_boundary) == Some(base_frontier)
    {
        primary(Purpose::PreviousStart)
    } else if has_removal(active.effect()) {
        primary(Purpose::WorkingOccurrence)
    } else {
        Pending::None
    }
}

fn source_position(
    purpose: Purpose,
    effect: DraftPieceMarkerEffectV1,
    fragment: &DraftPieceBuildFragmentV1,
) -> Option<DraftCompositePositionV1> {
    match purpose {
        Purpose::SourceBounds => Some(fragment.replacement().start()),
        Purpose::RemovalGap => match effect {
            DraftPieceMarkerEffectV1::Insert(_) => None,
            DraftPieceMarkerEffectV1::Remove { removal, .. }
            | DraftPieceMarkerEffectV1::Move { removal, .. }
            | DraftPieceMarkerEffectV1::SameIdReplacement { removal, .. } => {
                Some(removal.position())
            }
        },
        _ => None,
    }
}

pub(super) fn transition(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    left: DraftPieceActiveMarkerEffectV1,
    right: DraftPieceActiveMarkerEffectV1,
    fragment: &DraftPieceBuildFragmentV1,
) -> bool {
    if previous.base_frontier() != current.base_frontier() {
        return false;
    }
    if left.pending() == Pending::None {
        return finish(previous, current, left, right);
    }
    if previous.frontier() != current.frontier()
        || left.phase() != right.phase()
        || current.next_record_ordinal() < previous.next_record_ordinal()
    {
        return false;
    }
    match left.pending() {
        Pending::Proof {
            purpose,
            component,
            primary_marker_rank,
        } => {
            if left.working_roots() != right.working_roots()
                || previous.next_record_ordinal() != current.next_record_ordinal()
            {
                return false;
            }
            let Some(mut plan) = left.planning() else {
                return false;
            };
            let Some(next_plan) = right.planning() else {
                return false;
            };
            if let Pending::Proof {
                purpose: next_purpose,
                component: Component::Secondary,
                primary_marker_rank: Some(_),
            } = right.pending()
            {
                return component == Component::Primary
                    && purpose == next_purpose
                    && source_position(purpose, left.effect(), fragment).is_none_or(|position| {
                        matches!(
                            position.gap(),
                            DraftCompositeGapWitnessV1::AfterAll
                                | DraftCompositeGapWitnessV1::Between { .. }
                        )
                    })
                    && left.planning() == right.planning()
                    && left.removal_site() == right.removal_site();
            }
            if component == Component::Primary
                && source_position(purpose, left.effect(), fragment).is_some_and(|position| {
                    matches!(
                        position.gap(),
                        DraftCompositeGapWitnessV1::AfterAll
                            | DraftCompositeGapWitnessV1::Between { .. }
                    )
                })
            {
                return false;
            }
            if component == Component::Secondary
                && (primary_marker_rank.is_none()
                    || source_position(purpose, left.effect(), fragment).is_some_and(|position| {
                        !matches!(
                            position.gap(),
                            DraftCompositeGapWitnessV1::AfterAll
                                | DraftCompositeGapWitnessV1::Between { .. }
                        )
                    }))
            {
                return false;
            }
            let expected = match purpose {
                Purpose::SourceBounds => {
                    let Some(boundary) = next_plan.source_boundary else {
                        return false;
                    };
                    if (boundary.rank(), boundary.inner())
                        < (
                            previous.base_frontier().rank(),
                            previous.base_frontier().inner(),
                        )
                    {
                        return false;
                    }
                    plan.source_boundary = Some(boundary);
                    if has_removal(left.effect()) {
                        primary(Purpose::RemovalGap)
                    } else {
                        primary(Purpose::SourceInsertIdentityAbsent)
                    }
                }
                Purpose::RemovalGap => primary(Purpose::SourceOccurrence),
                Purpose::SourceOccurrence => primary(Purpose::SourceIdentity),
                Purpose::SourceIdentity | Purpose::SourceInsertIdentityAbsent => {
                    after_source(left, previous.base_frontier())
                }
                Purpose::PreviousStart => {
                    plan.previous_start = next_plan.previous_start;
                    if plan.previous_start.is_none() {
                        return false;
                    }
                    primary(Purpose::PreviousEnd)
                }
                Purpose::PreviousEnd => {
                    plan.previous_start = None;
                    if has_removal(left.effect()) {
                        primary(Purpose::WorkingOccurrence)
                    } else {
                        Pending::None
                    }
                }
                Purpose::WorkingOccurrence => primary(Purpose::WorkingIdentity),
                Purpose::WorkingIdentity => Pending::RemoveSequence,
                _ => return false,
            };
            right.pending() == expected
                && plan == next_plan
                && if purpose == Purpose::WorkingOccurrence {
                    right.removal_site().is_some()
                } else {
                    left.removal_site() == right.removal_site()
                }
        }
        Pending::RemoveSequence => {
            matches!(right.pending(), Pending::RemoveIdentity { sequence_target } if removed_sequence(left.working_roots(), sequence_target))
                && left.working_roots() == right.working_roots()
                && left.planning() == right.planning()
                && left.removal_site() == right.removal_site()
        }
        Pending::RemoveIdentity { sequence_target } => {
            matches!(right.pending(), Pending::RemoveOrder { sequence_target: next_sequence, identity_target } if sequence_target == next_sequence && removed_identity(left.working_roots(), identity_target))
                && left.working_roots() == right.working_roots()
                && left.planning() == right.planning()
                && left.removal_site() == right.removal_site()
        }
        Pending::RemoveOrder {
            sequence_target,
            identity_target,
        } => {
            right.pending() == Pending::None
                && left.planning() == right.planning()
                && left.removal_site() == right.removal_site()
                && installed_targets(right.working_roots(), sequence_target, identity_target)
                && left
                    .working_roots()
                    .marker_commitment()
                    .marker_count()
                    .checked_sub(1)
                    == Some(right.working_roots().marker_commitment().marker_count())
        }
        _ => false,
    }
}

fn removed_sequence(roots: DraftPieceBuildRootsV1, target: DraftPieceSequenceDescriptorV1) -> bool {
    let source = roots.sequence_summary();
    let target = target.summary;
    source.text_summary() == target.text_summary()
        && source.piece_count().checked_sub(1) == Some(target.piece_count())
        && source.marker_count().checked_sub(1) == Some(target.marker_count())
}

fn removed_identity(roots: DraftPieceBuildRootsV1, target: DraftPieceIdentityDescriptorV1) -> bool {
    roots.marker_index_summary().record_count().checked_sub(1)
        == Some(target.summary.record_count())
}

fn finish(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    left: DraftPieceActiveMarkerEffectV1,
    right: DraftPieceActiveMarkerEffectV1,
) -> bool {
    let Some(boundary) = left.planning().and_then(|plan| plan.source_boundary) else {
        return false;
    };
    let effective_end = match left.effect() {
        DraftPieceMarkerEffectV1::Remove { .. }
        | DraftPieceMarkerEffectV1::SameIdReplacement { .. } => {
            let Some(rank) = boundary.rank().checked_add(1) else {
                return false;
            };
            DraftPieceBuildBoundaryV1::new(rank, 0)
        }
        _ => boundary,
    };
    let source = previous.base_frontier();
    let successor = previous.successor_frontier();
    let mapped = if boundary.rank() == source.rank() {
        boundary
            .inner()
            .checked_sub(source.inner())
            .and_then(|delta| successor.inner().checked_add(delta))
            .map(|inner| DraftPieceBuildBoundaryV1::new(successor.rank(), inner))
    } else {
        boundary
            .rank()
            .checked_sub(source.rank())
            .and_then(|delta| successor.rank().checked_add(delta))
            .map(|rank| DraftPieceBuildBoundaryV1::new(rank, boundary.inner()))
    };
    let Some(mut mapped) = mapped else {
        return false;
    };
    if left
        .removal_site()
        .is_some_and(|site| site.piece_rank < mapped.rank())
    {
        let Some(rank) = mapped.rank().checked_sub(1) else {
            return false;
        };
        mapped = DraftPieceBuildBoundaryV1::new(rank, mapped.inner());
    }
    right.working_roots() == left.working_roots()
        && right.planning().is_none()
        && right.removal_site().is_none()
        && right.insertion_site().is_none()
        && right.pending() == Pending::None
        && right.phase() == Phase::Removing
        && previous.next_record_ordinal() == current.next_record_ordinal()
        && current.frontier()
            == DraftPieceBuildFrontierV1::Removing {
                fragment_ordinal: left.fragment_key().ordinal(),
                next_rank: boundary.rank(),
                end_rank: boundary.rank(),
                removed_markers: 0,
                base_end: effective_end,
                successor_start: mapped,
                successor_end: mapped,
            }
}
