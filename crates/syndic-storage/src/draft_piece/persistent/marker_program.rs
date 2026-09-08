use super::*;

mod position;
mod proof;
mod surgery;

#[cfg(feature = "test-faults")]
pub(super) fn locate_boundary_for_test(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    target: DraftCompositeSearchKeyV1,
    insertion_order: bool,
) -> Result<Option<(u64, u64, Option<DraftPieceMarkerV1>)>, DraftPiecePrepareErrorV1> {
    position::locate(context, tree, target, insertion_order).map(|fact| {
        fact.map(|fact| {
            let marker = match fact.leaf.value() {
                DraftPieceLeafValueV1::Marker(marker) => Some(*marker),
                DraftPieceLeafValueV1::Text(_) => None,
            };
            (fact.located.rank, fact.marker_ordinal, marker)
        })
    })
}

use DraftPieceMarkerPendingV1 as Pending;
use DraftPieceMarkerProofComponentV1 as Component;
use DraftPieceMarkerProofPurposeV1 as Purpose;

fn invalid<T>() -> Result<T, DraftPiecePrepareErrorV1> {
    Err(DraftPiecePrepareErrorV1::InvalidRoot)
}

fn increment(value: u64) -> Result<u64, DraftPiecePrepareErrorV1> {
    value
        .checked_add(1)
        .ok_or(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::AggregateOverflow,
        ))
}

fn proof(purpose: Purpose) -> Pending {
    Pending::Proof {
        purpose,
        component: Component::Primary,
        primary_marker_rank: None,
    }
}

fn removal(effect: DraftPieceMarkerEffectV1) -> Option<DraftPieceMarkerRemovalProofV1> {
    match effect {
        DraftPieceMarkerEffectV1::Insert(_) => None,
        DraftPieceMarkerEffectV1::Remove { removal, .. }
        | DraftPieceMarkerEffectV1::Move { removal, .. }
        | DraftPieceMarkerEffectV1::SameIdReplacement { removal, .. } => Some(removal),
    }
}

fn insertion(
    effect: DraftPieceMarkerEffectV1,
) -> Result<DraftPieceMarkerInsertionV1, DraftPiecePrepareErrorV1> {
    match effect {
        DraftPieceMarkerEffectV1::Insert(insertion)
        | DraftPieceMarkerEffectV1::Move { insertion, .. }
        | DraftPieceMarkerEffectV1::SameIdReplacement { insertion, .. } => Ok(insertion),
        DraftPieceMarkerEffectV1::Remove { .. } => invalid(),
    }
}

fn update(
    active: DraftPieceActiveMarkerEffectV1,
    roots: DraftPieceBuildRootsV1,
    phase: DraftPieceActiveMarkerPhaseV1,
    removal_site: Option<DraftPieceMarkerRemovalSiteV1>,
    planning: Option<DraftPieceMarkerPlanningV1>,
    insertion_site: Option<DraftPieceMarkerInsertionSiteV1>,
    pending: Pending,
) -> DraftPieceActiveMarkerEffectV1 {
    DraftPieceActiveMarkerEffectV1::new(
        active.fragment_key(),
        active.fragment_digest(),
        active.effect(),
        active.source_roots(),
        roots,
        active.source_frontier(),
        active.successor_frontier(),
        phase,
    )
    .with_program(removal_site, planning, insertion_site, pending)
}

fn with_pending(
    active: DraftPieceActiveMarkerEffectV1,
    pending: Pending,
) -> DraftPieceActiveMarkerEffectV1 {
    update(
        active,
        active.working_roots(),
        active.phase(),
        active.removal_site(),
        active.planning(),
        active.insertion_site(),
        pending,
    )
}

pub(super) fn activate(
    context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    validate_fragment(fragment.replacement()).map_err(DraftPiecePrepareErrorV1::Rejected)?;
    if !matches!(build.frontier(), DraftPieceBuildFrontierV1::Planning { fragment_ordinal } if fragment_ordinal == fragment.key().ordinal())
        || fragment.key()
            != DraftPieceBuildFragmentKeyV1::new(
                build.draft_id(),
                build.session_id(),
                build.operation_id(),
                fragment.key().ordinal(),
            )
        || build.marker_effect_continuation().active().is_some()
        || build
            .marker_effect_continuation()
            .scan()
            .next_fragment_ordinal()
            != fragment.key().ordinal()
        || (build.writer_admission().is_none()
            && !unadmitted_marker_builder_is_authorized_for_test(DraftPieceSettlementKeyV1::new(
                build.draft_id(),
                build.session_id(),
                build.operation_id(),
            )))
    {
        return invalid();
    }
    let continuation = build.marker_effect_continuation();
    let active = DraftPieceActiveMarkerEffectV1::new(
        fragment.key(),
        canonical_fragment_endpoint(fragment).digest(),
        fragment
            .replacement()
            .marker_effect()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
        build.working_roots(),
        build.working_roots(),
        continuation.source_logical_frontier(),
        continuation.successor_logical_frontier(),
        DraftPieceActiveMarkerPhaseV1::Removing,
    )
    .with_program(
        None,
        Some(DraftPieceMarkerPlanningV1 {
            source_boundary: None,
            previous_start: None,
        }),
        None,
        proof(Purpose::SourceBounds),
    );
    Ok(finish(
        context,
        build,
        active,
        build.frontier(),
        build.base_frontier(),
    ))
}

fn finish(
    context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    active: DraftPieceActiveMarkerEffectV1,
    frontier: DraftPieceBuildFrontierV1,
    base_frontier: DraftPieceBuildBoundaryV1,
) -> DraftPieceTreeQuantumV1 {
    let old = build.marker_effect_continuation();
    let mut quantum = finish_quantum(
        context,
        active.source_roots(),
        base_frontier,
        build.successor_frontier(),
        frontier,
        None,
        None,
    );
    quantum.marker_effect_continuation = Some(DraftPieceMarkerEffectContinuationV1::new(
        old.source_logical_frontier(),
        old.successor_logical_frontier(),
        old.scan(),
        Some(active),
    ));
    quantum
}

pub(super) fn advance(
    mut context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    let active = build
        .marker_effect_continuation()
        .active()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if active.fragment_key() != fragment.key()
        || active.fragment_digest() != canonical_fragment_endpoint(fragment).digest()
        || Some(active.effect()) != fragment.replacement().marker_effect()
        || active.source_roots() != build.working_roots()
        || !active.is_program_locally_exact()
        || !build
            .marker_effect_continuation()
            .is_locally_exact(DraftPieceSettlementKeyV1::new(
                build.draft_id(),
                build.session_id(),
                build.operation_id(),
            ))
    {
        return invalid();
    }
    validate_fragment(fragment.replacement()).map_err(DraftPiecePrepareErrorV1::Rejected)?;
    let mut frontier = build.frontier();
    let mut base_frontier = build.base_frontier();
    let active = match frontier {
        DraftPieceBuildFrontierV1::Planning { fragment_ordinal } => {
            if active.phase() != DraftPieceActiveMarkerPhaseV1::Removing {
                return invalid();
            }
            match active.pending() {
                Pending::Proof { .. } => proof::advance(&mut context, build, fragment, active)?,
                Pending::RemoveSequence
                | Pending::RemoveIdentity { .. }
                | Pending::RemoveOrder { .. } => surgery::advance(&mut context, active)?,
                Pending::None => {
                    let source = active
                        .planning()
                        .and_then(|p| p.source_boundary)
                        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
                    let boundary = checked_boundary(source)?;
                    let base_end = match active.effect() {
                        DraftPieceMarkerEffectV1::Remove { .. }
                        | DraftPieceMarkerEffectV1::SameIdReplacement { .. } => {
                            DraftPieceBuildBoundaryV1::new(increment(boundary.rank)?, 0)
                        }
                        _ => source,
                    };
                    let mapped = boundary_after_marker_removal(
                        mapped_boundary(
                            build.base_frontier(),
                            build.successor_frontier(),
                            boundary,
                        )?,
                        active.removal_site().map(|site| site.piece_rank),
                    );
                    frontier = DraftPieceBuildFrontierV1::Removing {
                        fragment_ordinal,
                        next_rank: boundary.rank,
                        end_rank: boundary.rank,
                        removed_markers: 0,
                        base_end,
                        successor_start: durable_boundary(mapped),
                        successor_end: durable_boundary(mapped),
                    };
                    update(
                        active,
                        active.working_roots(),
                        active.phase(),
                        None,
                        None,
                        None,
                        Pending::None,
                    )
                }
                _ => return invalid(),
            }
        }
        DraftPieceBuildFrontierV1::Removing {
            fragment_ordinal,
            next_rank,
            end_rank,
            removed_markers,
            base_end,
            successor_start,
            successor_end,
        } => {
            if next_rank != end_rank
                || removed_markers != 0
                || successor_start != successor_end
                || active.phase() != DraftPieceActiveMarkerPhaseV1::Removing
                || active.pending() != Pending::None
                || active.planning().is_some()
                || active.removal_site().is_some()
                || active.insertion_site().is_some()
            {
                return invalid();
            }
            frontier = DraftPieceBuildFrontierV1::Applying {
                fragment_ordinal,
                base_end,
                successor_start,
                successor_end,
            };
            update(
                active,
                active.working_roots(),
                DraftPieceActiveMarkerPhaseV1::DerivingInsertionGap,
                None,
                None,
                None,
                Pending::None,
            )
        }
        DraftPieceBuildFrontierV1::Applying {
            fragment_ordinal,
            base_end,
            successor_start,
            successor_end,
        } => {
            if successor_start != successor_end
                || active.phase() != DraftPieceActiveMarkerPhaseV1::DerivingInsertionGap
                || active.pending() != Pending::None
                || active.planning().is_some()
                || active.removal_site().is_some()
                || active.insertion_site().is_some()
            {
                return invalid();
            }
            base_frontier = base_end;
            frontier = DraftPieceBuildFrontierV1::Inserting {
                fragment_ordinal,
                next_piece: 0,
                next_byte: 0,
                base_end,
                successor_end,
            };
            if matches!(active.effect(), DraftPieceMarkerEffectV1::Remove { .. }) {
                update(
                    active,
                    active.working_roots(),
                    DraftPieceActiveMarkerPhaseV1::Publishing,
                    None,
                    None,
                    None,
                    Pending::None,
                )
            } else {
                with_pending(active, proof(Purpose::InsertIdentityAbsent))
            }
        }
        DraftPieceBuildFrontierV1::Inserting {
            fragment_ordinal,
            next_piece,
            next_byte,
            base_end,
            successor_end,
        } => {
            if next_byte != 0 {
                return invalid();
            }
            if active.phase() == DraftPieceActiveMarkerPhaseV1::Publishing {
                if next_piece != fragment.replacement().inserted().len() as u64
                    || active.pending() != Pending::None
                    || active.planning().is_some()
                    || active.removal_site().is_some()
                    || active.insertion_site().is_some()
                {
                    return invalid();
                }
                return publish(context, build, fragment, active, base_end, successor_end);
            }
            if next_piece != 0 {
                return invalid();
            }
            match active.pending() {
                Pending::Proof { .. } => proof::advance(&mut context, build, fragment, active)?,
                Pending::InsertSequence
                | Pending::InsertIdentity { .. }
                | Pending::InsertOrder { .. } => {
                    let next = surgery::advance(&mut context, active)?;
                    if next.phase() == DraftPieceActiveMarkerPhaseV1::Publishing {
                        let end = active
                            .insertion_site()
                            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                            .mapped_next_boundary;
                        frontier = DraftPieceBuildFrontierV1::Inserting {
                            fragment_ordinal,
                            next_piece: 1,
                            next_byte: 0,
                            base_end,
                            successor_end: end,
                        };
                    }
                    next
                }
                _ => return invalid(),
            }
        }
        _ => return invalid(),
    };
    Ok(finish(context, build, active, frontier, base_frontier))
}

fn publish(
    context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
    active: DraftPieceActiveMarkerEffectV1,
    base_end: DraftPieceBuildBoundaryV1,
    successor_end: DraftPieceBuildBoundaryV1,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    let old = build.marker_effect_continuation();
    let endpoint = canonical_fragment_endpoint(fragment);
    let count = increment(old.scan().completed_effect_count())?;
    let ordinal = increment(fragment.key().ordinal())?;
    let scan = DraftPieceMarkerEffectScanFrontierV1::new(
        ordinal,
        Some(endpoint),
        count,
        draft_piece_marker_effect_chain_link_v1(
            old.scan().effect_chain(),
            fragment.key(),
            endpoint.digest(),
            count,
            active.working_roots(),
        ),
    );
    let logical_end = fragment
        .replacement()
        .start()
        .utf8_offset()
        .checked_sub(active.source_frontier())
        .and_then(|delta| active.successor_frontier().checked_add(delta))
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let frontier = if fragment.key().ordinal() < build.fragment_count() {
        DraftPieceBuildFrontierV1::Planning {
            fragment_ordinal: ordinal,
        }
    } else {
        DraftPieceBuildFrontierV1::CrossValidating
    };
    let mut quantum = finish_quantum(
        context,
        active.working_roots(),
        base_end,
        successor_end,
        frontier,
        None,
        None,
    );
    quantum.marker_effect_continuation = Some(DraftPieceMarkerEffectContinuationV1::new(
        fragment.replacement().end().utf8_offset(),
        logical_end,
        scan,
        None,
    ));
    Ok(quantum)
}
