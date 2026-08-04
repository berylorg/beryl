use super::*;

pub(super) fn target_asset_reference_set(
    content: ContentReference,
) -> SealedAssetReferenceSetProof {
    let source = content.sealed_marker_summary().unwrap();
    SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([24; 16]),
        source,
        source.marker_count(),
        AssetReferenceSetDigest::from_bytes([23; 32]),
    )
    .unwrap()
}

pub(super) fn target_marker_projection(
    item: SyndicItemId,
    turn: beryl_model::SyndicTurnId,
) -> ProjectionRecord {
    let marker = target_marker();
    let atom_ordinal = ComposerAtomOrdinal::new(2).unwrap();
    let marker_ordinal = InputMarkerOrdinal::FIRST;
    let source_offset = 8_u64;
    let ordinal = ProjectionOrdinal::new(2).unwrap();
    let payload =
        ProjectionPayload::image_marker(atom_ordinal, marker_ordinal, source_offset, marker);
    let mut hash = Sha256::new();
    hash.update(b"beryl/syndic/projection/v1\0");
    hash.update([1]);
    hash.update(item.as_bytes());
    hash.update(source_offset.to_be_bytes());
    hash.update(ordinal.get().to_be_bytes());
    hash.update([3]);
    hash.update(atom_ordinal.get().to_be_bytes());
    hash.update(marker_ordinal.get().to_be_bytes());
    hash.update(source_offset.to_be_bytes());
    hash.update(marker.marker_id().as_bytes());
    hash.update(marker.label().get().to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest[..16]);
    ProjectionRecord::new(
        SyndicProjectionId::from_bytes(id),
        ProjectionRevision::new(1).unwrap(),
        item,
        turn,
        ordinal,
        payload,
    )
}

pub(super) fn selected_source() -> SelectedPathProof {
    let source = source_turn();
    let digest = child_turn_chain_digest(source, root_turn(), root_turn_chain_digest(root_turn()));
    SelectedPathProof::new(Some(source), ThreadRevision::new(1).unwrap(), digest)
}

pub(super) fn target_entry() -> CurrentTranscriptEntryProof {
    CurrentTranscriptEntryProof::new(
        TranscriptGeneration::FIRST,
        TranscriptPosition::new(2).unwrap(),
    )
}
