use super::*;

pub(super) fn deletion_proof(
    context: &mut BuildContext<'_>,
    sequence: Option<SequenceRef>,
    start: Boundary,
    end: Boundary,
) -> Result<DraftPieceMappingSpliceV1, DraftPiecePrepareErrorV1> {
    if start >= end {
        return invalid();
    }
    let rank = if end.inner > 0 {
        end.rank
    } else {
        end.rank
            .checked_sub(1)
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
    };
    let local_start = if rank == start.rank { start.inner } else { 0 };
    let fact = sequence_units::at_boundary(context, sequence, Boundary { rank, inner: 0 })?;
    let leaf = fact.leaf.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let DraftPieceLeafValueV1::Text(text) = leaf.value() else {
        return invalid();
    };
    let local_end = if end.inner > 0 { end.inner } else { text.len() };
    if local_start >= local_end
        || local_end > text.len()
        || !text.is_char_boundary(local_start)
        || !text.is_char_boundary(local_end)
    {
        return invalid();
    }
    Ok(DraftPieceMappingSpliceV1 {
        kind: Kind::TextDelete,
        a: fact.unit + local_start as u128,
        removed: (local_end - local_start) as u128,
        inserted: 0,
        leaf: Some((leaf.key().id(), leaf.digest())),
        rank,
        local_start: local_start as u64,
        local_end: local_end as u64,
    })
}

pub(super) fn insertion_chunk(
    fragment: &DraftPieceBuildFragmentV1,
    piece: u64,
    byte: u64,
) -> Result<(usize, &str), DraftPiecePrepareErrorV1> {
    let DraftPieceV1::Text(text) = fragment
        .replacement()
        .inserted()
        .get(usize::try_from(piece).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?)
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
    else {
        return invalid();
    };
    let start = usize::try_from(byte).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
    if start >= text.len() || !text.is_char_boundary(start) {
        return invalid();
    }
    let mut end = start
        .saturating_add(DRAFT_PIECE_TEXT_LEAF_MAX_BYTES)
        .min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok((end, &text[start..end]))
}

pub(super) fn apply_ready(
    mut context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
    ready: DraftPieceMappingReadySpliceV1,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    let (sequence, index, order) = load_working_roots(&mut context, build.working_roots())?;
    let (sequence, frontier) = match (ready.kind, build.frontier()) {
        (
            Kind::TextDelete,
            DraftPieceBuildFrontierV1::Applying {
                fragment_ordinal,
                base_end,
                successor_start,
                successor_end,
            },
        ) => {
            let start = checked_boundary(successor_start)?;
            let end = checked_boundary(successor_end)?;
            let (sequence, start, end) = sequence_edit::remove_text_slice(
                &mut context,
                sequence.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                start,
                end,
            )?;
            consume_ready(&mut context, Kind::TextDelete)?;
            if start != end {
                set_stage(&mut context, Stage::TextDeleteProof)?;
            }
            (
                sequence,
                DraftPieceBuildFrontierV1::Applying {
                    fragment_ordinal,
                    base_end,
                    successor_start: durable_boundary(start),
                    successor_end: durable_boundary(end),
                },
            )
        }
        (
            Kind::TextInsert,
            DraftPieceBuildFrontierV1::Inserting {
                fragment_ordinal,
                next_piece,
                next_byte,
                base_end,
                successor_end,
            },
        ) => {
            let boundary = checked_boundary(successor_end)?;
            let (end, chunk) = insertion_chunk(fragment, next_piece, next_byte)?;
            if ready.removed != 0 || ready.inserted != chunk.len() as u128 {
                return invalid();
            }
            let leaf = context.new_sequence_leaf(DraftPieceLeafValueV1::Text(chunk.to_owned()))?;
            let sequence = insert_sequence_leaf(&mut context, sequence, boundary, leaf)?;
            let rank = boundary
                .rank
                .checked_add(u64::from(boundary.inner != 0))
                .and_then(|rank| rank.checked_add(1))
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let DraftPieceV1::Text(text) = &fragment.replacement().inserted()[next_piece as usize]
            else {
                return invalid();
            };
            let (next_piece, next_byte) = if end == text.len() {
                (
                    next_piece
                        .checked_add(1)
                        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                    0,
                )
            } else {
                (next_piece, end as u64)
            };
            consume_ready(&mut context, Kind::TextInsert)?;
            (
                Some(sequence),
                DraftPieceBuildFrontierV1::Inserting {
                    fragment_ordinal,
                    next_piece,
                    next_byte,
                    base_end,
                    successor_end: DraftPieceBuildBoundaryV1::new(rank, 0),
                },
            )
        }
        _ => return invalid(),
    };
    let roots = build_roots(&mut context, sequence, index, order)?;
    if sequence_units::extent(sequence) != mapping(&context)?.current_map.measure().target {
        return invalid();
    }
    Ok(finish_quantum(
        context,
        roots,
        build.base_frontier(),
        build.successor_frontier(),
        frontier,
        None,
        None,
    ))
}
