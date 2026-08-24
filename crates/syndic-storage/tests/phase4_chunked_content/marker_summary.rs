use super::*;

#[test]
fn sealed_content_retains_exact_cross_domain_marker_summary_after_reopen() {
    let marker_free = PreparedContent::utf8("marker free")
        .unwrap()
        .reference(ContentRevision::new(1).unwrap())
        .sealed_marker_summary()
        .unwrap();
    assert_eq!(marker_free.sequential().marker_count(), 0);
    assert_eq!(
        marker_free.sequential().marker_digest(),
        content_marker_digest_seed()
    );
    assert_eq!(marker_free.sequential().maximum_image_label(), None);

    let home = TestHome::new("phase4-content-marker-summary");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();

    let marker_a = SyndicDraftMarkerId::from_bytes([103; 16]);
    let marker_b = SyndicDraftMarkerId::from_bytes([104; 16]);
    let label_a = ImageLabelOrdinal::new(27).unwrap();
    let label_b = ImageLabelOrdinal::new(100).unwrap();
    let payload = ComposerPayload::new(vec![
        ComposerAtom::image_marker(marker_a, label_a),
        ComposerAtom::text("between").unwrap(),
        ComposerAtom::image_marker(marker_b, label_b),
    ])
    .unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    let mut marker_digest = content_marker_digest_seed();
    marker_digest = advance_content_marker_digest(marker_digest, marker_a, label_a);
    marker_digest = advance_content_marker_digest(marker_digest, marker_b, label_b);
    let summary = content
        .reference(ContentRevision::new(2).unwrap())
        .sealed_marker_summary()
        .unwrap();
    assert_eq!(summary.content_id(), content.id());
    assert_eq!(summary.sequential().marker_count(), 2);
    assert_eq!(summary.sequential().marker_digest(), marker_digest);
    assert_eq!(summary.sequential().maximum_image_label(), Some(label_b));

    execute(
        &store,
        storage,
        storage.begin_content(
            storage.revision(&store).unwrap(),
            ContentBuild::from_prepared(&content),
        ),
    );
    let mut manifest = content.building_manifest();
    while let Some(next) = append_one_batch(&store, storage, &manifest, &content) {
        manifest = next;
    }
    manifest = seal_prepared_content(&store, storage, &manifest, &content);
    assert_eq!(manifest.lifecycle(), ContentLifecycle::Sealed);
    store.close().unwrap();

    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    let stored = reopened_storage
        .content_manifest(&reopened, content.id(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(stored.expected().maximum_image_label(), Some(label_b));
    assert_eq!(
        stored
            .sealed_reference()
            .unwrap()
            .sealed_marker_summary()
            .unwrap(),
        summary
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}
