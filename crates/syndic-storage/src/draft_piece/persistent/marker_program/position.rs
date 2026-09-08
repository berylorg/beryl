use super::*;

pub(super) struct Fact {
    pub(super) located: LocatedLeaf,
    pub(super) marker_ordinal: u64,
    pub(super) leaf: DraftPieceLeafRecordV1,
}

pub(super) fn locate(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    target: DraftCompositeSearchKeyV1,
    insertion_order: bool,
) -> Result<Option<Fact>, DraftPiecePrepareErrorV1> {
    let mut current = tree;
    let (mut anchor, mut rank, mut ordinal) = (0_u64, 0_u64, 0_u64);
    while current.height != 0 {
        let node =
            context.load_sequence_node(current.link, current.height, current.selected_root)?;
        let (mut child_anchor, mut child_rank, mut child_ordinal) = (anchor, rank, ordinal);
        let (mut selected, mut text_candidate) = (None, None);
        for child in node.children().iter().copied() {
            let first = checked_offset_key(child.first(), child_anchor)
                .map_err(DraftPiecePrepareErrorV1::Rejected)?;
            let last = checked_offset_key(child.last(), child_anchor)
                .map_err(DraftPiecePrepareErrorV1::Rejected)?;
            let child_end = child_anchor
                .checked_add(child.logical_utf8_bytes())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let candidate = (child, child_anchor, child_rank, child_ordinal);
            let accepts = if insertion_order {
                if child.logical_utf8_bytes() != 0
                    && child_anchor <= target.anchor()
                    && target.anchor() <= child_end
                    && (text_candidate.is_none() || child_anchor == target.anchor())
                {
                    text_candidate = Some(candidate);
                }
                child.marker_count() != 0
                    && target <= last
                    && last != DraftCompositeSearchKeyV1::AfterMarkers(target.anchor())
            } else {
                match target {
                    DraftCompositeSearchKeyV1::Marker { .. } => {
                        child.marker_count() != 0
                            && first <= target
                            && target <= last
                            && last != DraftCompositeSearchKeyV1::AfterMarkers(target.anchor())
                    }
                    DraftCompositeSearchKeyV1::BeforeMarkers(offset) => {
                        last > target && last != DraftCompositeSearchKeyV1::AfterMarkers(offset)
                    }
                    DraftCompositeSearchKeyV1::AfterMarkers(_) => last > target,
                }
            };
            if accepts {
                selected = Some(candidate);
                break;
            }
            child_anchor = child_end;
            child_rank = child_rank
                .checked_add(child.piece_count())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            child_ordinal = child_ordinal
                .checked_add(child.marker_count())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        }
        let Some((child, next_anchor, next_rank, next_ordinal)) = selected.or(text_candidate)
        else {
            return Ok(None);
        };
        current = SequenceRef {
            link: child,
            height: current.height - 1,
            selected_root: false,
        };
        anchor = next_anchor;
        rank = next_rank;
        ordinal = next_ordinal;
    }
    Ok(Some(Fact {
        located: LocatedLeaf {
            rank,
            anchor,
            link: current.link,
        },
        marker_ordinal: ordinal,
        leaf: context.load_sequence_leaf(current.link)?,
    }))
}

pub(super) fn text_boundary(
    tree: Option<SequenceRef>,
    fact: Option<&Fact>,
    offset: u64,
) -> Result<(Boundary, u64), DraftPiecePrepareErrorV1> {
    match fact {
        None if offset == tree.map_or(0, |tree| tree.link.logical_utf8_bytes()) => Ok((
            Boundary {
                rank: tree.map_or(0, |tree| tree.link.piece_count()),
                inner: 0,
            },
            tree.map_or(0, |tree| tree.link.marker_count()),
        )),
        Some(fact) => {
            let DraftPieceLeafValueV1::Text(text) = fact.leaf.value() else {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::InvalidGapWitness,
                ));
            };
            let inner = offset
                .checked_sub(fact.located.anchor)
                .and_then(|n| usize::try_from(n).ok())
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            if inner > text.len() || !text.is_char_boundary(inner) {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
                ));
            }
            Ok((
                if inner == text.len() {
                    Boundary {
                        rank: increment(fact.located.rank)?,
                        inner: 0,
                    }
                } else {
                    Boundary {
                        rank: fact.located.rank,
                        inner,
                    }
                },
                fact.marker_ordinal,
            ))
        }
        _ => invalid(),
    }
}

pub(super) enum PositionProof {
    Complete(Boundary),
    Primary(u64),
}

pub(super) fn resolve(
    context: &mut BuildContext<'_>,
    tree: Option<SequenceRef>,
    position: DraftCompositePositionV1,
    component: Component,
    primary: Option<u64>,
) -> Result<PositionProof, DraftPiecePrepareErrorV1> {
    if position.utf8_offset() > tree.map_or(0, |tree| tree.link.logical_utf8_bytes())
        || (component == Component::Primary) != primary.is_none()
    {
        return invalid();
    }
    let offset = position.utf8_offset();
    let gap = position.gap();
    let key = match (gap, component) {
        (
            DraftCompositeGapWitnessV1::Unambiguous
            | DraftCompositeGapWitnessV1::BeforeAll
            | DraftCompositeGapWitnessV1::AfterAll,
            Component::Primary,
        ) => DraftCompositeSearchKeyV1::BeforeMarkers(offset),
        (DraftCompositeGapWitnessV1::AfterAll, Component::Secondary) => {
            DraftCompositeSearchKeyV1::AfterMarkers(offset)
        }
        (
            DraftCompositeGapWitnessV1::Between {
                left_order_key,
                left_marker_id,
                ..
            },
            Component::Primary,
        ) => DraftCompositeSearchKeyV1::Marker {
            anchor: offset,
            order_key: left_order_key,
            marker_id: left_marker_id,
        },
        (
            DraftCompositeGapWitnessV1::Between {
                right_order_key,
                right_marker_id,
                ..
            },
            Component::Secondary,
        ) => DraftCompositeSearchKeyV1::Marker {
            anchor: offset,
            order_key: right_order_key,
            marker_id: right_marker_id,
        },
        _ => return invalid(),
    };
    let fact = tree
        .map(|tree| locate(context, tree, key, false))
        .transpose()?
        .flatten();
    match (gap, component) {
        (DraftCompositeGapWitnessV1::Unambiguous, Component::Primary) => Ok(
            PositionProof::Complete(text_boundary(tree, fact.as_ref(), offset)?.0),
        ),
        (
            DraftCompositeGapWitnessV1::BeforeAll | DraftCompositeGapWitnessV1::AfterAll,
            Component::Primary,
        ) => {
            let fact = fact.ok_or(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::InvalidGapWitness,
            ))?;
            if fact.located.anchor != offset
                || !matches!(fact.leaf.value(), DraftPieceLeafValueV1::Marker(_))
            {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::InvalidGapWitness,
                ));
            }
            if gap == DraftCompositeGapWitnessV1::AfterAll {
                Ok(PositionProof::Primary(fact.located.rank))
            } else {
                Ok(PositionProof::Complete(Boundary {
                    rank: fact.located.rank,
                    inner: 0,
                }))
            }
        }
        (DraftCompositeGapWitnessV1::AfterAll, Component::Secondary) => {
            let boundary = text_boundary(tree, fact.as_ref(), offset)?.0;
            if boundary.inner != 0
                || boundary.rank <= primary.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
            {
                return invalid();
            }
            Ok(PositionProof::Complete(boundary))
        }
        (DraftCompositeGapWitnessV1::Between { .. }, _) => {
            let fact = fact.ok_or(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::InvalidGapWitness,
            ))?;
            if !matches!(fact.leaf.value(), DraftPieceLeafValueV1::Marker(marker)
                if DraftCompositeSearchKeyV1::Marker { anchor: fact.located.anchor, order_key: marker.order_key(), marker_id: marker.marker_id() } == key)
            {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::InvalidGapWitness,
                ));
            }
            if component == Component::Primary {
                Ok(PositionProof::Primary(fact.located.rank))
            } else if fact.located.rank
                == increment(primary.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?)?
            {
                Ok(PositionProof::Complete(Boundary {
                    rank: fact.located.rank,
                    inner: 0,
                }))
            } else {
                Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::InvalidGapWitness,
                ))
            }
        }
        _ => invalid(),
    }
}
