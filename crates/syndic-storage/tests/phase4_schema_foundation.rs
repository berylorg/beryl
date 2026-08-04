use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, ContentRevision, SealedAssetReferenceSetProof,
    SyndicDraftMarkerId,
};
use syndic_storage::{
    CanonicalItemPresentation, ComposerAtom, ComposerPayload, ImageLabelOrdinal, PreparedContent,
    SyndicRecordError,
};

fn marker(value: u8) -> SyndicDraftMarkerId {
    SyndicDraftMarkerId::from_bytes([value; 16])
}

#[test]
fn image_label_ordinals_use_canonical_bijective_letters() {
    for (ordinal, expected) in [
        (1, "A"),
        (26, "Z"),
        (27, "AA"),
        (52, "AZ"),
        (53, "BA"),
        (702, "ZZ"),
        (703, "AAA"),
        (u64::MAX, "GKGWBYLWRXTLPO"),
    ] {
        assert_eq!(
            ImageLabelOrdinal::new(ordinal).unwrap().to_string(),
            expected
        );
    }
}

#[test]
fn composer_markers_have_nonzero_labels_and_unique_stable_identity() {
    assert!(ImageLabelOrdinal::new(0).is_err());
    let label = ImageLabelOrdinal::new(7).unwrap();
    let atom = ComposerAtom::image_marker(marker(1), label);
    let image = atom.image_marker_value().unwrap();
    assert_eq!(image.marker_id(), marker(1));
    assert_eq!(image.label(), label);

    let error = ComposerPayload::new(vec![
        atom,
        ComposerAtom::image_marker(marker(1), ImageLabelOrdinal::new(8).unwrap()),
    ])
    .unwrap_err();
    assert_eq!(
        error,
        SyndicRecordError::DuplicateImageMarker {
            kind: "composer payload",
            marker_id: marker(1),
        }
    );
}

#[test]
fn canonical_user_input_retains_one_complete_asset_reference_proof() {
    let first_label = ImageLabelOrdinal::FIRST;
    let second_label = ImageLabelOrdinal::new(2).unwrap();
    let draft = ComposerPayload::new(vec![
        ComposerAtom::text("before").unwrap(),
        ComposerAtom::image_marker(marker(1), first_label),
        ComposerAtom::text("between").unwrap(),
        ComposerAtom::image_marker(marker(2), second_label),
    ])
    .unwrap();
    let content = PreparedContent::composer(&draft).unwrap();
    let reference = content.reference(ContentRevision::new(1).unwrap());
    let source = reference.sealed_marker_summary().unwrap();
    let proof = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([3; 16]),
        source,
        source.marker_count(),
        AssetReferenceSetDigest::from_bytes([4; 32]),
    )
    .unwrap();
    let canonical = CanonicalItemPresentation::user_input(reference, Some(proof));
    assert!(matches!(
        canonical,
        CanonicalItemPresentation::UserInput { .. }
    ));
    assert_eq!(canonical.content(), Some(reference));
    assert_eq!(canonical.asset_reference_set(), Some(proof));
    assert_eq!(source.maximum_image_label(), Some(second_label));
}

#[test]
fn composer_content_accepts_more_than_1024_image_markers() {
    let atoms = (1_u64..=1_025)
        .map(|ordinal| {
            let mut identity = [0_u8; 16];
            identity[..8].copy_from_slice(&ordinal.to_be_bytes());
            ComposerAtom::image_marker(
                SyndicDraftMarkerId::from_bytes(identity),
                ImageLabelOrdinal::new(ordinal).unwrap(),
            )
        })
        .collect();
    let payload = ComposerPayload::new(atoms).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    assert_eq!(content.summary().image_marker_count(), 1_025);
    assert_eq!(
        content.summary().maximum_image_label(),
        Some(ImageLabelOrdinal::new(1_025).unwrap())
    );
}
