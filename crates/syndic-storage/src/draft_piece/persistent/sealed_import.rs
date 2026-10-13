use super::*;

pub(crate) fn append_import_leaf(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftPieceBuildRecordV1,
    frontier: DraftSealedImportFrontierV1,
    leaf: Option<DraftPieceLeafValueV1>,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    let mut context = BuildContext::with_ordinal(
        storage,
        store,
        build.draft_id(),
        Some(build.session_id()),
        build.operation_id(),
        build.next_record_ordinal(),
    );
    let (mut sequence, mut index, mut order) =
        load_working_roots(&mut context, build.working_roots())?;
    if let Some(value) = leaf {
        let marker = match &value {
            DraftPieceLeafValueV1::Marker(v) => Some(*v),
            _ => None,
        };
        let inserted = context.new_sequence_leaf(value)?;
        sequence = Some(append_sequence(&mut context, sequence, inserted)?);
        if let Some(marker) = marker {
            index = index_insert(
                &mut context,
                index,
                DraftMarkerIdentityOccurrenceV1::new(
                    marker.marker_id(),
                    marker.label(),
                    marker.asset_id(),
                    marker.order_key(),
                    inserted.link.id(),
                    inserted.link.digest(),
                ),
            )?;
            order = marker_order_insert(
                &mut context,
                order,
                build.working_roots().sequence_summary().marker_count(),
                marker.marker_id(),
                marker.label(),
                marker.asset_id(),
            )?;
        }
    }
    let roots = build_roots(&mut context, sequence, index, order)?;
    let end = DraftPieceBuildBoundaryV1::new(roots.sequence_summary().piece_count(), 0);
    Ok(finish_quantum(
        context,
        roots,
        DraftPieceBuildBoundaryV1::new(0, 0),
        end,
        DraftPieceBuildFrontierV1::SealedImport(frontier),
        None,
        None,
    ))
}

fn append_sequence(
    context: &mut BuildContext<'_>,
    root: Option<SequenceRef>,
    leaf: SequenceRef,
) -> Result<SequenceRef, DraftPiecePrepareErrorV1> {
    let Some(root) = root else {
        return Ok(leaf);
    };
    let parts = append_sequence_path(context, root, leaf, 0)?;
    match parts.as_slice() {
        [root] => Ok(*root),
        [left, right] if left.height == right.height => {
            context.new_sequence_node(left.height + 1, vec![left.link, right.link])
        }
        _ => Err(DraftPiecePrepareErrorV1::InvalidRoot),
    }
}

fn append_sequence_path(
    context: &mut BuildContext<'_>,
    root: SequenceRef,
    leaf: SequenceRef,
    depth: u8,
) -> Result<Vec<SequenceRef>, DraftPiecePrepareErrorV1> {
    if depth > DRAFT_PIECE_MAX_HEIGHT {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    if root.height == 0 {
        return Ok(vec![root, leaf]);
    }
    let node = context.load_sequence_node(root.link, root.height, root.selected_root)?;
    let mut children = node.children().to_vec();
    let last = children
        .pop()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let appended = append_sequence_path(
        context,
        SequenceRef {
            link: last,
            height: root.height - 1,
            selected_root: false,
        },
        leaf,
        depth + 1,
    )?;
    children.extend(appended.into_iter().map(|p| p.link));
    pack_inserted_sequence_children(context, root.height, children)
}

pub(crate) fn cross_validate_import_leaf(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftPieceBuildRecordV1,
    mut frontier: DraftSealedImportFrontierV1,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    let mut context = BuildContext::with_ordinal(
        storage,
        store,
        build.draft_id(),
        Some(build.session_id()),
        build.operation_id(),
        build.next_record_ordinal(),
    );
    let (sequence, index, order) = load_working_roots(&mut context, build.working_roots())?;
    let count = build.working_roots().sequence_summary().piece_count();
    let mut root = None;
    let mut digest = None;
    if frontier.next_leaf_rank < count {
        let cursor = piece_cursor(
            &mut context,
            sequence.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
            frontier.next_leaf_rank,
        )?;
        let leaf = context.load_sequence_leaf(cursor.leaf.link)?;
        if let DraftPieceLeafValueV1::Marker(marker) = leaf.value() {
            let expected = DraftMarkerIdentityOccurrenceV1::new(
                marker.marker_id(),
                marker.label(),
                marker.asset_id(),
                marker.order_key(),
                leaf.key().id(),
                leaf.digest(),
            );
            if index_lookup(&mut context, index, marker.marker_id())? != Some(expected)
                || order_at_rank(
                    &mut context,
                    order.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                    frontier.checked_markers,
                )? != (marker.marker_id(), marker.label(), marker.asset_id())
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            frontier.checked_markers = frontier
                .checked_markers
                .checked_add(1)
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            frontier.sequential_digest = beryl_model::advance_sequential_marker_digest(
                frontier.sequential_digest,
                marker.marker_id(),
                marker.label(),
            );
            frontier.ordered_digest = beryl_model::advance_ordered_marker_asset_digest(
                frontier.ordered_digest,
                marker.marker_id(),
                marker.label(),
                marker.asset_id(),
            );
            frontier.maximum_label = Some(
                frontier
                    .maximum_label
                    .map_or(marker.label(), |old| old.max(marker.label())),
            );
        }
        frontier.next_leaf_rank = frontier
            .next_leaf_rank
            .checked_add(1)
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    } else if frontier.next_leaf_rank == count && frontier.checked_markers == frontier.markers {
        let summary = frontier.basis.content.summary();
        if frontier.sequential_digest != summary.marker_digest()
            || frontier.maximum_label != summary.maximum_image_label()
            || frontier.basis.assets.is_some_and(|p| {
                p.ordered_assets().marker_asset_digest() != frontier.ordered_digest
            })
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        resolve_position(&mut context, sequence, build.caret())?;
        resolve_position(&mut context, sequence, build.selection())?;
        let completed =
            finalize_build_root(&mut context, build.operation_id(), sequence, index, order)?;
        digest = Some(draft_piece_build_digest_v1(
            build.proposal_digest(),
            completed.reference(),
        ));
        root = Some(completed);
        frontier.phase = DraftSealedImportPhaseV1::Complete;
    } else {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(finish_quantum(
        context,
        build.working_roots(),
        build.base_frontier(),
        build.successor_frontier(),
        DraftPieceBuildFrontierV1::SealedImport(frontier),
        root,
        digest,
    ))
}

fn order_at_rank(
    context: &mut BuildContext<'_>,
    mut root: MarkerOrderRef,
    mut rank: u64,
) -> Result<(SyndicDraftMarkerId, ImageLabelOrdinal, AssetId), DraftPiecePrepareErrorV1> {
    if rank >= root.link.marker_count() {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    loop {
        let record = context.load_marker_order_record(root, root.selected_root)?;
        match record {
            DraftMarkerOrderRecordV1::Leaf {
                marker_id,
                label,
                asset_id,
                ..
            } if rank == 0 => return Ok((marker_id, label, asset_id)),
            DraftMarkerOrderRecordV1::Internal {
                height, children, ..
            } => {
                let mut selected = None;
                for child in children {
                    if rank < child.marker_count() {
                        selected = Some(child);
                        break;
                    }
                    rank -= child.marker_count();
                }
                root = MarkerOrderRef {
                    link: selected.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                    height: height - 1,
                    selected_root: false,
                };
            }
            _ => return Err(DraftPiecePrepareErrorV1::InvalidRoot),
        }
    }
}
