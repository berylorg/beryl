use super::*;
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, ContentRevision, OrderedMarkerAssetSummaryV1,
    SealedAssetReferenceSetProof, SyndicDraftMarkerId,
};

#[test]
fn earlier_selected_source_excludes_later_turn_but_inherits_parent_label_frontier() {
    let home = TestHome::new("discussion-inherited-prefix");
    let (store, storage) = seeded(&home);
    support::converge_and_release_terminal_history(&store, storage.clone(), id(30), source_turn());
    let label = ImageLabelOrdinal::new(37).unwrap();
    let content = PreparedContent::composer(
        &ComposerPayload::new(vec![
            ComposerAtom::text("later parent input").unwrap(),
            ComposerAtom::image_marker(SyndicDraftMarkerId::from_bytes([218; 16]), label),
        ])
        .unwrap(),
    )
    .unwrap();
    let markers = content
        .reference(ContentRevision::new(1).unwrap())
        .sealed_marker_summary()
        .unwrap();
    let assets = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([219; 16]),
        markers.sequential(),
        OrderedMarkerAssetSummaryV1::new([220; 32], markers.sequential().marker_count()),
        markers.sequential().marker_count(),
        AssetReferenceSetDigest::from_bytes([221; 32]),
    )
    .unwrap();
    let later = support::exact_cas::submit_prepared_current_draft(
        &store,
        storage.clone(),
        id(30),
        draft_id(222),
        SyndicItemId::from_bytes([223; 16]),
        &content,
        Some(assets),
        timestamp(90),
    );
    support::exact_cas::converge_items(&store, storage.clone(), id(30), later);
    support::exact_cas::converge_transcript(&store, storage.clone(), id(30));
    let prepared = prepare(&store, &storage);
    let child = prepared.intent().thread_id();
    assert!(matches!(
        store.execute(command(&store, prepared)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .thread(&store, id(30), limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(later)
    );
    assert_eq!(
        storage
            .thread(&store, child, limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(source_turn())
    );
    let labels = storage
        .image_label_authority_head(&store, child, limit())
        .unwrap()
        .unwrap();
    assert_eq!(labels.inherited(), ImageLabelFrontier::from_raw(37));
    assert_eq!(labels.permanent(), labels.inherited());
    assert_eq!(
        storage
            .draft_image_label_protection_head(&store, child, limit())
            .unwrap()
            .unwrap()
            .protected_maximum(),
        labels.inherited()
    );
    let origin = storage
        .resolve_image_label_origin_span(&store, child, label, limit())
        .unwrap()
        .unwrap();
    assert_eq!(origin.span().thread_id(), id(30));
    assert_eq!(origin.span().asset_reference_set(), assets);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
