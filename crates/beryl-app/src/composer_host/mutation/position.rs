use gpui_text_input::{
    ByteOffset, InlineObjectGap, InlineObjectNeighbor, MutationPositionsIntent, SourcePosition,
};
use syndic_storage::{
    DraftPieceMarkerAtV1, DraftPieceMarkerEdgeProofRequestV1, DraftPieceMarkerEdgeProofV1,
    DraftPieceMarkerV1,
};

use super::*;

pub(super) fn page_is_replacement(
    page: &MutationPage,
    replacement: SourceRange,
    mut inserted: u64,
) -> bool {
    page.items().iter().all(|item| match item {
        gpui_text_input::MutationPageItem::Utf8 {
            inserted_offset,
            text,
        } => {
            if *inserted_offset != inserted {
                return false;
            }
            let Some(next) = inserted.checked_add(text.len() as u64) else {
                return false;
            };
            inserted = next;
            true
        }
        gpui_text_input::MutationPageItem::Object(gpui_text_input::ObjectChange::Remove {
            target,
        }) => {
            matches!(
                replacement
                    .start()
                    .compare_in_revision(target.range().start()),
                Some(Ordering::Less | Ordering::Equal)
            ) && matches!(
                target.range().end().compare_in_revision(replacement.end()),
                Some(Ordering::Less | Ordering::Equal)
            )
        }
        _ => false,
    })
}

pub(super) fn resolve_intended(
    host: &SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    begin: MutationBeginRequest,
    proposal: WidgetLaneFrontier,
    finish: MutationFinishInput,
) -> Result<MutationPositions, ComposerHostError> {
    let MutationPositionsIntent::ReplacementEnd {
        replacement,
        inserted_utf8_bytes,
    } = finish.intended()
    else {
        return finish
            .intended()
            .exact()
            .ok_or(ComposerHostError::MutationMalformed);
    };
    if replacement != begin.proposal().replacement()
        || !proposal.utf8_removals_only
        || inserted_utf8_bytes != finish.proposal().totals.inserted_bytes
    {
        return Err(ComposerHostError::MutationMalformed);
    }
    let start = replacement.start().byte_offset.get();
    let end = replacement.end().byte_offset.get();
    let removed = end
        .checked_sub(start)
        .ok_or(ComposerHostError::MutationMalformed)?;
    let byte_len = binding
        .logical_extent()
        .logical_utf8_bytes()
        .checked_sub(removed)
        .and_then(|bytes| bytes.checked_add(inserted_utf8_bytes))
        .ok_or(ComposerHostError::MutationMalformed)?;
    let collapse = start
        .checked_add(inserted_utf8_bytes)
        .ok_or(ComposerHostError::MutationMalformed)?;
    if byte_len != finish.intended_extent().byte_len() || collapse > byte_len {
        return Err(ComposerHostError::MutationMalformed);
    }
    let preceding = match replacement.start().gap {
        InlineObjectGap::After(object)
        | InlineObjectGap::Between {
            preceding: object, ..
        } => Some(object),
        InlineObjectGap::NoObjects | InlineObjectGap::Before(_) => None,
    };
    let following = match replacement.end().gap {
        InlineObjectGap::Before(object)
        | InlineObjectGap::Between {
            following: object, ..
        } => Some(object),
        InlineObjectGap::NoObjects | InlineObjectGap::After(_) => None,
    };
    for (object, endpoint, first) in [
        (preceding, replacement.start(), false),
        (following, replacement.end(), true),
    ] {
        if let Some(object) = object {
            authenticate_boundary(host, store, binding, object, endpoint, first)?;
        }
    }
    let preceding = if inserted_utf8_bytes == 0 {
        preceding
    } else {
        None
    };
    let gap = match (preceding, following) {
        (None, None) => InlineObjectGap::NoObjects,
        (Some(object), None) => InlineObjectGap::after(object),
        (None, Some(object)) => InlineObjectGap::before(object),
        (Some(left), Some(right)) => InlineObjectGap::between(left, right)
            .map_err(|_| ComposerHostError::MutationPositionUnrepresentable)?,
    };
    Ok(MutationPositions::collapsed(SourcePosition::new(
        ByteOffset::new(collapse),
        gap,
    )))
}

fn authenticate_boundary(
    host: &SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    object: InlineObjectNeighbor,
    endpoint: SourcePosition,
    first: bool,
) -> Result<(), ComposerHostError> {
    let id = translation::marker_id(object.id());
    let existing = host
        .storage
        .draft_marker_identity(store, binding.root(), id)?
        .ok_or(ComposerHostError::MutationMalformed)?;
    if existing.marker_id() != id || u128::from(existing.order_key()) != object.order().get() {
        return Err(ComposerHostError::MutationMalformed);
    }
    let witness = DraftPieceMarkerAtV1::new(
        endpoint.byte_offset.get(),
        DraftPieceMarkerV1::new(
            id,
            existing.order_key(),
            existing.label(),
            existing.asset_id(),
        ),
    );
    if !host
        .storage
        .validate_draft_marker_location(store, binding.root(), witness)?
    {
        return Err(ComposerHostError::MutationMalformed);
    }
    if matches!(
        endpoint.gap,
        InlineObjectGap::Before(_) | InlineObjectGap::After(_)
    ) {
        let request = if first {
            DraftPieceMarkerEdgeProofRequestV1::First { marker: witness }
        } else {
            DraftPieceMarkerEdgeProofRequestV1::Last { marker: witness }
        };
        let expected = if first {
            DraftPieceMarkerEdgeProofV1::First { marker: witness }
        } else {
            DraftPieceMarkerEdgeProofV1::Last { marker: witness }
        };
        if host
            .storage
            .draft_piece_marker_edge_proof(store, binding.root(), request, 512)?
            != Some(expected)
        {
            return Err(ComposerHostError::MutationMalformed);
        }
    }
    Ok(())
}
