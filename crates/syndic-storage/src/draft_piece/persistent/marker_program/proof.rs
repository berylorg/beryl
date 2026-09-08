use super::position::{PositionProof, locate, resolve, text_boundary};
use super::*;

pub(super) fn previous(
    context: &BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
) -> Result<DraftPieceBuildFragmentV1, DraftPiecePrepareErrorV1> {
    let ordinal = fragment
        .key()
        .ordinal()
        .checked_sub(1)
        .filter(|ordinal| *ordinal != 0)
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let key = DraftPieceBuildFragmentKeyV1::new(
        build.draft_id(),
        build.session_id(),
        build.operation_id(),
        ordinal,
    );
    let previous = context
        .point::<DraftPieceBuildFragmentsFamily>(key)?
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if previous.key() != key
        || previous.chain_digest() != fragment.preceding_chain()
        || previous.chain_digest()
            != draft_piece_fragment_chain_link_v1(
                previous.preceding_chain(),
                ordinal,
                previous.replacement(),
            )
        || validate_fragment(previous.replacement()).is_err()
    {
        return invalid();
    }
    Ok(previous)
}

fn after_source(
    build: &DraftPieceBuildRecordV1,
    active: DraftPieceActiveMarkerEffectV1,
) -> Result<Pending, DraftPiecePrepareErrorV1> {
    let boundary = active
        .planning()
        .and_then(|p| p.source_boundary)
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    Ok(
        if active.fragment_key().ordinal() > 1 && boundary == build.base_frontier() {
            proof(Purpose::PreviousStart)
        } else {
            Pending::None
        },
    )
}

fn occurrence(
    context: &mut BuildContext<'_>,
    sequence: Option<SequenceRef>,
    expected: DraftMarkerIdentityOccurrenceV1,
    anchor: u64,
    effect: DraftPieceMarkerEffectV1,
) -> Result<(DraftPieceMarkerRemovalSiteV1, u128), DraftPiecePrepareErrorV1> {
    let tree = sequence.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let fact = locate(
        context,
        tree,
        DraftCompositeSearchKeyV1::Marker {
            anchor,
            order_key: expected.order_key(),
            marker_id: expected.marker_id(),
        },
        false,
    )?
    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if fact.located.anchor != anchor
        || fact.leaf.key().id() != expected.sequence_leaf_id()
        || fact.leaf.digest() != expected.sequence_leaf_digest()
        || !matches!(fact.leaf.value(), DraftPieceLeafValueV1::Marker(marker)
            if marker.marker_id() == expected.marker_id() && marker.order_key() == expected.order_key()
                && marker.label() == expected.label() && marker.asset_id() == expected.asset_id())
    {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::Overlap,
        ));
    }
    validate_marker_effect_charge(effect, &fact.leaf)?;
    Ok((
        DraftPieceMarkerRemovalSiteV1 {
            piece_rank: fact.located.rank,
            marker_ordinal: fact.marker_ordinal,
        },
        u128::from(anchor) + u128::from(fact.marker_ordinal),
    ))
}

pub(super) fn advance(
    context: &mut BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
    active: DraftPieceActiveMarkerEffectV1,
) -> Result<DraftPieceActiveMarkerEffectV1, DraftPiecePrepareErrorV1> {
    let Pending::Proof {
        purpose,
        component,
        primary_marker_rank,
    } = active.pending()
    else {
        return invalid();
    };
    let original = matches!(
        purpose,
        Purpose::SourceBounds
            | Purpose::RemovalGap
            | Purpose::SourceOccurrence
            | Purpose::SourceIdentity
            | Purpose::SourceInsertIdentityAbsent
            | Purpose::PreviousStart
            | Purpose::PreviousEnd
    );
    let (sequence, index) = if original {
        let sequence = load_root(context, build.predecessor_root())?;
        (
            sequence,
            validate_index_root(context, build.predecessor_root())?,
        )
    } else {
        let (sequence, index, _) = load_working_roots(context, active.working_roots())?;
        (sequence, index)
    };
    if matches!(
        purpose,
        Purpose::SourceBounds | Purpose::RemovalGap | Purpose::PreviousStart | Purpose::PreviousEnd
    ) {
        let mut planning = active
            .planning()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        let previous = if matches!(purpose, Purpose::PreviousStart | Purpose::PreviousEnd) {
            Some(previous(context, build, fragment)?)
        } else {
            None
        };
        let position = match purpose {
            Purpose::SourceBounds => fragment.replacement().start(),
            Purpose::RemovalGap => removal(active.effect())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                .position(),
            Purpose::PreviousStart => previous
                .as_ref()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                .replacement()
                .start(),
            Purpose::PreviousEnd => previous
                .as_ref()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                .replacement()
                .end(),
            _ => return invalid(),
        };
        let (boundary, unit) =
            match resolve(context, sequence, position, component, primary_marker_rank)? {
                PositionProof::Primary(rank) => {
                    return Ok(with_pending(
                        active,
                        Pending::Proof {
                            purpose,
                            component: Component::Secondary,
                            primary_marker_rank: Some(rank),
                        },
                    ));
                }
                PositionProof::Complete(boundary, unit) => (durable_boundary(boundary), unit),
            };
        let pending = match purpose {
            Purpose::SourceBounds => {
                if checked_boundary(boundary)? < checked_boundary(build.base_frontier())? {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::Overlap,
                    ));
                }
                planning.source_boundary = Some(boundary);
                context
                    .mapping
                    .as_mut()
                    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                    .fragment_source_end_unit = Some(unit);
                proof(if removal(active.effect()).is_some() {
                    Purpose::RemovalGap
                } else {
                    Purpose::SourceInsertIdentityAbsent
                })
            }
            Purpose::RemovalGap => {
                if !matches!(active.effect(), DraftPieceMarkerEffectV1::Move { .. })
                    && Some(boundary) != planning.source_boundary
                {
                    return invalid();
                }
                proof(Purpose::SourceOccurrence)
            }
            Purpose::PreviousStart => {
                planning.previous_start = Some(boundary);
                proof(Purpose::PreviousEnd)
            }
            Purpose::PreviousEnd => {
                if planning.previous_start == Some(boundary)
                    && previous.as_ref().is_none_or(|previous| {
                        previous.replacement().marker_effect().is_none()
                            || previous.replacement().marker_effect() == Some(active.effect())
                    })
                {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::DuplicateEmptyRange,
                    ));
                }
                planning.previous_start = None;
                Pending::None
            }
            _ => return invalid(),
        };
        return Ok(update(
            active,
            active.working_roots(),
            active.phase(),
            active.removal_site(),
            Some(planning),
            None,
            pending,
        ));
    }
    if component != Component::Primary || primary_marker_rank.is_some() {
        return invalid();
    }
    match purpose {
        Purpose::SourceOccurrence => {
            let removed = removal(active.effect()).ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let anchor = removed.position().utf8_offset();
            let (site, unit) = occurrence(
                context,
                sequence,
                removed.occurrence(),
                anchor,
                active.effect(),
            )?;
            if !matches!(active.effect(), DraftPieceMarkerEffectV1::Move { .. })
                && active.planning().and_then(|p| p.source_boundary)
                    != Some(DraftPieceBuildBoundaryV1::new(site.piece_rank, 0))
            {
                return invalid();
            }
            mapping_program::set_stage(
                context,
                mapping_program::Stage::MarkerSource {
                    removal_source_unit: Some(unit),
                },
            )?;
            Ok(with_pending(active, proof(Purpose::SourceIdentity)))
        }
        Purpose::SourceIdentity => {
            let expected = removal(active.effect())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                .occurrence();
            if index_lookup(context, index, expected.marker_id())? != Some(expected) {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::DuplicateMarkerIdentity,
                ));
            }
            Ok(with_pending(active, after_source(build, active)?))
        }
        Purpose::SourceInsertIdentityAbsent | Purpose::InsertIdentityAbsent => {
            if purpose == Purpose::SourceInsertIdentityAbsent
                && !matches!(active.effect(), DraftPieceMarkerEffectV1::Insert(_))
            {
                return invalid();
            }
            let insertion = insertion(active.effect())?;
            if index_lookup(context, index, insertion.marker().marker_id())?.is_some() {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::DuplicateMarkerIdentity,
                ));
            }
            Ok(with_pending(
                active,
                if purpose == Purpose::SourceInsertIdentityAbsent {
                    after_source(build, active)?
                } else {
                    proof(Purpose::InsertAnchor)
                },
            ))
        }
        Purpose::InsertAnchor | Purpose::InsertOrder | Purpose::InsertAfter => {
            insert_gap(context, build, fragment, active, sequence, purpose)
        }
        _ => invalid(),
    }
}

fn insert_gap(
    context: &mut BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
    active: DraftPieceActiveMarkerEffectV1,
    sequence: Option<SequenceRef>,
    purpose: Purpose,
) -> Result<DraftPieceActiveMarkerEffectV1, DraftPiecePrepareErrorV1> {
    let insertion = insertion(active.effect())?;
    let anchor = insertion.anchor();
    if anchor > sequence.map_or(0, |tree| tree.link.logical_utf8_bytes()) {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::OutOfOrder,
        ));
    }
    let key = match purpose {
        Purpose::InsertAnchor => DraftCompositeSearchKeyV1::BeforeMarkers(anchor),
        Purpose::InsertOrder => DraftCompositeSearchKeyV1::Marker {
            anchor,
            order_key: insertion.marker().order_key(),
            marker_id: SyndicDraftMarkerId::from_bytes([0; 16]),
        },
        Purpose::InsertAfter => DraftCompositeSearchKeyV1::AfterMarkers(anchor),
        _ => return invalid(),
    };
    let fact = sequence
        .map(|tree| locate(context, tree, key, purpose == Purpose::InsertOrder))
        .transpose()?
        .flatten();
    let site = match purpose {
        Purpose::InsertAnchor => {
            if fact.as_ref().is_some_and(|fact| {
                fact.located.anchor == anchor
                    && matches!(fact.leaf.value(), DraftPieceLeafValueV1::Marker(_))
            }) {
                return Ok(with_pending(active, proof(Purpose::InsertOrder)));
            }
            text_boundary(sequence, fact.as_ref(), anchor)?
        }
        Purpose::InsertOrder => {
            if let Some(fact) = fact.as_ref() {
                if let DraftPieceLeafValueV1::Marker(marker) = fact.leaf.value() {
                    if fact.located.anchor == anchor {
                        if marker.order_key() == insertion.marker().order_key() {
                            return Err(DraftPiecePrepareErrorV1::Rejected(
                                DraftPieceRejectedReasonV1::DuplicateMarkerOrder,
                            ));
                        }
                        if marker.order_key() > insertion.marker().order_key() {
                            return complete_gap(
                                context,
                                build,
                                fragment,
                                active,
                                Boundary {
                                    rank: fact.located.rank,
                                    inner: 0,
                                },
                                fact.marker_ordinal,
                            );
                        }
                    }
                }
            }
            return Ok(with_pending(active, proof(Purpose::InsertAfter)));
        }
        Purpose::InsertAfter => text_boundary(sequence, fact.as_ref(), anchor)?,
        _ => return invalid(),
    };
    complete_gap(context, build, fragment, active, site.0, site.1)
}

fn complete_gap(
    context: &mut BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
    active: DraftPieceActiveMarkerEffectV1,
    boundary: Boundary,
    marker_ordinal: u64,
) -> Result<DraftPieceActiveMarkerEffectV1, DraftPiecePrepareErrorV1> {
    let DraftPieceBuildFrontierV1::Inserting { .. } = build.frontier() else {
        return invalid();
    };
    let anchor = insertion(active.effect())?.anchor();
    let frontier = fragment
        .replacement()
        .start()
        .utf8_offset()
        .checked_sub(active.source_frontier())
        .and_then(|delta| active.successor_frontier().checked_add(delta))
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if anchor > frontier {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::OutOfOrder,
        ));
    }
    mapping_program::set_stage(
        context,
        mapping_program::Stage::InsertMap {
            splice: DraftPieceMappingSpliceV1 {
                kind: DraftPieceMappingSpliceKindV1::MarkerInsert,
                a: u128::from(anchor) + u128::from(marker_ordinal),
                removed: 0,
                inserted: 1,
                leaf: None,
                rank: boundary.rank,
                local_start: boundary.inner as u64,
                local_end: 0,
            },
        },
    )?;
    Ok(update(
        active,
        active.working_roots(),
        DraftPieceActiveMarkerPhaseV1::Inserting,
        None,
        None,
        Some(DraftPieceMarkerInsertionSiteV1 {
            boundary: durable_boundary(boundary),
            marker_ordinal,
        }),
        Pending::None,
    ))
}
