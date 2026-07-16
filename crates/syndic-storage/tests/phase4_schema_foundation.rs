use std::num::NonZeroU64;

use beryl_model::{AssetId, ContentRevision, SyndicDraftMarkerId};
use syndic_storage::{
    CanonicalItemPayload, ComposerAtom, ComposerPayload, ImageLabelOrdinal, PreparedContent,
    ResolvedImageMarker, SubmittedComposerAtom, SubmittedComposerPayload, SyndicRecordError,
};

fn marker(value: u8) -> SyndicDraftMarkerId {
    SyndicDraftMarkerId::from_bytes([value; 16])
}

fn asset(value: u8) -> AssetId {
    AssetId::sha256_v1([value; 32], NonZeroU64::new(u64::from(value)).unwrap())
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
fn submitted_payload_preserves_exact_order_and_resolves_every_marker() {
    let first_label = ImageLabelOrdinal::FIRST;
    let second_label = ImageLabelOrdinal::new(2).unwrap();
    let draft = ComposerPayload::new(vec![
        ComposerAtom::text("before").unwrap(),
        ComposerAtom::image_marker(marker(1), first_label),
        ComposerAtom::text("between").unwrap(),
        ComposerAtom::image_marker(marker(2), second_label),
    ])
    .unwrap();
    let first = ResolvedImageMarker::new(marker(1), first_label, asset(1));
    let second = ResolvedImageMarker::new(marker(2), second_label, asset(2));

    let submitted = SubmittedComposerPayload::resolve(&draft, vec![first, second]).unwrap();
    assert_eq!(submitted.utf8_bytes(), "beforebetween".len());
    assert_eq!(submitted.image_marker_count(), 2);
    assert!(matches!(
        submitted.atoms(),
        [
            SubmittedComposerAtom::Text(before),
            SubmittedComposerAtom::ImageMarker(actual_first),
            SubmittedComposerAtom::Text(between),
            SubmittedComposerAtom::ImageMarker(actual_second),
        ] if before.as_ref() == "before"
            && *actual_first == first
            && between.as_ref() == "between"
            && *actual_second == second
    ));

    let content = PreparedContent::composer(&draft).unwrap();
    let reference = content.reference(ContentRevision::new(1).unwrap());
    let canonical = CanonicalItemPayload::user_input(reference, 2);
    assert!(matches!(canonical, CanonicalItemPayload::UserInput { .. }));
    assert_eq!(canonical.content(), Some(reference));
    assert_eq!(canonical.marker_count(), 2);
}

#[test]
fn submitted_resolution_rejects_missing_extra_reordered_and_duplicate_facts() {
    let first_label = ImageLabelOrdinal::FIRST;
    let second_label = ImageLabelOrdinal::new(2).unwrap();
    let draft = ComposerPayload::new(vec![
        ComposerAtom::image_marker(marker(1), first_label),
        ComposerAtom::image_marker(marker(2), second_label),
    ])
    .unwrap();
    let first = ResolvedImageMarker::new(marker(1), first_label, asset(1));
    let second = ResolvedImageMarker::new(marker(2), second_label, asset(2));

    assert!(matches!(
        SubmittedComposerPayload::resolve(&draft, vec![first]),
        Err(SyndicRecordError::MarkerResolutionCountMismatch {
            expected: 2,
            actual: 1
        })
    ));
    assert!(matches!(
        SubmittedComposerPayload::resolve(&draft, vec![first, second, first]),
        Err(SyndicRecordError::MarkerResolutionCountMismatch {
            expected: 2,
            actual: 3
        })
    ));
    assert!(matches!(
        SubmittedComposerPayload::resolve(&draft, vec![second, first]),
        Err(SyndicRecordError::MarkerResolutionMismatch { atom_index: 0 })
    ));
    assert!(matches!(
        SubmittedComposerPayload::resolve(&draft, vec![first, first]),
        Err(SyndicRecordError::DuplicateImageMarker {
            kind: "submitted marker resolutions",
            marker_id,
        }) if marker_id == marker(1)
    ));
}

#[test]
fn repeated_label_requires_one_exact_asset_but_distinct_marker_ids() {
    let label = ImageLabelOrdinal::FIRST;
    let draft = ComposerPayload::new(vec![
        ComposerAtom::image_marker(marker(1), label),
        ComposerAtom::image_marker(marker(2), label),
    ])
    .unwrap();

    let submitted = SubmittedComposerPayload::resolve(
        &draft,
        vec![
            ResolvedImageMarker::new(marker(1), label, asset(1)),
            ResolvedImageMarker::new(marker(2), label, asset(1)),
        ],
    )
    .unwrap();
    assert_eq!(submitted.image_marker_count(), 2);

    assert_eq!(
        SubmittedComposerPayload::resolve(
            &draft,
            vec![
                ResolvedImageMarker::new(marker(1), label, asset(1)),
                ResolvedImageMarker::new(marker(2), label, asset(2)),
            ],
        )
        .unwrap_err(),
        SyndicRecordError::LabelAssetMismatch { label }
    );
}
