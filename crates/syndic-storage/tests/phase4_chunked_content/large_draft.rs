use super::*;

#[test]
fn multi_million_token_scale_draft_stages_reopens_and_publishes_exactly() {
    let home = TestHome::new("phase4-large-content");
    let mut store = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(1);
    let draft = draft_id(2);
    create_thread(&store, storage, thread, draft);

    let payload = huge_boundary_payload();
    assert!(payload.utf8_bytes() > 10_000_000);
    let content = PreparedContent::composer(&payload).unwrap();
    assert!(content.chunks().len() > CONTENT_APPEND_MAX_CHUNKS);
    assert!(
        content
            .chunks()
            .iter()
            .all(|chunk| chunk.bytes().len() <= CONTENT_CHUNK_MAX_BYTES)
    );

    execute(
        &store,
        storage,
        storage.begin_content(
            storage.revision(&store).unwrap(),
            ContentBuild::from_prepared(&content),
        ),
    )
    .unwrap();
    let mut manifest = content.building_manifest();
    manifest = append_one_batch(&store, storage, &manifest, &content).unwrap();
    assert_eq!(manifest.lifecycle(), ContentLifecycle::Building);
    assert_eq!(
        read_composer_payload(
            &store,
            storage,
            &storage
                .current_draft(&store, thread, point_limit())
                .unwrap()
                .unwrap(),
        ),
        ComposerPayload::default()
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let stored = storage
        .content_manifest(&store, content.id(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(stored, manifest);
    while let Some(next) = append_one_batch(&store, storage, &manifest, &content) {
        manifest = next;
    }
    assert_eq!(manifest.expected(), content.summary());

    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(2)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute(
        &store,
        storage,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    )
    .unwrap();
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().content().id(), content.id());
    assert_eq!(read_composer_payload(&store, storage, &current), payload);
    assert_eq!(current.content().lifecycle(), ContentLifecycle::Sealed);
    assert_eq!(
        storage
            .draft(&store, draft, point_limit())
            .unwrap()
            .unwrap()
            .content()
            .id(),
        content.id()
    );

    let mut after = None;
    let mut observed_chunks = 0_u64;
    loop {
        let page = storage
            .content_chunks(
                &store,
                content.id(),
                after,
                CursorReadLimits::new(1, CONTENT_CHUNK_MAX_BYTES + 256).unwrap(),
            )
            .unwrap();
        assert!(page.records().len() <= 1);
        for chunk in page.records() {
            observed_chunks += 1;
            after = Some(chunk.ordinal());
        }
        if !page.has_more() {
            break;
        }
    }
    assert_eq!(observed_chunks, content.summary().chunk_count());

    let second_thread = id(3);
    create_thread(&store, storage, second_thread, draft_id(4));
    let second = storage
        .current_draft(&store, second_thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&second, &content, timestamp(3)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute(
        &store,
        storage,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    )
    .unwrap();
    let second = storage
        .current_draft(&store, second_thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(second.draft().content(), current.draft().content());

    let duplicate = execute(
        &store,
        storage,
        storage.begin_content(
            storage.revision(&store).unwrap(),
            ContentBuild::from_prepared(&content),
        ),
    )
    .unwrap_err();
    let CommandError::ContributorValidation { source, .. } = duplicate else {
        panic!("expected content identity rejection");
    };
    assert!(matches!(
        source.downcast_ref::<SyndicMutationError>(),
        Some(SyndicMutationError::ContentIdentityCollision)
    ));
    assert!(
        ContentAppend::prepare(current.content(), &content)
            .unwrap()
            .is_none()
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let current = storage
        .current_draft(&reopened, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(read_composer_payload(&reopened, storage, &current), payload);
    reopened.close().unwrap();
}
