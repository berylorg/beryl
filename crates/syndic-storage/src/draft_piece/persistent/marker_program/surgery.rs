use super::*;

fn sequence_descriptor(
    context: &mut BuildContext<'_>,
    sequence: Option<SequenceRef>,
) -> Result<DraftPieceSequenceDescriptorV1, DraftPiecePrepareErrorV1> {
    let sequence = compress_sequence_root(context, sequence)?;
    let sequence = match sequence {
        Some(sequence) if sequence.height == 0 => {
            Some(context.new_sequence_node(1, vec![sequence.link])?)
        }
        value => value,
    };
    let Some(sequence) = sequence else {
        return Ok(DraftPieceSequenceDescriptorV1 {
            root_node_id: None,
            summary: DraftPieceSummaryV1::new(
                0,
                0,
                0,
                0,
                0,
                canonical_empty_marker_digest_v1(),
                0,
                canonical_empty_root_digest_v1(),
            ),
        });
    };
    let provisional = DraftPieceSummaryV1::new(
        sequence.link.logical_utf8_bytes(),
        sequence.link.newline_count(),
        sequence.link.logical_line_count(),
        sequence.link.piece_count(),
        sequence.link.marker_count(),
        sequence.link.marker_digest(),
        sequence.height,
        DraftPieceDigestV1::from_bytes([0; 32]),
    );
    Ok(DraftPieceSequenceDescriptorV1 {
        root_node_id: Some(sequence.link.id()),
        summary: DraftPieceSummaryV1::new(
            provisional.logical_utf8_bytes(),
            provisional.newline_count(),
            provisional.logical_line_count(),
            provisional.piece_count(),
            provisional.marker_count(),
            provisional.marker_digest(),
            provisional.height(),
            root_digest(provisional, sequence.link.digest()),
        ),
    })
}

fn identity_descriptor(index: Option<IndexRef>) -> DraftPieceIdentityDescriptorV1 {
    match index {
        Some(index) => DraftPieceIdentityDescriptorV1 {
            root_node_id: Some(index.link.id()),
            summary: DraftMarkerIdentityIndexSummaryV1::new(
                index.link.record_count(),
                index.height,
                index_root_digest(index.link.record_count(), index.height, index.link.digest()),
            ),
        },
        None => DraftPieceIdentityDescriptorV1 {
            root_node_id: None,
            summary: DraftMarkerIdentityIndexSummaryV1::new(
                0,
                0,
                canonical_empty_marker_identity_index_digest_v1(),
            ),
        },
    }
}

fn target_roots(
    context: &mut BuildContext<'_>,
    sequence: DraftPieceSequenceDescriptorV1,
    identity: DraftPieceIdentityDescriptorV1,
    order: Option<MarkerOrderRef>,
) -> Result<DraftPieceBuildRootsV1, DraftPiecePrepareErrorV1> {
    let order = compress_marker_order_root(context, order)?;
    let (order_id, height, commitment) = match order {
        Some(order) => (
            Some(order.link.id()),
            order.height,
            DraftMarkerCommitmentV1::new(
                *order.link.digest().as_bytes(),
                order.link.marker_count(),
                order.link.maximum_image_label(),
            )
            .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?,
        ),
        None => (None, 0, canonical_empty_draft_marker_commitment_v1()),
    };
    let roots = DraftPieceBuildRootsV1::new(
        sequence.root_node_id,
        sequence.summary,
        identity.root_node_id,
        identity.summary,
        order_id,
        height,
        commitment,
    );
    load_working_roots(context, roots)?;
    Ok(roots)
}

fn validate_delta(
    before: DraftPieceBuildRootsV1,
    after: DraftPieceBuildRootsV1,
    inserted: bool,
    split: bool,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let old = before.sequence_summary();
    let new = after.sequence_summary();
    let expected_markers = if inserted {
        old.marker_count().checked_add(1)
    } else {
        old.marker_count().checked_sub(1)
    };
    let expected_pieces = if inserted {
        old.piece_count().checked_add(1 + u64::from(split))
    } else {
        old.piece_count().checked_sub(1)
    };
    if new.logical_utf8_bytes() != old.logical_utf8_bytes()
        || new.newline_count() != old.newline_count()
        || new.logical_line_count() != old.logical_line_count()
        || Some(new.marker_count()) != expected_markers
        || Some(new.piece_count()) != expected_pieces
        || Some(after.marker_index_summary().record_count()) != expected_markers
        || Some(after.marker_commitment().marker_count()) != expected_markers
    {
        return invalid();
    }
    Ok(())
}

pub(super) fn advance(
    context: &mut BuildContext<'_>,
    active: DraftPieceActiveMarkerEffectV1,
) -> Result<DraftPieceActiveMarkerEffectV1, DraftPiecePrepareErrorV1> {
    let (sequence, identity, order) = load_working_roots(context, active.working_roots())?;
    match active.pending() {
        Pending::RemoveSequence => {
            let expected = removal(active.effect())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                .occurrence();
            let site = active
                .removal_site()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let result = sequence_edit::remove_marker(
                context,
                sequence.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                site.piece_rank,
                expected,
            )?;
            let sequence_target = sequence_descriptor(context, result)?;
            Ok(with_pending(
                active,
                Pending::RemoveIdentity { sequence_target },
            ))
        }
        Pending::RemoveIdentity { sequence_target } => {
            let expected = removal(active.effect())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                .occurrence();
            let identity_target = identity_descriptor(index_delete(context, identity, expected)?);
            Ok(with_pending(
                active,
                Pending::RemoveOrder {
                    sequence_target,
                    identity_target,
                },
            ))
        }
        Pending::RemoveOrder {
            sequence_target,
            identity_target,
        } => {
            let expected = removal(active.effect())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                .occurrence();
            let site = active
                .removal_site()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let order = marker_order_delete(
                context,
                order,
                site.marker_ordinal,
                (expected.marker_id(), expected.label(), expected.asset_id()),
            )?;
            let roots = target_roots(context, sequence_target, identity_target, order)?;
            validate_delta(active.working_roots(), roots, false, false)?;
            mapping::install_map(context, roots, false)?;
            Ok(update(
                active,
                roots,
                active.phase(),
                None,
                active.planning(),
                None,
                Pending::None,
            ))
        }
        Pending::InsertSequence => {
            let insertion = insertion(active.effect())?;
            let marker = insertion.marker();
            let site = active
                .insertion_site()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let leaf = context.new_sequence_leaf(DraftPieceLeafValueV1::Marker(marker))?;
            validate_marker_effect_charge(
                active.effect(),
                context
                    .sequence_leaves
                    .get(&leaf.link.id())
                    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
            )?;
            let result =
                insert_sequence_leaf(context, sequence, checked_boundary(site.boundary)?, leaf)?;
            let sequence_target = sequence_descriptor(context, Some(result))?;
            Ok(with_pending(
                active,
                Pending::InsertIdentity {
                    sequence_target,
                    new_leaf_id: leaf.link.id(),
                    new_leaf_digest: leaf.link.digest(),
                },
            ))
        }
        Pending::InsertIdentity {
            sequence_target,
            new_leaf_id,
            new_leaf_digest,
        } => {
            let marker = insertion(active.effect())?.marker();
            let occurrence = DraftMarkerIdentityOccurrenceV1::new(
                marker.marker_id(),
                marker.label(),
                marker.asset_id(),
                marker.order_key(),
                new_leaf_id,
                new_leaf_digest,
            );
            let identity_target = identity_descriptor(index_insert(context, identity, occurrence)?);
            Ok(with_pending(
                active,
                Pending::InsertOrder {
                    sequence_target,
                    identity_target,
                    new_leaf_id,
                    new_leaf_digest,
                },
            ))
        }
        Pending::InsertOrder {
            sequence_target,
            identity_target,
            new_leaf_id: _,
            new_leaf_digest: _,
        } => {
            let marker = insertion(active.effect())?.marker();
            let site = active
                .insertion_site()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let order = marker_order_insert(
                context,
                order,
                site.marker_ordinal,
                marker.marker_id(),
                marker.label(),
                marker.asset_id(),
            )?;
            let roots = target_roots(context, sequence_target, identity_target, order)?;
            validate_delta(
                active.working_roots(),
                roots,
                true,
                site.boundary.inner() != 0,
            )?;
            if roots.marker_commitment().maximum_image_label()
                != active
                    .working_roots()
                    .marker_commitment()
                    .maximum_image_label()
                    .into_iter()
                    .chain(Some(marker.label()))
                    .max()
            {
                return invalid();
            }
            mapping::install_map(context, roots, true)?;
            Ok(update(
                active,
                roots,
                DraftPieceActiveMarkerPhaseV1::Publishing,
                None,
                None,
                None,
                Pending::None,
            ))
        }
        _ => invalid(),
    }
}
