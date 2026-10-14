use super::*;

pub(crate) fn prefix_ordinal(frontier: DraftPieceBuildFrontierV1) -> Option<u64> {
    match frontier {
        DraftPieceBuildFrontierV1::Applying {
            fragment_ordinal, ..
        }
        | DraftPieceBuildFrontierV1::Inserting {
            fragment_ordinal, ..
        } => Some(fragment_ordinal),
        _ => None,
    }
}

pub(crate) fn is_deferred(
    frontier: DraftPieceBuildFrontierV1,
    continuation: DraftPieceMarkerEffectContinuationV1,
) -> bool {
    prefix_ordinal(frontier)
        .is_some_and(|ordinal| ordinal < continuation.scan().next_fragment_ordinal())
}

pub(crate) fn active_is_deferred(
    frontier: DraftPieceBuildFrontierV1,
    active: DraftPieceActiveMarkerEffectV1,
) -> bool {
    prefix_ordinal(frontier).is_some_and(|ordinal| ordinal < active.fragment_key().ordinal())
}

pub(crate) fn is_prefix(fragment: &DraftPieceBuildFragmentV1) -> bool {
    let replacement = fragment.replacement();
    !replacement.is_continuation()
        && replacement.marker_effect().is_none()
        && replacement.inserted().is_empty()
        && replacement.start() != replacement.end()
}

pub(crate) fn is_removal_at(fragment: &DraftPieceBuildFragmentV1, anchor: u64) -> bool {
    let replacement = fragment.replacement();
    !replacement.is_continuation()
        && replacement.inserted().is_empty()
        && replacement.start() == replacement.end()
        && replacement.start().utf8_offset() == anchor
        && matches!(replacement.marker_effect(), Some(DraftPieceMarkerEffectV1::Remove { removal, .. })
            if removal.position() == replacement.start())
}
