use std::collections::BTreeMap;

use beryl_model::SyndicDraftMarkerId;
use gpui_text_input::{
    InlineObjectGap, InlineObjectId, MutationLane, MutationPage, MutationPageItem, ObjectChange,
    SourcePosition,
};
use syndic_storage::{
    DraftCompositeGapWitnessV1, DraftCompositePositionV1, DraftMutationStagingLaneV1,
    DraftMutationStagingPageItemV1, DraftPieceDigestV1, DraftPieceMarkerAtV1,
    DraftPieceMarkerEffectChargesV1, DraftPieceMarkerEffectV1, DraftPieceMarkerInsertionV1,
    DraftPieceMarkerRemovalProofV1, DraftPieceMarkerV1, DraftPieceReplacementV1, DraftPieceV1,
    draft_piece_fragment_chain_link_v1,
};

use super::*;

pub(super) struct TranslatedWidgetPage {
    pub(super) lane: DraftMutationStagingLaneV1,
    pub(super) items: Vec<DraftMutationStagingPageItemV1>,
    pub(super) fragment_count: u64,
    pub(super) fragment_chain: DraftPieceDigestV1,
    pub(super) proposal_envelope_applied: bool,
    pub(super) last_proposal_range: Option<(DraftCompositePositionV1, DraftCompositePositionV1)>,
    pub(super) remaining_proposal_range: SourceRange,
}

pub(super) fn validate_marker_metadata_intake(
    page: &MutationPage,
    marker_metadata: &[ComposerHostImageMarkerMetadata],
) -> Result<(), ComposerHostError> {
    if page.items().is_empty()
        || page.items().len() > 256
        || page.totals().retained_bytes > 65_536
        || marker_metadata.len() > 256
    {
        return Err(ComposerHostError::MutationMalformed);
    }
    let consuming_items = match page.key().lane() {
        MutationLane::Source => 0,
        MutationLane::Proposal => page
            .items()
            .iter()
            .filter(|item| matches!(item, MutationPageItem::Object(ObjectChange::Insert { .. })))
            .count(),
    };
    if marker_metadata.len() != consuming_items {
        return Err(ComposerHostError::MutationMalformed);
    }
    Ok(())
}

impl TranslatedWidgetPage {
    pub(super) fn proposal(
        items: Vec<DraftMutationStagingPageItemV1>,
        fragment_count: u64,
        fragment_chain: DraftPieceDigestV1,
        proposal_envelope_applied: bool,
        last_proposal_range: Option<(DraftCompositePositionV1, DraftCompositePositionV1)>,
        remaining_proposal_range: SourceRange,
    ) -> Self {
        Self {
            lane: DraftMutationStagingLaneV1::Proposal,
            items,
            fragment_count,
            fragment_chain,
            proposal_envelope_applied,
            last_proposal_range,
            remaining_proposal_range,
        }
    }
}

pub(super) fn translate_widget_page(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    pending: &ComposerHostMutationCoordinator,
    page: &MutationPage,
    marker_metadata: &[ComposerHostImageMarkerMetadata],
) -> Result<TranslatedWidgetPage, ComposerHostError> {
    match page.key().lane() {
        MutationLane::Source => translate_source_page(pending, page),
        MutationLane::Proposal => {
            translate_proposal_page(storage, store, pending, page, marker_metadata)
        }
    }
}

fn translate_source_page(
    pending: &ComposerHostMutationCoordinator,
    page: &MutationPage,
) -> Result<TranslatedWidgetPage, ComposerHostError> {
    let envelope = pending.begin.proposal().replacement();
    let mut items = Vec::with_capacity(page.items().len());
    for item in page.items() {
        let position = match item {
            MutationPageItem::Utf8 {
                inserted_offset, ..
            } => {
                let offset = envelope
                    .start()
                    .byte_offset
                    .get()
                    .checked_add(*inserted_offset)
                    .ok_or(ComposerHostError::MutationMalformed)?;
                if offset == envelope.start().byte_offset.get() {
                    envelope.start()
                } else {
                    SourcePosition::new(
                        gpui_text_input::ByteOffset::new(offset),
                        InlineObjectGap::NoObjects,
                    )
                }
            }
            MutationPageItem::Object(change) => match change {
                ObjectChange::Insert { object } => {
                    SourcePosition::new(object.anchor(), InlineObjectGap::NoObjects)
                }
                ObjectChange::Remove { target }
                | ObjectChange::Replace { target, .. }
                | ObjectChange::Move { target, .. } => target.range().start(),
            },
            MutationPageItem::Atom(_) => return Err(ComposerHostError::MutationMalformed),
        };
        items.push(DraftMutationStagingPageItemV1::SourcePosition(
            canonical_position(position)?,
        ));
    }
    if items.is_empty() {
        return Err(ComposerHostError::MutationMalformed);
    }
    Ok(TranslatedWidgetPage {
        lane: DraftMutationStagingLaneV1::Source,
        items,
        fragment_count: pending.fragment_count,
        fragment_chain: pending.fragment_chain,
        proposal_envelope_applied: pending.proposal_envelope_applied,
        last_proposal_range: pending.last_proposal_range,
        remaining_proposal_range: pending.remaining_proposal_range,
    })
}

fn translate_proposal_page(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    pending: &ComposerHostMutationCoordinator,
    page: &MutationPage,
    marker_metadata: &[ComposerHostImageMarkerMetadata],
) -> Result<TranslatedWidgetPage, ComposerHostError> {
    let root = pending.session.newest_root();
    let mut remaining = pending.remaining_proposal_range;
    let mut start = canonical_position(remaining.start())?;
    let mut end = canonical_position(remaining.end())?;
    let mut metadata = marker_metadata_map(marker_metadata)?;
    let mut replacements = Vec::new();
    let mut envelope_applied = pending.proposal_envelope_applied;
    let mut last_range = pending.last_proposal_range;
    for item in page.items() {
        match item {
            MutationPageItem::Utf8 {
                inserted_offset,
                text,
            } => {
                let (item_start, item_end, continuing) = if !envelope_applied {
                    (start, end, false)
                } else {
                    let (last_start, last_end) =
                        last_range.ok_or(ComposerHostError::MutationMalformed)?;
                    (last_start, last_end, true)
                };
                let _ = inserted_offset;
                for (chunk_index, chunk) in scalar_chunks(text, 48 * 1024).into_iter().enumerate() {
                    let pieces = if chunk.is_empty() {
                        Vec::new()
                    } else {
                        vec![DraftPieceV1::Text(chunk.to_owned())]
                    };
                    replacements.push(replacement(
                        item_start,
                        item_end,
                        pieces,
                        continuing || chunk_index != 0,
                        None,
                    ));
                    envelope_applied = true;
                    last_range = Some((item_start, item_end));
                }
            }
            MutationPageItem::Object(change) => {
                let consumed = match *change {
                    ObjectChange::Insert { .. } => None,
                    ObjectChange::Remove { target }
                    | ObjectChange::Replace { target, .. }
                    | ObjectChange::Move { target, .. } => Some(target.range()),
                };
                let (pieces, effect, natural_point) = match *change {
                    ObjectChange::Insert { object } => {
                        let marker = new_marker(&mut metadata, object)?;
                        let point = canonical_position(SourcePosition::new(
                            object.anchor(),
                            InlineObjectGap::NoObjects,
                        ))?;
                        (
                            vec![DraftPieceV1::Marker(marker)],
                            DraftPieceMarkerEffectV1::Insert(DraftPieceMarkerInsertionV1::new(
                                object.anchor().get(),
                                marker,
                                DraftPieceMarkerEffectChargesV1::canonical_single_marker(),
                            )),
                            point,
                        )
                    }
                    ObjectChange::Remove { target } => {
                        let target_marker = target_marker(storage, store, root, target)?;
                        let target_start = canonical_position(target.range().start())?;
                        (
                            Vec::new(),
                            DraftPieceMarkerEffectV1::Remove {
                                removal: target_marker.removal,
                                charges: DraftPieceMarkerEffectChargesV1::canonical_single_marker(),
                            },
                            target_start,
                        )
                    }
                    ObjectChange::Replace { target, object } => {
                        if target.id() != object.id() {
                            return Err(ComposerHostError::MutationMalformed);
                        }
                        let target_marker = target_marker(storage, store, root, target)?;
                        let target_start = canonical_position(target.range().start())?;
                        let marker = DraftPieceMarkerV1::new(
                            target_marker.marker.marker().marker_id(),
                            object_order(object.order())?,
                            target_marker.marker.marker().label(),
                        );
                        (
                            vec![DraftPieceV1::Marker(marker)],
                            DraftPieceMarkerEffectV1::SameIdReplacement {
                                removal: target_marker.removal,
                                insertion: DraftPieceMarkerInsertionV1::new(
                                    object.anchor().get(),
                                    marker,
                                    DraftPieceMarkerEffectChargesV1::canonical_single_marker(),
                                ),
                            },
                            target_start,
                        )
                    }
                    ObjectChange::Move { target, object } => {
                        if target.id() != object.id() {
                            return Err(ComposerHostError::MutationMalformed);
                        }
                        let target_marker = target_marker(storage, store, root, target)?;
                        let point = canonical_position(SourcePosition::new(
                            object.anchor(),
                            InlineObjectGap::NoObjects,
                        ))?;
                        let marker = DraftPieceMarkerV1::new(
                            target_marker.marker.marker().marker_id(),
                            object_order(object.order())?,
                            target_marker.marker.marker().label(),
                        );
                        (
                            vec![DraftPieceV1::Marker(marker)],
                            DraftPieceMarkerEffectV1::Move {
                                removal: target_marker.removal,
                                insertion: DraftPieceMarkerInsertionV1::new(
                                    object.anchor().get(),
                                    marker,
                                    DraftPieceMarkerEffectChargesV1::canonical_single_marker(),
                                ),
                            },
                            point,
                        )
                    }
                };
                let point = if !envelope_applied {
                    start
                } else if natural_point.utf8_offset() > end.utf8_offset() {
                    natural_point
                } else {
                    end
                };
                replacements.push(replacement(point, point, pieces, false, Some(effect)));
                last_range = Some((point, point));
                if let Some(consumed) = consumed {
                    if consumed.start() == remaining.start() {
                        start = canonical_position(consumed.end())?;
                        remaining = SourceRange::new(consumed.end(), remaining.end())
                            .map_err(|_| ComposerHostError::MutationMalformed)?;
                    } else if consumed.end() == remaining.end() {
                        end = canonical_position(consumed.start())?;
                        remaining = SourceRange::new(remaining.start(), consumed.start())
                            .map_err(|_| ComposerHostError::MutationMalformed)?;
                    }
                }
            }
            MutationPageItem::Atom(_) => return Err(ComposerHostError::MutationMalformed),
        }
    }
    if replacements.is_empty() || !metadata.is_empty() {
        return Err(ComposerHostError::MutationMalformed);
    }
    let mut count = pending.fragment_count;
    let mut chain = pending.fragment_chain;
    let mut items = Vec::with_capacity(replacements.len());
    for replacement in replacements {
        count = count
            .checked_add(1)
            .ok_or(ComposerHostError::MutationMalformed)?;
        chain = draft_piece_fragment_chain_link_v1(chain, count, &replacement);
        items.push(DraftMutationStagingPageItemV1::Proposal(replacement));
    }
    Ok(TranslatedWidgetPage::proposal(
        items,
        count,
        chain,
        envelope_applied,
        last_range,
        remaining,
    ))
}

fn replacement(
    start: DraftCompositePositionV1,
    end: DraftCompositePositionV1,
    pieces: Vec<DraftPieceV1>,
    continuation: bool,
    effect: Option<DraftPieceMarkerEffectV1>,
) -> DraftPieceReplacementV1 {
    let replacement = if continuation {
        DraftPieceReplacementV1::continuation(start, end, pieces)
    } else {
        DraftPieceReplacementV1::new(start, end, pieces)
    };
    if let Some(effect) = effect {
        replacement.with_marker_effect(effect)
    } else {
        replacement
    }
}

struct ValidatedTargetMarker {
    marker: DraftPieceMarkerAtV1,
    removal: DraftPieceMarkerRemovalProofV1,
}

fn scalar_chunks(value: &str, max_bytes: usize) -> Vec<&str> {
    if value.is_empty() {
        return vec![value];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < value.len() {
        let mut end = value.len().min(start + max_bytes);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        chunks.push(&value[start..end]);
        start = end;
    }
    chunks
}

fn marker_metadata_map(
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

fn new_marker(
    metadata: &mut BTreeMap<InlineObjectId, ImageLabelOrdinal>,
    object: gpui_text_input::SuccessorObject,
) -> Result<DraftPieceMarkerV1, ComposerHostError> {
    let label = metadata
        .remove(&object.id())
        .ok_or(ComposerHostError::MutationMalformed)?;
    Ok(DraftPieceMarkerV1::new(
        marker_id(object.id()),
        object_order(object.order())?,
        label,
    ))
}

fn target_marker(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    target: gpui_text_input::ObjectTarget,
) -> Result<ValidatedTargetMarker, ComposerHostError> {
    if target.range().start().byte_offset != target.range().end().byte_offset {
        return Err(ComposerHostError::MutationMalformed);
    }
    let id = marker_id(target.id());
    let existing = storage
        .draft_marker_identity(store, root, id)?
        .ok_or(ComposerHostError::MutationMalformed)?;
    if existing.marker_id() != id || existing.order_key() != object_order(target.order())? {
        return Err(ComposerHostError::MutationMalformed);
    }
    let marker = DraftPieceMarkerAtV1::new(
        target.range().start().byte_offset.get(),
        DraftPieceMarkerV1::new(id, existing.order_key(), existing.label()),
    );
    if !storage.validate_draft_marker_location(store, root, marker)? {
        return Err(ComposerHostError::MutationMalformed);
    }
    Ok(ValidatedTargetMarker {
        marker,
        removal: DraftPieceMarkerRemovalProofV1::new(
            canonical_position(target.range().start())?,
            existing,
        ),
    })
}

fn object_order(order: gpui_text_input::InlineObjectOrder) -> Result<u64, ComposerHostError> {
    u64::try_from(order.get()).map_err(|_| ComposerHostError::MutationMalformed)
}

fn marker_id(id: InlineObjectId) -> SyndicDraftMarkerId {
    SyndicDraftMarkerId::from_bytes(id.get().to_be_bytes())
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
            left_order_key: object_order(preceding.order())?,
            left_marker_id: marker_id(preceding.id()),
            right_order_key: object_order(following.order())?,
            right_marker_id: marker_id(following.id()),
        },
    };
    Ok(DraftCompositePositionV1::new(
        position.byte_offset.get(),
        gap,
    ))
}
