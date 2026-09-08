use super::super::build_mapping::model::MapRoot;
use super::*;
use DraftPieceActiveMarkerPhaseV1 as Phase;
use DraftPieceBuildFrontierV1 as Frontier;
use DraftPieceMappingSpliceKindV1 as Kind;
use DraftPieceMappingStageV1 as Stage;
use DraftPieceMarkerPendingV1 as Pending;

mod publication;
mod query;
mod splice;

fn units(roots: DraftPieceBuildRootsV1) -> u128 {
    u128::from(roots.sequence_summary().logical_utf8_bytes())
        + u128::from(roots.sequence_summary().marker_count())
}

fn actual_roots(receipt: &DraftPieceBuildProgressReceiptV1) -> DraftPieceBuildRootsV1 {
    receipt
        .marker_effect_continuation()
        .active()
        .map_or(receipt.working_roots(), |active| active.working_roots())
}

fn boundary_valid(boundary: DraftPieceBuildBoundaryV1, roots: DraftPieceBuildRootsV1) -> bool {
    boundary.rank() <= roots.sequence_summary().piece_count()
        && (boundary.rank() != roots.sequence_summary().piece_count() || boundary.inner() == 0)
        && boundary.inner() < DRAFT_PIECE_TEXT_LEAF_MAX_BYTES as u64
}

fn ordered(a: DraftPieceBuildBoundaryV1, b: DraftPieceBuildBoundaryV1) -> bool {
    (a.rank(), a.inner()) <= (b.rank(), b.inner())
}

fn proof_valid(proof: DraftPieceMappingProofComponentV1) -> bool {
    proof.primary_marker_rank.is_some()
        == (proof.component == DraftPieceMarkerProofComponentV1::Secondary)
}

pub(super) fn source_extent_is_exact(build: &DraftPieceBuildRecordV1) -> bool {
    let Some(mapping) = build.mapping() else {
        return false;
    };
    let roots = DraftPieceBuildRootsV1::from_root(build.predecessor_root());
    let extent = units(roots);
    let boundary = |b: DraftPieceBuildBoundaryV1| boundary_valid(b, roots);
    let fact = |b: DraftPieceBuildBoundaryV1, unit: u128| {
        boundary(b)
            && unit >= u128::from(b.rank()) + u128::from(b.inner())
            && unit <= extent
            && (b.rank() == roots.sequence_summary().piece_count()) == (unit == extent)
    };
    let proof = |p: DraftPieceMappingProofComponentV1| {
        p.primary_marker_rank
            .is_none_or(|rank| rank < roots.sequence_summary().piece_count())
    };
    if !fact(build.base_frontier(), mapping.completed_source_unit) {
        return false;
    }
    if let Some(active) = build.marker_effect_continuation().active() {
        if active
            .planning()
            .is_some_and(|p| p.previous_start.is_some_and(|b| !boundary(b)))
        {
            return false;
        }
        if let Some(b) = active.planning().and_then(|p| p.source_boundary) {
            if mapping
                .fragment_source_end_unit
                .is_none_or(|unit| !fact(b, unit))
            {
                return false;
            }
        }
        if let Pending::Proof {
            primary_marker_rank: Some(rank),
            ..
        } = active.pending()
        {
            if rank >= roots.sequence_summary().piece_count() {
                return false;
            }
        }
    }
    if let Frontier::Removing { base_end, .. }
    | Frontier::Applying { base_end, .. }
    | Frontier::Inserting { base_end, .. } = build.frontier()
    {
        if mapping
            .fragment_source_end_unit
            .is_none_or(|unit| !fact(base_end, unit))
        {
            return false;
        }
    }
    match mapping.mapping_stage {
        Stage::TextSourceStart { proof: p } => proof(p),
        Stage::TextSourceEnd { start, proof: p } => fact(start.boundary, start.unit) && proof(p),
        Stage::TextPreviousStart {
            start,
            end,
            proof: p,
        } => fact(start.boundary, start.unit) && fact(end.boundary, end.unit) && proof(p),
        Stage::TextPreviousEnd {
            start,
            end,
            previous_start,
            proof: p,
        } => {
            fact(start.boundary, start.unit)
                && fact(end.boundary, end.unit)
                && boundary(previous_start)
                && proof(p)
        }
        Stage::TextMapStart { start, end } => {
            fact(start.boundary, start.unit) && fact(end.boundary, end.unit)
        }
        Stage::TextMapEnd {
            source_start,
            source_end,
            ..
        }
        | Stage::TextResolveStart {
            source_start,
            source_end,
            ..
        }
        | Stage::TextResolveEnd {
            source_start,
            source_end,
            ..
        } => {
            boundary(source_start)
                && mapping
                    .fragment_source_end_unit
                    .is_some_and(|unit| fact(source_end, unit))
        }
        _ => true,
    }
}

fn delta_valid(kind: Kind, removed: u128, inserted: u128) -> bool {
    match kind {
        Kind::TextDelete => {
            (1..=DRAFT_PIECE_TEXT_LEAF_MAX_BYTES as u128).contains(&removed) && inserted == 0
        }
        Kind::TextInsert => {
            removed == 0 && (1..=DRAFT_PIECE_TEXT_LEAF_MAX_BYTES as u128).contains(&inserted)
        }
        Kind::MarkerDelete => removed == 1 && inserted == 0,
        Kind::MarkerInsert => removed == 0 && inserted == 1,
    }
}

fn completed_target(
    current: MapRoot,
    target: MapRoot,
    a: u128,
    removed: u128,
    inserted: u128,
) -> bool {
    current.valid()
        && target.valid()
        && a.checked_add(removed)
            .is_some_and(|end| end <= current.measure().target)
        && target.measure().source == current.measure().source
        && current
            .measure()
            .target
            .checked_sub(removed)
            .and_then(|n| n.checked_add(inserted))
            == Some(target.measure().target)
}

pub(super) fn endpoint_is_exact(
    mapping: Option<DraftPieceBuildMappingV1>,
    active: Option<DraftPieceActiveMarkerEffectV1>,
    frontier: Frontier,
    roots: DraftPieceBuildRootsV1,
    original_units: Option<u128>,
) -> bool {
    let Some(mapping) = mapping else {
        return false;
    };
    let effective = active.map_or(roots, |active| active.working_roots());
    let measure = mapping.current_map.measure();
    if !mapping.current_map.valid()
        || original_units.is_some_and(|n| n != measure.source)
        || measure.target != units(effective)
        || mapping.completed_source_unit > measure.source
        || mapping
            .fragment_source_end_unit
            .is_some_and(|end| end < mapping.completed_source_unit || end > measure.source)
        || active.is_some_and(|a| a.source_roots() != roots || !a.is_program_locally_exact())
    {
        return false;
    }
    let stage = mapping.mapping_stage;
    let fragment_end = mapping.fragment_source_end_unit;
    let planning = matches!(frontier, Frontier::Planning { .. });
    let applying = matches!(frontier, Frontier::Applying { .. });
    let inserting = matches!(frontier, Frontier::Inserting { .. });
    let text_planning = planning && active.is_none();
    let marker_planning =
        planning && active.is_some_and(|a| a.phase() == Phase::Removing && a.planning().is_some());
    let marker_idle = active.is_none_or(|a| a.pending() == Pending::None);
    let fact = |fact: DraftPieceMappingSourceFactV1| {
        fact.unit <= measure.source && fact.unit >= mapping.completed_source_unit
    };
    let facts = |start: DraftPieceMappingSourceFactV1, end: DraftPieceMappingSourceFactV1| {
        fact(start)
            && fact(end)
            && start.unit <= end.unit
            && ordered(start.boundary, end.boundary)
            && fragment_end == Some(end.unit)
    };
    match stage {
        Stage::Idle => match frontier {
            Frontier::Receiving { .. } | Frontier::CrossValidating | Frontier::Complete | Frontier::Planning { .. } => active.is_none() && fragment_end.is_none(),
            Frontier::Removing { next_rank, end_rank, removed_markers, .. } => fragment_end.is_some() && marker_idle && next_rank == end_rank && removed_markers == 0,
            Frontier::Applying { .. } => fragment_end.is_some() && marker_idle,
            Frontier::Inserting { .. } => fragment_end.is_some() && active.is_none_or(|a| a.phase() == Phase::DerivingInsertionGap),
        },
        Stage::TextSourceStart { proof } => text_planning && fragment_end.is_none() && proof_valid(proof),
        Stage::TextSourceEnd { start, proof } => text_planning && fragment_end.is_none() && start.unit <= measure.source && proof_valid(proof),
        Stage::TextPreviousStart { start, end, proof } | Stage::TextPreviousEnd { start, end, proof, .. } => {
            text_planning && facts(start, end) && start.unit == end.unit && end.unit == mapping.completed_source_unit
                && matches!(frontier, Frontier::Planning { fragment_ordinal } if fragment_ordinal > 1) && proof_valid(proof)
        }
        Stage::TextMapStart { start, end } => text_planning && facts(start, end),
        Stage::TextMapEnd { source_start, source_end, mapped_start } => text_planning && fragment_end.is_some() && ordered(source_start, source_end) && mapped_start <= measure.target,
        Stage::TextResolveStart { source_start, source_end, mapped_start, mapped_end } => text_planning && fragment_end.is_some() && ordered(source_start, source_end) && mapped_start <= mapped_end && mapped_end <= measure.target,
        Stage::TextResolveEnd { source_start, source_end, mapped_end, start_boundary, start_marker_ordinal } => text_planning && fragment_end.is_some() && ordered(source_start, source_end) && mapped_end <= measure.target && boundary_valid(start_boundary, effective) && start_marker_ordinal <= effective.sequence_summary().marker_count(),
        Stage::MarkerSource { removal_source_unit } => {
            marker_planning && active.is_some_and(|a| {
                let Pending::Proof { purpose, .. } = a.pending() else { return false; };
                use DraftPieceMarkerProofPurposeV1 as Purpose;
                let removes = !matches!(a.effect(), DraftPieceMarkerEffectV1::Insert(_));
                let source_done = matches!(purpose, Purpose::SourceIdentity | Purpose::PreviousStart | Purpose::PreviousEnd);
                fragment_end.is_none() == (purpose == Purpose::SourceBounds)
                    && removal_source_unit.is_some() == (removes && source_done)
                    && removal_source_unit.is_none_or(|unit| unit < measure.source)
                    && a.working_roots() == a.source_roots() && a.removal_site().is_none()
            })
        }
        Stage::MarkerMapRemoval { source_unit } => marker_planning && fragment_end.is_some() && marker_idle && source_unit < measure.source && active.is_some_and(|a| a.removal_site().is_none() && a.working_roots() == a.source_roots() && !matches!(a.effect(), DraftPieceMarkerEffectV1::Insert(_))),
        Stage::MarkerResolveRemoval { mapped_unit } => marker_planning && fragment_end.is_some() && marker_idle && mapped_unit < measure.target && active.is_some_and(|a| a.removal_site().is_none() && a.working_roots() == a.source_roots()),
        Stage::MarkerWorkingIdentity { mapped_unit } => marker_planning && fragment_end.is_some() && marker_idle && mapped_unit < measure.target && active.is_some_and(|a| a.removal_site().is_some() && a.working_roots() == a.source_roots()),
        Stage::MarkerMapBoundary | Stage::MarkerResolveBoundary { .. } | Stage::MarkerPlanningReady { .. } => {
            marker_planning && fragment_end.is_some() && marker_idle && active.is_some_and(|a| a.removal_site().is_none())
                && match stage { Stage::MarkerResolveBoundary { mapped_unit } => mapped_unit <= measure.target, Stage::MarkerPlanningReady { boundary } => boundary_valid(boundary, effective), _ => true }
        }
        Stage::TextDeleteProof => applying && active.is_none() && fragment_end.is_some() && matches!(frontier, Frontier::Applying { successor_start, successor_end, .. } if successor_start != successor_end && ordered(successor_start, successor_end)),
        Stage::TextInsertProof => inserting && active.is_none() && fragment_end.is_some(),
        Stage::DeleteMap { splice, target, remaining_end } => {
            fragment_end.is_some() && marker_idle && splice_valid(splice, active, frontier, effective)
                && matches!(splice.kind, Kind::TextDelete | Kind::MarkerDelete)
                && splice.a.checked_add(splice.removed).is_some_and(|end| {
                    splice.a <= remaining_end && remaining_end <= end && end <= measure.target
                        && target.valid() && target.measure().source == measure.source
                        && measure.target.checked_sub(end - remaining_end) == Some(target.measure().target)
                })
        }
        Stage::InsertMap { splice } => fragment_end.is_some() && marker_idle && splice_valid(splice, active, frontier, effective) && matches!(splice.kind, Kind::TextInsert | Kind::MarkerInsert) && splice.a <= measure.target,
        Stage::MapComplete { splice, target } => fragment_end.is_some() && marker_idle && splice_valid(splice, active, frontier, effective) && completed_target(mapping.current_map, target, splice.a, splice.removed, splice.inserted),
        Stage::Ready(ready) => fragment_end.is_some() && delta_valid(ready.kind, ready.removed, ready.inserted)
            && completed_target(mapping.current_map, ready.target, ready.a, ready.removed, ready.inserted)
            && match ready.kind {
                Kind::TextDelete => active.is_none() && applying,
                Kind::TextInsert => active.is_none() && inserting,
                Kind::MarkerDelete => marker_planning && active.is_some_and(|a| a.removal_site().is_some() && matches!(a.pending(), Pending::RemoveSequence | Pending::RemoveIdentity { .. } | Pending::RemoveOrder { .. })),
                Kind::MarkerInsert => inserting && active.is_some_and(|a| a.phase() == Phase::Inserting && a.insertion_site().is_some() && matches!(a.pending(), Pending::InsertSequence | Pending::InsertIdentity { .. } | Pending::InsertOrder { .. })),
            },
        Stage::RefreshMap | Stage::RefreshSequence { .. } | Stage::PublishReady { .. } => {
            inserting && fragment_end.is_some() && marker_idle && active.is_none_or(|a| {
                a.phase() == Phase::Publishing && a.planning().is_none() && a.removal_site().is_none() && a.insertion_site().is_none()
                    && matches!(frontier, Frontier::Inserting { next_piece, next_byte: 0, .. } if next_piece == u64::from(!matches!(a.effect(), DraftPieceMarkerEffectV1::Remove { .. })))
            }) && match stage {
                Stage::RefreshSequence { mapped_unit } => mapped_unit <= measure.target,
                Stage::PublishReady { successor_boundary, logical_offset } => boundary_valid(successor_boundary, effective) && logical_offset <= effective.sequence_summary().logical_utf8_bytes(),
                _ => true,
            }
        }
    }
}

fn splice_valid(
    splice: DraftPieceMappingSpliceV1,
    active: Option<DraftPieceActiveMarkerEffectV1>,
    frontier: Frontier,
    effective: DraftPieceBuildRootsV1,
) -> bool {
    if !delta_valid(splice.kind, splice.removed, splice.inserted) {
        return false;
    }
    match splice.kind {
        Kind::TextDelete => {
            active.is_none()
                && splice.leaf.is_some()
                && splice.local_end <= DRAFT_PIECE_TEXT_LEAF_MAX_BYTES as u64
                && splice
                    .local_end
                    .checked_sub(splice.local_start)
                    .map(u128::from)
                    == Some(splice.removed)
                && matches!(frontier, Frontier::Applying { successor_start: start, successor_end: end, .. }
                if start != end && ordered(start, end) && splice.rank == if end.inner() == 0 { end.rank().checked_sub(1).unwrap_or(u64::MAX) } else { end.rank() }
                && splice.local_start == if splice.rank == start.rank() { start.inner() } else { 0 }
                && (end.inner() == 0 || splice.local_end == end.inner()))
        }
        Kind::TextInsert => {
            active.is_none()
                && matches!(frontier, Frontier::Inserting { successor_end, .. } if splice.rank == successor_end.rank() && splice.local_start == successor_end.inner())
                && splice.leaf.is_none()
                && splice.local_end == 0
                && boundary_valid(
                    DraftPieceBuildBoundaryV1::new(splice.rank, splice.local_start),
                    effective,
                )
        }
        Kind::MarkerDelete => {
            matches!(frontier, Frontier::Planning { .. })
                && splice.leaf.is_none()
                && splice.local_start == 0
                && splice.local_end == 0
                && active.is_some_and(|a| {
                    a.removal_site()
                        .is_some_and(|site| site.piece_rank == splice.rank)
                })
        }
        Kind::MarkerInsert => {
            matches!(
                frontier,
                Frontier::Inserting {
                    next_piece: 0,
                    next_byte: 0,
                    ..
                }
            ) && splice.leaf.is_none()
                && splice.local_end == 0
                && active.is_some_and(|a| {
                    a.insertion_site().is_some_and(|site| {
                        site.boundary
                            == DraftPieceBuildBoundaryV1::new(splice.rank, splice.local_start)
                    })
                })
        }
    }
}

fn control_frozen(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
) -> bool {
    previous.working_roots() == current.working_roots()
        && previous.base_frontier() == current.base_frontier()
        && previous.successor_frontier() == current.successor_frontier()
        && previous.frontier() == current.frontier()
        && previous.next_record_ordinal() == current.next_record_ordinal()
        && previous.marker_effect_continuation() == current.marker_effect_continuation()
}

pub(super) fn transition_is_exact(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> bool {
    let (Some(left), Some(right)) = (previous.mapping(), current.mapping()) else {
        return false;
    };
    if current.previous() != Some(previous.reference())
        || !endpoint_is_exact(
            previous.mapping(),
            previous.marker_effect_continuation().active(),
            previous.frontier(),
            previous.working_roots(),
            None,
        )
        || !endpoint_is_exact(
            current.mapping(),
            current.marker_effect_continuation().active(),
            current.frontier(),
            current.working_roots(),
            Some(left.current_map.measure().source),
        )
    {
        return false;
    }
    if matches!(
        current.lifecycle(),
        DraftPieceBuildLifecycleV1::Cancelled
            | DraftPieceBuildLifecycleV1::Rejected
            | DraftPieceBuildLifecycleV1::Error
    ) {
        return left == right
            && control_frozen(previous, current)
            && previous.writer_admission() == current.writer_admission()
            && previous.fragment_endpoint() == current.fragment_endpoint()
            && previous.durable_continuation() == current.durable_continuation()
            && previous.successor() == current.successor()
            && previous.build_digest() == current.build_digest();
    }
    if !matches!(left.mapping_stage, Stage::PublishReady { .. }) {
        let before = previous.marker_effect_continuation();
        let after = current.marker_effect_continuation();
        if before.scan() != after.scan()
            || before.source_logical_frontier() != after.source_logical_frontier()
            || before.successor_logical_frontier() != after.successor_logical_frontier()
        {
            return false;
        }
    }
    if left.mapping_stage == Stage::Idle && right.mapping_stage == Stage::Idle {
        return publication::continuation(previous, current, left, right, fragment)
            || left == right && idle_transition(previous, current, fragment);
    }
    if previous.fragment_endpoint() != current.fragment_endpoint()
        || previous.durable_continuation() != current.durable_continuation()
        || previous.successor() != current.successor()
        || previous.build_digest() != current.build_digest()
    {
        return false;
    }
    if !matches!(left.mapping_stage, Stage::PublishReady { .. }) {
        if left.completed_source_unit != right.completed_source_unit
            || previous.writer_admission() != current.writer_admission()
        {
            return false;
        }
    }
    query::transition(previous, current, left, right, fragment)
        || splice::transition(previous, current, left, right, fragment)
        || publication::transition(previous, current, left, right, fragment)
}

fn idle_transition(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> bool {
    if previous.working_roots() != current.working_roots()
        || previous.base_frontier() != current.base_frontier()
        || previous.successor_frontier() != current.successor_frontier()
        || previous.next_record_ordinal() != current.next_record_ordinal()
        || previous.writer_admission() != current.writer_admission()
    {
        return false;
    }
    match (previous.frontier(), current.frontier()) {
        (Frontier::Receiving { .. }, Frontier::Receiving { .. } | Frontier::Planning { .. }) => {
            previous.marker_effect_continuation() == current.marker_effect_continuation()
        }
        (Frontier::Removing { .. }, Frontier::Applying { .. })
        | (Frontier::Applying { .. }, Frontier::Inserting { .. }) => {
            previous.fragment_endpoint() == current.fragment_endpoint()
                && previous.durable_continuation() == current.durable_continuation()
                && super::sequence_progress::transition_is_exact(previous, current)
                && super::marker_progress::transition_is_exact(previous, current, fragment)
        }
        (Frontier::Inserting { .. }, Frontier::Inserting { .. }) => {
            previous.marker_effect_continuation().active().is_some()
                && previous.fragment_endpoint() == current.fragment_endpoint()
                && previous.durable_continuation() == current.durable_continuation()
                && super::marker_progress::transition_is_exact(previous, current, fragment)
        }
        (Frontier::CrossValidating, Frontier::Complete)
        | (Frontier::Complete, Frontier::Complete) => {
            previous.fragment_endpoint() == current.fragment_endpoint()
                && previous.durable_continuation() == current.durable_continuation()
                && previous.marker_effect_continuation() == current.marker_effect_continuation()
        }
        _ => false,
    }
}
