use super::terminal_gap::validate_exact_terminal_gap;
use super::terminal_transform::UnchangedMarkerTransform;
use super::translation::{marker_id, proven_position};
use super::*;

pub(super) fn validate_request_key(
    binding: ComposerHostBinding,
    request: &ComposerHostMutationRequest,
) -> Result<(), ComposerHostError> {
    let key = request.proposal.key();
    let mut operation = [0_u8; 16];
    operation[8..].copy_from_slice(&key.operation().get().to_be_bytes());
    if key.binding() != BindingId::new(binding.host_generation().get())
        || key.base_revision() != SourceRevision::new(binding.candidate().candidate_generation())
        || request.operation_id.as_bytes() != &operation
    {
        return Err(ComposerHostError::MutationMalformed);
    }
    Ok(())
}

pub(super) fn validate_object_changes(
    proposal: MutationProposal,
    fragments: &[MutationFragment],
) -> Result<(), ComposerHostError> {
    let replacement = proposal.replacement();
    let mut seen_ids = BTreeSet::new();
    let mut previous_target: Option<gpui_text_input::ObjectTarget> = None;
    let mut previous_successor: Option<gpui_text_input::SuccessorObject> = None;
    for fragment in fragments {
        let MutationFragmentPayload::Object(change) = fragment.payload() else {
            continue;
        };
        let (target, destination, successor) = match *change {
            ObjectChange::Insert { at, object } => (None, Some(at), Some(object)),
            ObjectChange::Remove { target } => (Some(target), None, None),
            ObjectChange::Replace { target, object } => (Some(target), None, Some(object)),
            ObjectChange::Move { target, to, object } => (Some(target), Some(to), Some(object)),
        };
        if target.is_some_and(|target| !source_range_contains(replacement, target.range()))
            || destination
                .is_some_and(|position| !source_range_contains_position(replacement, position))
        {
            return Err(ComposerHostError::MutationMalformed);
        }
        if let Some(target) = target
            && successor.is_some_and(|object| {
                matches!(change, ObjectChange::Move { .. }) && object.id() != target.id()
            })
        {
            return Err(ComposerHostError::MutationMalformed);
        }
        match *change {
            ObjectChange::Insert { at, object } => validate_successor_at(at, object, None)?,
            ObjectChange::Remove { .. } => {}
            ObjectChange::Replace { target, object } => {
                if object.anchor() != target.range().start().byte_offset
                    || object.order() != target.order()
                    || (object.id() != target.id()
                        && target_unchanged_neighbors(target)
                            .into_iter()
                            .flatten()
                            .any(|neighbor| neighbor == object.id()))
                {
                    return Err(ComposerHostError::MutationMalformed);
                }
            }
            ObjectChange::Move { target, to, object } => {
                validate_successor_at(to, object, Some(target.id()))?;
            }
        }
        if target.is_some_and(|target| seen_ids.contains(&target.id()))
            || successor.is_some_and(|object| seen_ids.contains(&object.id()))
        {
            return Err(ComposerHostError::MutationMalformed);
        }
        if let Some(target) = target {
            seen_ids.insert(target.id());
        }
        if let Some(successor) = successor {
            seen_ids.insert(successor.id());
        }
        if let (Some(previous), Some(actual)) = (previous_target, target)
            && !matches!(
                previous
                    .range()
                    .end()
                    .compare_in_revision(actual.range().start()),
                Some(Ordering::Less | Ordering::Equal)
            )
        {
            return Err(ComposerHostError::MutationMalformed);
        }
        if let (Some(previous), Some(actual)) = (previous_successor, successor) {
            let previous_key = (previous.anchor(), previous.order(), previous.id());
            let actual_key = (actual.anchor(), actual.order(), actual.id());
            if (previous.anchor() == actual.anchor() && previous.order() == actual.order())
                || previous_key >= actual_key
            {
                return Err(ComposerHostError::MutationMalformed);
            }
        }
        if target.is_some() {
            previous_target = target;
        }
        if successor.is_some() {
            previous_successor = successor;
        }
    }
    Ok(())
}

fn validate_successor_at(
    position: SourcePosition,
    object: gpui_text_input::SuccessorObject,
    moving: Option<InlineObjectId>,
) -> Result<(), ComposerHostError> {
    if object.anchor() != position.byte_offset {
        return Err(ComposerHostError::MutationMalformed);
    }
    let invalid_neighbor = |neighbor: gpui_text_input::InlineObjectNeighbor| {
        neighbor.id() == object.id() || moving.is_some_and(|moving| neighbor.id() == moving)
    };
    let ordered = match position.gap {
        InlineObjectGap::NoObjects => true,
        InlineObjectGap::Before(following) => {
            !invalid_neighbor(following) && object.order() < following.order()
        }
        InlineObjectGap::Between {
            preceding,
            following,
        } => {
            !invalid_neighbor(preceding)
                && !invalid_neighbor(following)
                && preceding.order() < object.order()
                && object.order() < following.order()
        }
        InlineObjectGap::After(preceding) => {
            !invalid_neighbor(preceding) && preceding.order() < object.order()
        }
    };
    if !ordered {
        return Err(ComposerHostError::MutationMalformed);
    }
    Ok(())
}

fn target_unchanged_neighbors(
    target: gpui_text_input::ObjectTarget,
) -> [Option<InlineObjectId>; 2] {
    let preceding = match target.range().start().gap {
        InlineObjectGap::Between {
            preceding,
            following,
        } if following.id() == target.id() && following.order() == target.order() => {
            Some(preceding.id())
        }
        _ => None,
    };
    let following = match target.range().end().gap {
        InlineObjectGap::Between {
            preceding,
            following,
        } if preceding.id() == target.id() && preceding.order() == target.order() => {
            Some(following.id())
        }
        _ => None,
    };
    [preceding, following]
}

fn source_range_contains(
    outer: gpui_text_input::SourceRange,
    inner: gpui_text_input::SourceRange,
) -> bool {
    matches!(
        outer.start().compare_in_revision(inner.start()),
        Some(Ordering::Less | Ordering::Equal)
    ) && matches!(
        inner.end().compare_in_revision(outer.end()),
        Some(Ordering::Less | Ordering::Equal)
    )
}

fn source_range_contains_position(
    range: gpui_text_input::SourceRange,
    position: SourcePosition,
) -> bool {
    matches!(
        range.start().compare_in_revision(position),
        Some(Ordering::Less | Ordering::Equal)
    ) && matches!(
        position.compare_in_revision(range.end()),
        Some(Ordering::Less | Ordering::Equal)
    )
}

pub(super) fn prove_destination(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    destination: SourcePosition,
    moving: Option<(gpui_text_input::ObjectTarget, DraftPieceMarkerAtV1)>,
) -> Result<(), ComposerHostError> {
    if destination.gap == InlineObjectGap::NoObjects
        && moving.is_some_and(|(target, witness)| {
            witness.anchor() == destination.byte_offset.get()
                && matches!(target.range().start().gap, InlineObjectGap::Before(neighbor)
                    if neighbor.id() == target.id() && neighbor.order() == target.order())
                && matches!(target.range().end().gap, InlineObjectGap::After(neighbor)
                    if neighbor.id() == target.id() && neighbor.order() == target.order())
        })
    {
        return Ok(());
    }
    proven_position(storage, store, root, destination).map(|_| ())
}
pub(super) fn validate_terminal_positions(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    proposal: MutationProposal,
    positions: MutationPositions,
    fragments: &[MutationFragment],
    targets: &[DraftPieceMarkerAtV1],
    successors: &[DraftPieceMarkerAtV1],
) -> Result<(), ComposerHostError> {
    let transform = UnchangedMarkerTransform::new(proposal, fragments)?;
    for position in [
        positions.caret(),
        positions.selection_anchor(),
        positions.selection_head(),
    ] {
        validate_terminal_position(
            storage, store, root, proposal, transform, position, fragments, targets, successors,
        )?;
    }
    Ok(())
}

fn validate_terminal_position(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    proposal: MutationProposal,
    transform: UnchangedMarkerTransform,
    position: SourcePosition,
    fragments: &[MutationFragment],
    targets: &[DraftPieceMarkerAtV1],
    successors: &[DraftPieceMarkerAtV1],
) -> Result<(), ComposerHostError> {
    let removed = |id| {
        targets
            .iter()
            .any(|target| target.marker().marker_id() == marker_id(id))
    };
    let authenticate = |neighbor: gpui_text_input::InlineObjectNeighbor| {
        if let Some(successor) = successors
            .iter()
            .find(|successor| successor.marker().marker_id() == marker_id(neighbor.id()))
        {
            if successor.anchor() != position.byte_offset.get()
                || successor.marker().order_key()
                    != u64::try_from(neighbor.order().get())
                        .map_err(|_| ComposerHostError::MutationMalformed)?
            {
                return Err(ComposerHostError::MutationMalformed);
            }
            Ok(())
        } else if removed(neighbor.id()) {
            Err(ComposerHostError::MutationMalformed)
        } else {
            let identity = storage
                .draft_marker_identity(store, root, marker_id(neighbor.id()))?
                .ok_or(ComposerHostError::MutationMalformed)?;
            if identity.order_key()
                != u64::try_from(neighbor.order().get())
                    .map_err(|_| ComposerHostError::MutationMalformed)?
            {
                return Err(ComposerHostError::MutationMalformed);
            }
            let mut authenticated_anchor = None;
            for anchor in transform.predecessor_anchor_candidates(position.byte_offset.get())? {
                let Some(anchor) = anchor else {
                    continue;
                };
                let witness = DraftPieceMarkerAtV1::new(
                    anchor,
                    DraftPieceMarkerV1::new(
                        identity.marker_id(),
                        identity.order_key(),
                        identity.label(),
                    ),
                );
                if storage.validate_draft_marker_location(store, root, witness)? {
                    if authenticated_anchor.replace(anchor).is_some() {
                        return Err(ComposerHostError::MutationMalformed);
                    }
                }
            }
            let predecessor_anchor =
                authenticated_anchor.ok_or(ComposerHostError::MutationMalformed)?;
            if transform.successor_anchor(proposal, predecessor_anchor, neighbor)?
                != position.byte_offset.get()
            {
                return Err(ComposerHostError::MutationMalformed);
            }
            Ok(())
        }
    };
    match position.gap {
        InlineObjectGap::NoObjects => {
            if successors
                .iter()
                .any(|successor| successor.anchor() == position.byte_offset.get())
            {
                return Err(ComposerHostError::MutationMalformed);
            }
            if targets
                .iter()
                .any(|target| target.anchor() == position.byte_offset.get())
            {
                let sole_target = fragments.iter().any(|fragment| {
                    let target = match fragment.payload() {
                        MutationFragmentPayload::Object(ObjectChange::Remove { target })
                        | MutationFragmentPayload::Object(ObjectChange::Move { target, .. }) => {
                            Some(*target)
                        }
                        _ => None,
                    };
                    target.is_some_and(|target| {
                        target.range().start().byte_offset == position.byte_offset
                            && matches!(target.range().start().gap, InlineObjectGap::Before(_))
                            && matches!(target.range().end().gap, InlineObjectGap::After(_))
                    })
                });
                if !sole_target {
                    return Err(ComposerHostError::MutationMalformed);
                }
                return Ok(());
            }
            Ok(())
        }
        InlineObjectGap::Before(following) => {
            authenticate(following)?;
            let following_order = u64::try_from(following.order().get())
                .map_err(|_| ComposerHostError::MutationMalformed)?;
            if successors.iter().any(|successor| {
                successor.anchor() == position.byte_offset.get()
                    && successor.marker().marker_id() != marker_id(following.id())
                    && successor.marker().order_key() < following_order
            }) || successor_named_edge_is_invalid(fragments, following.id(), true)
            {
                return Err(ComposerHostError::MutationMalformed);
            }
            validate_exact_terminal_gap(
                storage, store, root, proposal, transform, position, targets, successors,
            )?;
            Ok(())
        }
        InlineObjectGap::After(preceding) => {
            authenticate(preceding)?;
            let preceding_order = u64::try_from(preceding.order().get())
                .map_err(|_| ComposerHostError::MutationMalformed)?;
            if successors.iter().any(|successor| {
                successor.anchor() == position.byte_offset.get()
                    && successor.marker().marker_id() != marker_id(preceding.id())
                    && successor.marker().order_key() > preceding_order
            }) || successor_named_edge_is_invalid(fragments, preceding.id(), false)
            {
                return Err(ComposerHostError::MutationMalformed);
            }
            validate_exact_terminal_gap(
                storage, store, root, proposal, transform, position, targets, successors,
            )?;
            Ok(())
        }
        InlineObjectGap::Between {
            preceding,
            following,
        } => {
            authenticate(preceding)?;
            authenticate(following)?;
            let left = u64::try_from(preceding.order().get())
                .map_err(|_| ComposerHostError::MutationMalformed)?;
            let right = u64::try_from(following.order().get())
                .map_err(|_| ComposerHostError::MutationMalformed)?;
            if left >= right
                || successors.iter().any(|successor| {
                    successor.anchor() == position.byte_offset.get()
                        && successor.marker().marker_id() != marker_id(preceding.id())
                        && successor.marker().marker_id() != marker_id(following.id())
                        && left < successor.marker().order_key()
                        && successor.marker().order_key() < right
                })
            {
                return Err(ComposerHostError::MutationMalformed);
            }
            validate_exact_terminal_gap(
                storage, store, root, proposal, transform, position, targets, successors,
            )?;
            Ok(())
        }
    }
}

fn successor_named_edge_is_invalid(
    fragments: &[MutationFragment],
    id: InlineObjectId,
    before: bool,
) -> bool {
    fragments.iter().any(|fragment| {
        let context = match fragment.payload() {
            MutationFragmentPayload::Object(ObjectChange::Insert { at, object })
            | MutationFragmentPayload::Object(ObjectChange::Move { to: at, object, .. })
                if object.id() == id =>
            {
                Some(at.gap)
            }
            MutationFragmentPayload::Object(ObjectChange::Replace { target, object })
                if object.id() == id =>
            {
                return if before {
                    !matches!(target.range().start().gap, InlineObjectGap::Before(_))
                } else {
                    !matches!(target.range().end().gap, InlineObjectGap::After(_))
                };
            }
            _ => None,
        };
        context.is_some_and(|gap| {
            if before {
                !matches!(gap, InlineObjectGap::NoObjects | InlineObjectGap::Before(_))
            } else {
                !matches!(gap, InlineObjectGap::NoObjects | InlineObjectGap::After(_))
            }
        })
    })
}

pub(super) fn validate_committed_successor(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    positions: MutationPositions,
    successors: &[DraftPieceMarkerAtV1],
) -> Result<(), ComposerHostError> {
    for position in [
        positions.caret(),
        positions.selection_anchor(),
        positions.selection_head(),
    ] {
        proven_position(storage, store, root, position)?;
    }
    for successor in successors {
        let identity = storage
            .draft_marker_identity(store, root, successor.marker().marker_id())?
            .ok_or(ComposerHostError::MutationMalformed)?;
        if identity.marker_id() != successor.marker().marker_id()
            || identity.order_key() != successor.marker().order_key()
            || identity.label() != successor.marker().label()
            || !storage.validate_draft_marker_location(store, root, *successor)?
        {
            return Err(ComposerHostError::MutationMalformed);
        }
    }
    Ok(())
}
