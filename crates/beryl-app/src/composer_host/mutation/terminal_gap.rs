use super::terminal_transform::UnchangedMarkerTransform;
use super::translation::marker_id;
use super::*;

pub(super) fn validate_exact_terminal_gap(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    proposal: MutationProposal,
    transform: UnchangedMarkerTransform,
    position: SourcePosition,
    targets: &[DraftPieceMarkerAtV1],
    successors: &[DraftPieceMarkerAtV1],
) -> Result<(), ComposerHostError> {
    let removed: BTreeSet<_> = targets
        .iter()
        .map(|target| target.marker().marker_id())
        .collect();
    let mut predecessor_anchors = transform
        .predecessor_anchor_candidates(position.byte_offset.get())?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    predecessor_anchors.sort_unstable();
    predecessor_anchors.dedup();

    let mut markers = Vec::new();
    for predecessor_anchor in predecessor_anchors {
        let page = storage.draft_piece_marker_demand(
            store,
            root,
            DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(predecessor_anchor),
                DraftPieceMarkerDirectionV1::Forward,
                None,
                DRAFT_PIECE_PAGE_MAX_RECORDS,
                DRAFT_PIECE_PAGE_MAX_BYTES,
            ),
        )?;
        if page.root() != root || !page.requested_side_complete() || page.continuation().is_some() {
            return Err(ComposerHostError::MutationMalformed);
        }
        for occurrence in page.markers() {
            let marker = occurrence.marker();
            if removed.contains(&marker.marker_id()) {
                continue;
            }
            let neighbor = gpui_text_input::InlineObjectNeighbor::new(
                InlineObjectId::new(u128::from_be_bytes(*marker.marker_id().as_bytes())),
                gpui_text_input::InlineObjectOrder::new(u128::from(marker.order_key())),
            );
            if transform.successor_anchor(proposal, occurrence.anchor(), neighbor)?
                == position.byte_offset.get()
            {
                markers.push(marker);
            }
        }
    }
    markers.extend(
        successors
            .iter()
            .filter(|successor| successor.anchor() == position.byte_offset.get())
            .map(|successor| successor.marker()),
    );
    markers.sort_unstable_by_key(|marker| (marker.order_key(), marker.marker_id()));
    if markers.windows(2).any(|pair| {
        pair[0].order_key() == pair[1].order_key() || pair[0].marker_id() == pair[1].marker_id()
    }) {
        return Err(ComposerHostError::MutationMalformed);
    }

    let matches = |marker: DraftPieceMarkerV1,
                   neighbor: gpui_text_input::InlineObjectNeighbor|
     -> Result<bool, ComposerHostError> {
        Ok(marker.marker_id() == marker_id(neighbor.id())
            && marker.order_key()
                == u64::try_from(neighbor.order().get())
                    .map_err(|_| ComposerHostError::MutationMalformed)?)
    };
    match position.gap {
        InlineObjectGap::NoObjects => {
            if !markers.is_empty() {
                return Err(ComposerHostError::MutationMalformed);
            }
        }
        InlineObjectGap::Before(following) => {
            if !markers
                .first()
                .is_some_and(|marker| matches(*marker, following).unwrap_or(false))
            {
                return Err(ComposerHostError::MutationMalformed);
            }
        }
        InlineObjectGap::After(preceding) => {
            if !markers
                .last()
                .is_some_and(|marker| matches(*marker, preceding).unwrap_or(false))
            {
                return Err(ComposerHostError::MutationMalformed);
            }
        }
        InlineObjectGap::Between {
            preceding,
            following,
        } => {
            let Some(index) = markers
                .iter()
                .position(|marker| matches(*marker, preceding).unwrap_or(false))
            else {
                return Err(ComposerHostError::MutationMalformed);
            };
            if !markers
                .get(index + 1)
                .is_some_and(|marker| matches(*marker, following).unwrap_or(false))
            {
                return Err(ComposerHostError::MutationMalformed);
            }
        }
    }
    Ok(())
}
