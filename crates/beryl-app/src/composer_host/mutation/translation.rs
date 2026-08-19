use super::validation::{prove_destination, validate_object_changes, validate_terminal_positions};
use super::*;

pub(super) fn translate_request(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    request: &ComposerHostMutationRequest,
) -> Result<
    (
        Vec<DraftPieceReplacementV1>,
        MutationPositions,
        Vec<DraftPieceMarkerAtV1>,
        Vec<DraftPieceMarkerAtV1>,
    ),
    ComposerHostError,
> {
    if request.fragments.len() > DRAFT_PIECE_PAGE_MAX_RECORDS.saturating_add(1)
        || request.marker_metadata.len() > DRAFT_PIECE_PAGE_MAX_RECORDS
    {
        return Err(ComposerHostError::MutationMalformed);
    }
    validate_object_changes(request.proposal, &request.fragments)?;
    let start = request.proposal.replacement().start();
    let end = request.proposal.replacement().end();
    let predecessor_start = proven_position(storage, store, root, start)?;
    let predecessor_end = proven_position(storage, store, root, end)?;
    if compare_positions(predecessor_start, predecessor_end)? == Ordering::Greater {
        return Err(ComposerHostError::MutationMalformed);
    }
    let mut metadata = marker_metadata(&request.marker_metadata)?;
    let mut parts = Vec::new();
    let mut moves = Vec::new();
    let mut terminal = None;
    let mut inserted = 0_u64;
    let mut prior_successor = None;
    let mut targets = BTreeSet::new();
    let mut target_witnesses = Vec::new();
    let mut successor_witnesses = Vec::new();
    let mut successors = BTreeSet::new();
    for (ordinal, fragment) in request.fragments.iter().enumerate() {
        if fragment.key() != request.proposal.key() || fragment.ordinal() != ordinal {
            return Err(ComposerHostError::MutationMalformed);
        }
        let part = match fragment.payload() {
            MutationFragmentPayload::Utf8 {
                inserted_offset,
                text,
            } => {
                if *inserted_offset != inserted {
                    return Err(ComposerHostError::MutationMalformed);
                }
                inserted = inserted
                    .checked_add(
                        u64::try_from(text.len())
                            .map_err(|_| ComposerHostError::MutationMalformed)?,
                    )
                    .ok_or(ComposerHostError::MutationMalformed)?;
                if text.is_empty() {
                    Vec::new()
                } else {
                    vec![DraftPieceV1::Text(text.clone())]
                }
            }
            MutationFragmentPayload::Object(ObjectChange::Insert { at, object }) => {
                prove_destination(storage, store, root, *at, None)?;
                if at.byte_offset != object.anchor()
                    || object.anchor().get()
                        != start
                            .byte_offset
                            .get()
                            .checked_add(inserted)
                            .ok_or(ComposerHostError::MutationMalformed)?
                {
                    return Err(ComposerHostError::MutationMalformed);
                }
                let marker = new_marker(&mut metadata, *object)?;
                successor_witnesses.push(DraftPieceMarkerAtV1::new(object.anchor().get(), marker));
                validate_successor(&mut prior_successor, &mut successors, *object)?;
                vec![DraftPieceV1::Marker(marker)]
            }
            MutationFragmentPayload::Object(ObjectChange::Remove { target }) => {
                if !targets.insert(target.id()) {
                    return Err(ComposerHostError::MutationMalformed);
                }
                let target_request = *target;
                let target = target_marker(storage, store, root, target_request, &mut metadata)?;
                validate_target_coverage(
                    storage,
                    store,
                    root,
                    predecessor_start,
                    predecessor_end,
                    target_request,
                )?;
                target_witnesses.push(target);
                Vec::new()
            }
            MutationFragmentPayload::Object(ObjectChange::Replace { target, object }) => {
                if !targets.insert(target.id()) {
                    return Err(ComposerHostError::MutationMalformed);
                }
                let target_request = *target;
                let target = target_marker(storage, store, root, target_request, &mut metadata)?;
                validate_target_coverage(
                    storage,
                    store,
                    root,
                    predecessor_start,
                    predecessor_end,
                    target_request,
                )?;
                target_witnesses.push(target);
                if object.anchor().get() != target.anchor()
                    || object.anchor().get()
                        != start
                            .byte_offset
                            .get()
                            .checked_add(inserted)
                            .ok_or(ComposerHostError::MutationMalformed)?
                    || u64::try_from(object.order().get())
                        .map_err(|_| ComposerHostError::MutationMalformed)?
                        != target.marker().order_key()
                {
                    return Err(ComposerHostError::MutationMalformed);
                }
                let marker = if marker_id(object.id()) == target.marker().marker_id() {
                    DraftPieceMarkerV1::new(
                        target.marker().marker_id(),
                        u64::try_from(object.order().get())
                            .map_err(|_| ComposerHostError::MutationMalformed)?,
                        target.marker().label(),
                    )
                } else {
                    new_marker(&mut metadata, *object)?
                };
                successor_witnesses.push(DraftPieceMarkerAtV1::new(object.anchor().get(), marker));
                validate_successor(&mut prior_successor, &mut successors, *object)?;
                if marker.marker_id() == target.marker().marker_id() {
                    if marker.label() != target.marker().label() {
                        return Err(ComposerHostError::MutationMalformed);
                    }
                    moves.push(DraftPieceMarkerMoveV1::new(target, marker, 1));
                }
                vec![DraftPieceV1::Marker(marker)]
            }
            MutationFragmentPayload::Object(ObjectChange::Move { target, to, object }) => {
                if !targets.insert(target.id()) {
                    return Err(ComposerHostError::MutationMalformed);
                }
                let target_request = *target;
                let target = target_marker(storage, store, root, target_request, &mut metadata)?;
                prove_destination(storage, store, root, *to, Some((target_request, target)))?;
                target_witnesses.push(target);
                if marker_id(object.id()) != target.marker().marker_id()
                    || to.byte_offset != object.anchor()
                    || object.anchor().get()
                        != start
                            .byte_offset
                            .get()
                            .checked_add(inserted)
                            .ok_or(ComposerHostError::MutationMalformed)?
                {
                    return Err(ComposerHostError::MutationMalformed);
                }
                let marker = DraftPieceMarkerV1::new(
                    target.marker().marker_id(),
                    u64::try_from(object.order().get())
                        .map_err(|_| ComposerHostError::MutationMalformed)?,
                    target.marker().label(),
                );
                successor_witnesses.push(DraftPieceMarkerAtV1::new(object.anchor().get(), marker));
                validate_successor(&mut prior_successor, &mut successors, *object)?;
                moves.push(DraftPieceMarkerMoveV1::new(target, marker, 1));
                vec![DraftPieceV1::Marker(marker)]
            }
            MutationFragmentPayload::Atom(_) => {
                return Err(ComposerHostError::MutationMalformed);
            }
            MutationFragmentPayload::Terminal { intended } => {
                if ordinal + 1 != request.fragments.len() {
                    return Err(ComposerHostError::MutationMalformed);
                }
                if terminal.replace(*intended).is_some() {
                    return Err(ComposerHostError::MutationMalformed);
                }
                continue;
            }
        };
        parts.push(part);
    }
    let positions = terminal.ok_or(ComposerHostError::MutationMalformed)?;
    validate_terminal_positions(
        storage,
        store,
        root,
        request.proposal,
        positions,
        &request.fragments,
        &target_witnesses,
        &successor_witnesses,
    )?;
    if positions.caret() != positions.selection_head() || !metadata.is_empty() {
        return Err(ComposerHostError::MutationMalformed);
    }
    if parts.is_empty() {
        return Err(ComposerHostError::MutationMalformed);
    }
    let first = parts.remove(0);
    let mut replacements = Vec::with_capacity(parts.len());
    replacements.push(
        DraftPieceReplacementV1::new(predecessor_start, predecessor_end, first).with_moves(moves),
    );
    for part in parts {
        replacements.push(DraftPieceReplacementV1::continuation(
            predecessor_start,
            predecessor_end,
            part,
        ));
    }
    Ok((
        replacements,
        positions,
        target_witnesses,
        successor_witnesses,
    ))
}

fn new_marker(
    metadata: &mut BTreeMap<InlineObjectId, ImageLabelOrdinal>,
    object: gpui_text_input::SuccessorObject,
) -> Result<DraftPieceMarkerV1, ComposerHostError> {
    let label = metadata
        .remove(&object.id())
        .ok_or(ComposerHostError::MutationMalformed)?;
    Ok(DraftPieceMarkerV1::new(
        marker_id(object.id()),
        u64::try_from(object.order().get()).map_err(|_| ComposerHostError::MutationMalformed)?,
        label,
    ))
}

fn target_marker(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    target: gpui_text_input::ObjectTarget,
    metadata: &mut BTreeMap<InlineObjectId, ImageLabelOrdinal>,
) -> Result<DraftPieceMarkerAtV1, ComposerHostError> {
    if target.range().start().byte_offset != target.range().end().byte_offset {
        return Err(ComposerHostError::MutationMalformed);
    }
    let marker_id = marker_id(target.id());
    let order_key =
        u64::try_from(target.order().get()).map_err(|_| ComposerHostError::MutationMalformed)?;
    let existing = storage
        .draft_marker_identity(store, root, marker_id)?
        .ok_or(ComposerHostError::MutationMalformed)?;
    let supplied_label = metadata
        .remove(&target.id())
        .ok_or(ComposerHostError::MutationMalformed)?;
    if existing.marker_id() != marker_id
        || existing.order_key() != order_key
        || existing.label() != supplied_label
    {
        return Err(ComposerHostError::MutationMalformed);
    }
    let marker = DraftPieceMarkerV1::new(marker_id, existing.order_key(), existing.label());
    let witness = DraftPieceMarkerAtV1::new(target.range().start().byte_offset.get(), marker);
    let start = proven_position(storage, store, root, target.range().start())?;
    let end = proven_position(storage, store, root, target.range().end())?;
    let starts_before_target = match start.gap() {
        DraftCompositeGapWitnessV1::BeforeAll => true,
        DraftCompositeGapWitnessV1::Between {
            right_order_key,
            right_marker_id,
            ..
        } => right_order_key == marker.order_key() && right_marker_id == marker.marker_id(),
        DraftCompositeGapWitnessV1::Unambiguous | DraftCompositeGapWitnessV1::AfterAll => false,
    };
    let ends_after_target = match end.gap() {
        DraftCompositeGapWitnessV1::AfterAll => true,
        DraftCompositeGapWitnessV1::Between {
            left_order_key,
            left_marker_id,
            ..
        } => left_order_key == marker.order_key() && left_marker_id == marker.marker_id(),
        DraftCompositeGapWitnessV1::Unambiguous | DraftCompositeGapWitnessV1::BeforeAll => false,
    };
    if !starts_before_target || !ends_after_target {
        return Err(ComposerHostError::MutationMalformed);
    }
    if !storage.validate_draft_marker_location(store, root, witness)? {
        return Err(ComposerHostError::MutationMalformed);
    }
    Ok(witness)
}

fn validate_successor(
    prior: &mut Option<(u64, u64, InlineObjectId)>,
    successors: &mut BTreeSet<InlineObjectId>,
    object: gpui_text_input::SuccessorObject,
) -> Result<(), ComposerHostError> {
    let order =
        u64::try_from(object.order().get()).map_err(|_| ComposerHostError::MutationMalformed)?;
    let current = (object.anchor().get(), order, object.id());
    if !successors.insert(object.id()) || prior.is_some_and(|prior| prior >= current) {
        return Err(ComposerHostError::MutationMalformed);
    }
    *prior = Some(current);
    Ok(())
}

fn marker_metadata(
    supplied: &[ComposerHostImageMarkerMetadata],
) -> Result<BTreeMap<InlineObjectId, ImageLabelOrdinal>, ComposerHostError> {
    let mut metadata = BTreeMap::new();
    for value in supplied {
        if metadata.insert(value.object_id(), value.label()).is_some() {
            return Err(ComposerHostError::MutationMalformed);
        }
    }
    Ok(metadata)
}

pub(super) fn marker_id(id: InlineObjectId) -> SyndicDraftMarkerId {
    SyndicDraftMarkerId::from_bytes(id.get().to_be_bytes())
}

pub(super) fn proven_position(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    position: SourcePosition,
) -> Result<DraftCompositePositionV1, ComposerHostError> {
    let anchor = position.byte_offset.get();
    let request = match position.gap {
        InlineObjectGap::NoObjects => DraftPieceMarkerEdgeProofRequestV1::Absence { anchor },
        InlineObjectGap::Before(neighbor) => DraftPieceMarkerEdgeProofRequestV1::First {
            marker: marker_neighbor(storage, store, root, anchor, neighbor)?,
        },
        InlineObjectGap::After(neighbor) => DraftPieceMarkerEdgeProofRequestV1::Last {
            marker: marker_neighbor(storage, store, root, anchor, neighbor)?,
        },
        InlineObjectGap::Between {
            preceding,
            following,
        } => DraftPieceMarkerEdgeProofRequestV1::Adjacent {
            left: marker_neighbor(storage, store, root, anchor, preceding)?,
            right: marker_neighbor(storage, store, root, anchor, following)?,
        },
    };
    let retained_byte_ceiling = match request {
        DraftPieceMarkerEdgeProofRequestV1::Absence { .. } => 9,
        DraftPieceMarkerEdgeProofRequestV1::First { .. }
        | DraftPieceMarkerEdgeProofRequestV1::Last { .. } => 41,
        DraftPieceMarkerEdgeProofRequestV1::Adjacent { .. } => 81,
    };
    let expected = match request {
        DraftPieceMarkerEdgeProofRequestV1::Absence { anchor } => {
            DraftPieceMarkerEdgeProofV1::Absence { anchor }
        }
        DraftPieceMarkerEdgeProofRequestV1::First { marker } => {
            DraftPieceMarkerEdgeProofV1::First { marker }
        }
        DraftPieceMarkerEdgeProofRequestV1::Last { marker } => {
            DraftPieceMarkerEdgeProofV1::Last { marker }
        }
        DraftPieceMarkerEdgeProofRequestV1::Adjacent { left, right } => {
            DraftPieceMarkerEdgeProofV1::Adjacent { left, right }
        }
    };
    if storage.draft_piece_marker_edge_proof(store, root, request, retained_byte_ceiling)?
        != Some(expected)
    {
        return Err(ComposerHostError::MutationMalformed);
    }
    canonical_position(position)
}

fn marker_neighbor(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    anchor: u64,
    neighbor: gpui_text_input::InlineObjectNeighbor,
) -> Result<DraftPieceMarkerAtV1, ComposerHostError> {
    let id = marker_id(neighbor.id());
    let order =
        u64::try_from(neighbor.order().get()).map_err(|_| ComposerHostError::MutationMalformed)?;
    let occurrence = storage
        .draft_marker_identity(store, root, id)?
        .ok_or(ComposerHostError::MutationMalformed)?;
    if occurrence.marker_id() != id || occurrence.order_key() != order {
        return Err(ComposerHostError::MutationMalformed);
    }
    let marker = DraftPieceMarkerAtV1::new(
        anchor,
        DraftPieceMarkerV1::new(id, order, occurrence.label()),
    );
    if !storage.validate_draft_marker_location(store, root, marker)? {
        return Err(ComposerHostError::MutationMalformed);
    }
    Ok(marker)
}

pub(super) fn canonical_position(
    position: SourcePosition,
) -> Result<DraftCompositePositionV1, ComposerHostError> {
    let gap = match position.gap {
        InlineObjectGap::NoObjects => DraftCompositeGapWitnessV1::Unambiguous,
        InlineObjectGap::Before(_) => DraftCompositeGapWitnessV1::BeforeAll,
        InlineObjectGap::After(_) => DraftCompositeGapWitnessV1::AfterAll,
        InlineObjectGap::Between {
            preceding,
            following,
        } => DraftCompositeGapWitnessV1::Between {
            left_order_key: u64::try_from(preceding.order().get())
                .map_err(|_| ComposerHostError::MutationMalformed)?,
            left_marker_id: marker_id(preceding.id()),
            right_order_key: u64::try_from(following.order().get())
                .map_err(|_| ComposerHostError::MutationMalformed)?,
            right_marker_id: marker_id(following.id()),
        },
    };
    Ok(DraftCompositePositionV1::new(
        position.byte_offset.get(),
        gap,
    ))
}

fn validate_target_coverage(
    _storage: &syndic_storage::SyndicStorage,
    _store: &HomeStore,
    _root: DraftPieceRootReferenceV1,
    envelope_start: DraftCompositePositionV1,
    envelope_end: DraftCompositePositionV1,
    target: gpui_text_input::ObjectTarget,
) -> Result<(), ComposerHostError> {
    let target_start = canonical_position(target.range().start())?;
    let target_end = canonical_position(target.range().end())?;
    if compare_positions(envelope_start, target_start)? == Ordering::Greater
        || compare_positions(target_end, envelope_end)? == Ordering::Greater
    {
        return Err(ComposerHostError::MutationMalformed);
    }
    Ok(())
}

fn compare_positions(
    left: DraftCompositePositionV1,
    right: DraftCompositePositionV1,
) -> Result<Ordering, ComposerHostError> {
    match left.utf8_offset().cmp(&right.utf8_offset()) {
        Ordering::Equal => {}
        ordering => return Ok(ordering),
    }
    match (left.gap(), right.gap()) {
        (DraftCompositeGapWitnessV1::Unambiguous, DraftCompositeGapWitnessV1::Unambiguous)
        | (DraftCompositeGapWitnessV1::BeforeAll, DraftCompositeGapWitnessV1::BeforeAll)
        | (DraftCompositeGapWitnessV1::AfterAll, DraftCompositeGapWitnessV1::AfterAll) => {
            Ok(Ordering::Equal)
        }
        (DraftCompositeGapWitnessV1::BeforeAll, _) => Ok(Ordering::Less),
        (_, DraftCompositeGapWitnessV1::BeforeAll) => Ok(Ordering::Greater),
        (DraftCompositeGapWitnessV1::AfterAll, _) => Ok(Ordering::Greater),
        (_, DraftCompositeGapWitnessV1::AfterAll) => Ok(Ordering::Less),
        (
            DraftCompositeGapWitnessV1::Between {
                left_order_key: left_order,
                left_marker_id: left_id,
                right_order_key: left_right_order,
                right_marker_id: left_right_id,
            },
            DraftCompositeGapWitnessV1::Between {
                left_order_key: right_order,
                left_marker_id: right_id,
                right_order_key: right_right_order,
                right_marker_id: right_right_id,
            },
        ) => Ok(
            (left_order, left_id, left_right_order, left_right_id).cmp(&(
                right_order,
                right_id,
                right_right_order,
                right_right_id,
            )),
        ),
        (DraftCompositeGapWitnessV1::Unambiguous, _)
        | (_, DraftCompositeGapWitnessV1::Unambiguous) => Err(ComposerHostError::MutationMalformed),
    }
}
