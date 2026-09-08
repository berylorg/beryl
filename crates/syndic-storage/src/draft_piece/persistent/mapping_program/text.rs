use super::super::marker_program::position::{PositionProof, resolve};
use super::text_splice::{apply_ready, deletion_proof, insertion_chunk};
use super::*;

fn primary() -> DraftPieceMappingProofComponentV1 {
    DraftPieceMappingProofComponentV1 {
        component: DraftPieceMarkerProofComponentV1::Primary,
        primary_marker_rank: None,
    }
}

fn secondary(rank: u64) -> DraftPieceMappingProofComponentV1 {
    DraftPieceMappingProofComponentV1 {
        component: DraftPieceMarkerProofComponentV1::Secondary,
        primary_marker_rank: Some(rank),
    }
}

fn source_proof(
    context: &mut BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    position: DraftCompositePositionV1,
    proof: DraftPieceMappingProofComponentV1,
) -> Result<PositionProof, DraftPiecePrepareErrorV1> {
    let sequence = load_root(context, build.predecessor_root())?;
    resolve(
        context,
        sequence,
        position,
        proof.component,
        proof.primary_marker_rank,
    )
}

fn previous(
    context: &BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
) -> Result<DraftPieceBuildFragmentV1, DraftPiecePrepareErrorV1> {
    marker_program::previous_fragment(context, build, fragment)
}

pub(super) fn advance(
    mut context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    if build.frontier() == DraftPieceBuildFrontierV1::CrossValidating {
        if mapping(&context)?.mapping_stage != Stage::Idle
            || mapping(&context)?.fragment_source_end_unit.is_some()
        {
            return invalid();
        }
        let (sequence, index, order) = load_working_roots(&mut context, build.working_roots())?;
        if sequence.map_or(0, |t| t.link.marker_count())
            != index.map_or(0, |t| t.link.record_count())
            || sequence.map_or(0, |t| t.link.marker_count())
                != order.map_or(0, |t| t.link.marker_count())
        {
            return invalid();
        }
        resolve_position(&mut context, sequence, build.caret())?;
        resolve_position(&mut context, sequence, build.selection())?;
        let root = finalize_build_root(&mut context, build.operation_id(), sequence, index, order)?;
        let digest = draft_piece_build_digest_v1(build.proposal_digest(), root.reference());
        return Ok(finish_quantum(
            context,
            DraftPieceBuildRootsV1::from_root(root.reference()),
            build.base_frontier(),
            build.successor_frontier(),
            DraftPieceBuildFrontierV1::Complete,
            Some(root),
            Some(digest),
        ));
    }
    let fragment = fragment.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if fragment.key()
        != DraftPieceBuildFragmentKeyV1::new(
            build.draft_id(),
            build.session_id(),
            build.operation_id(),
            fragment.key().ordinal(),
        )
        || fragment.replacement().marker_effect().is_some()
    {
        return invalid();
    }
    let state = mapping(&context)?;
    let mut frontier = build.frontier();
    let next = match state.mapping_stage {
        Stage::Idle => match frontier {
            DraftPieceBuildFrontierV1::Planning { fragment_ordinal }
                if fragment_ordinal == fragment.key().ordinal() =>
            {
                if fragment.replacement().is_continuation() {
                    if fragment_ordinal == 1 || fragment.replacement().inserted().is_empty() {
                        return invalid();
                    }
                    let previous = previous(&context, build, fragment)?;
                    if previous.replacement().start() != fragment.replacement().start()
                        || previous.replacement().end() != fragment.replacement().end()
                    {
                        return Err(DraftPiecePrepareErrorV1::Rejected(
                            DraftPieceRejectedReasonV1::OutOfOrder,
                        ));
                    }
                    context
                        .mapping
                        .as_mut()
                        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                        .fragment_source_end_unit = Some(state.completed_source_unit);
                    frontier = DraftPieceBuildFrontierV1::Inserting {
                        fragment_ordinal,
                        next_piece: 0,
                        next_byte: 0,
                        base_end: build.base_frontier(),
                        successor_end: build.successor_frontier(),
                    };
                    Stage::Idle
                } else {
                    Stage::TextSourceStart { proof: primary() }
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
                if next_rank != end_rank || next_rank != base_end.rank() || removed_markers != 0 {
                    return invalid();
                }
                frontier = DraftPieceBuildFrontierV1::Applying {
                    fragment_ordinal,
                    base_end,
                    successor_start,
                    successor_end,
                };
                Stage::Idle
            }
            DraftPieceBuildFrontierV1::Applying {
                fragment_ordinal,
                base_end,
                successor_start,
                successor_end,
            } => {
                if successor_start == successor_end {
                    frontier = DraftPieceBuildFrontierV1::Inserting {
                        fragment_ordinal,
                        next_piece: 0,
                        next_byte: 0,
                        base_end,
                        successor_end: successor_start,
                    };
                    Stage::Idle
                } else {
                    Stage::TextDeleteProof
                }
            }
            DraftPieceBuildFrontierV1::Inserting {
                next_piece,
                next_byte,
                ..
            } => {
                if next_piece == fragment.replacement().inserted().len() as u64 && next_byte == 0 {
                    Stage::RefreshMap
                } else if next_piece < fragment.replacement().inserted().len() as u64 {
                    Stage::TextInsertProof
                } else {
                    return invalid();
                }
            }
            _ => return invalid(),
        },
        Stage::TextSourceStart { proof } => {
            match source_proof(&mut context, build, fragment.replacement().start(), proof)? {
                PositionProof::Primary(rank) => Stage::TextSourceStart {
                    proof: secondary(rank),
                },
                PositionProof::Complete(boundary, unit) => Stage::TextSourceEnd {
                    start: DraftPieceMappingSourceFactV1 {
                        boundary: durable_boundary(boundary),
                        unit,
                    },
                    proof: primary(),
                },
            }
        }
        Stage::TextSourceEnd { start, proof } => {
            match source_proof(&mut context, build, fragment.replacement().end(), proof)? {
                PositionProof::Primary(rank) => Stage::TextSourceEnd {
                    start,
                    proof: secondary(rank),
                },
                PositionProof::Complete(boundary, unit) => {
                    let end = DraftPieceMappingSourceFactV1 {
                        boundary: durable_boundary(boundary),
                        unit,
                    };
                    if start.unit > end.unit {
                        return Err(DraftPiecePrepareErrorV1::Rejected(
                            DraftPieceRejectedReasonV1::OutOfOrder,
                        ));
                    }
                    if start.unit < state.completed_source_unit {
                        return Err(DraftPiecePrepareErrorV1::Rejected(
                            DraftPieceRejectedReasonV1::Overlap,
                        ));
                    }
                    context
                        .mapping
                        .as_mut()
                        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                        .fragment_source_end_unit = Some(unit);
                    if fragment.key().ordinal() > 1
                        && start.unit == end.unit
                        && start.unit == state.completed_source_unit
                    {
                        Stage::TextPreviousStart {
                            start,
                            end,
                            proof: primary(),
                        }
                    } else {
                        Stage::TextMapStart { start, end }
                    }
                }
            }
        }
        Stage::TextPreviousStart { start, end, proof } => {
            let previous = previous(&context, build, fragment)?;
            match source_proof(&mut context, build, previous.replacement().start(), proof)? {
                PositionProof::Primary(rank) => Stage::TextPreviousStart {
                    start,
                    end,
                    proof: secondary(rank),
                },
                PositionProof::Complete(boundary, _) => Stage::TextPreviousEnd {
                    start,
                    end,
                    previous_start: durable_boundary(boundary),
                    proof: primary(),
                },
            }
        }
        Stage::TextPreviousEnd {
            start,
            end,
            previous_start,
            proof,
        } => {
            let previous = previous(&context, build, fragment)?;
            match source_proof(&mut context, build, previous.replacement().end(), proof)? {
                PositionProof::Primary(rank) => Stage::TextPreviousEnd {
                    start,
                    end,
                    previous_start,
                    proof: secondary(rank),
                },
                PositionProof::Complete(boundary, _) => {
                    if durable_boundary(boundary) == previous_start {
                        return Err(DraftPiecePrepareErrorV1::Rejected(
                            DraftPieceRejectedReasonV1::DuplicateEmptyRange,
                        ));
                    }
                    Stage::TextMapStart { start, end }
                }
            }
        }
        Stage::TextMapStart { start, end } => Stage::TextMapEnd {
            source_start: start.boundary,
            source_end: end.boundary,
            mapped_start: source_cut(&mut context, start.unit, false)?,
        },
        Stage::TextMapEnd {
            source_start,
            source_end,
            mapped_start,
        } => Stage::TextResolveStart {
            source_start,
            source_end,
            mapped_start,
            mapped_end: source_cut(
                &mut context,
                state
                    .fragment_source_end_unit
                    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                false,
            )?,
        },
        Stage::TextResolveStart {
            source_start,
            source_end,
            mapped_start,
            mapped_end,
        } => {
            let (sequence, _, _) = load_working_roots(&mut context, build.working_roots())?;
            let fact = sequence_units::at_unit(&mut context, sequence, mapped_start)?;
            Stage::TextResolveEnd {
                source_start,
                source_end,
                mapped_end,
                start_boundary: durable_boundary(fact.boundary),
                start_marker_ordinal: fact.marker_ordinal,
            }
        }
        Stage::TextResolveEnd {
            source_start: _,
            source_end,
            mapped_end,
            start_boundary,
            start_marker_ordinal,
        } => {
            let (sequence, _, _) = load_working_roots(&mut context, build.working_roots())?;
            let fact = sequence_units::at_unit(&mut context, sequence, mapped_end)?;
            if fact.marker_ordinal != start_marker_ordinal
                || checked_boundary(start_boundary)? > fact.boundary
            {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::Overlap,
                ));
            }
            frontier = DraftPieceBuildFrontierV1::Removing {
                fragment_ordinal: fragment.key().ordinal(),
                next_rank: source_end.rank(),
                end_rank: source_end.rank(),
                removed_markers: 0,
                base_end: source_end,
                successor_start: start_boundary,
                successor_end: durable_boundary(fact.boundary),
            };
            Stage::Idle
        }
        Stage::TextDeleteProof => {
            let DraftPieceBuildFrontierV1::Applying {
                successor_start,
                successor_end,
                ..
            } = frontier
            else {
                return invalid();
            };
            let (sequence, _, _) = load_working_roots(&mut context, build.working_roots())?;
            let splice = deletion_proof(
                &mut context,
                sequence,
                checked_boundary(successor_start)?,
                checked_boundary(successor_end)?,
            )?;
            begin_delete(&mut context, splice)?;
            return Ok(finish(context, build, frontier));
        }
        Stage::TextInsertProof => {
            let DraftPieceBuildFrontierV1::Inserting {
                next_piece,
                next_byte,
                successor_end,
                ..
            } = frontier
            else {
                return invalid();
            };
            let (sequence, _, _) = load_working_roots(&mut context, build.working_roots())?;
            let fact = sequence_units::at_boundary(
                &mut context,
                sequence,
                checked_boundary(successor_end)?,
            )?;
            let (_, chunk) = insertion_chunk(fragment, next_piece, next_byte)?;
            Stage::InsertMap {
                splice: DraftPieceMappingSpliceV1 {
                    kind: Kind::TextInsert,
                    a: fact.unit,
                    removed: 0,
                    inserted: chunk.len() as u128,
                    leaf: None,
                    rank: successor_end.rank(),
                    local_start: successor_end.inner(),
                    local_end: 0,
                },
            }
        }
        Stage::Ready(ready) => return apply_ready(context, build, fragment, ready),
        Stage::PublishReady {
            successor_boundary,
            logical_offset,
        } => return publish(context, build, fragment, successor_boundary, logical_offset),
        _ => return invalid(),
    };
    set_stage(&mut context, next)?;
    Ok(finish(context, build, frontier))
}

fn publish(
    mut context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
    successor_boundary: DraftPieceBuildBoundaryV1,
    logical_offset: u64,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    let DraftPieceBuildFrontierV1::Inserting {
        next_piece,
        next_byte,
        base_end,
        ..
    } = build.frontier()
    else {
        return invalid();
    };
    if next_piece != fragment.replacement().inserted().len() as u64 || next_byte != 0 {
        return invalid();
    }
    let next = fragment
        .key()
        .ordinal()
        .checked_add(1)
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let frontier = if fragment.key().ordinal() < build.fragment_count() {
        DraftPieceBuildFrontierV1::Planning {
            fragment_ordinal: next,
        }
    } else {
        DraftPieceBuildFrontierV1::CrossValidating
    };
    publish_mapping(&mut context)?;
    let mut quantum = finish_quantum(
        context,
        build.working_roots(),
        base_end,
        successor_boundary,
        frontier,
        None,
        None,
    );
    let old = build.marker_effect_continuation();
    quantum.marker_effect_continuation = Some(DraftPieceMarkerEffectContinuationV1::new(
        fragment.replacement().end().utf8_offset(),
        logical_offset,
        DraftPieceMarkerEffectScanFrontierV1::new(
            next,
            Some(canonical_fragment_endpoint(fragment)),
            old.scan().completed_effect_count(),
            old.scan().effect_chain(),
        ),
        None,
    ));
    Ok(quantum)
}
