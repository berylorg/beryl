use super::*;

#[test]
fn multi_million_token_scale_content_stages_reopens_and_reads_exactly() {
    let home = TestHome::new("phase4-large-content");
    let mut store = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();

    let payload = huge_boundary_payload();
    assert!(payload.utf8_bytes() > 10_000_000);
    let content = PreparedContent::composer(&payload).unwrap();
    assert!(content.chunks().len() > CONTENT_APPEND_MAX_CHUNKS);
    assert!(content
        .chunks()
        .iter()
        .all(|chunk| chunk.bytes().len() <= CONTENT_CHUNK_MAX_BYTES));

    execute(
        &store,
        storage,
        storage.begin_content(
            storage.revision(&store).unwrap(),
            ContentBuild::from_prepared(&content),
        ),
    );
    let mut manifest = content.building_manifest();
    manifest = append_one_batch(&store, storage, &manifest, &content).unwrap();
    assert_eq!(manifest.lifecycle(), ContentLifecycle::Building);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    while let Some(next) = append_one_batch(&store, storage, &manifest, &content) {
        manifest = next;
    }
    manifest = seal_prepared_content(&store, storage, &manifest, &content);
    assert_eq!(manifest.expected(), content.summary());
    let sealed = manifest.sealed_reference().unwrap();
    assert_eq!(sealed.id(), content.id());
    let mut assembler = ComposerContentAssembler::new(sealed).unwrap();

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
            assembler.push(chunk).unwrap();
        }
        if !page.has_more() {
            break;
        }
    }
    assert_eq!(observed_chunks, content.summary().chunk_count());
    assert_eq!(assembler.finish().unwrap(), payload);

    let duplicate = execute_outcome(
        &store,
        storage.begin_content(
            storage.revision(&store).unwrap(),
            ContentBuild::from_prepared(&content),
        ),
    );
    let CommandOutcome::NotCommitted {
        evidence: duplicate,
    } = duplicate
    else {
        panic!("expected rejected duplicate content command, got {duplicate:?}");
    };
    let CommandError::ContributorValidation { source, .. } = duplicate else {
        panic!("expected content identity rejection");
    };
    assert!(matches!(
        source.downcast_ref::<SyndicMutationError>(),
        Some(SyndicMutationError::ContentIdentityCollision)
    ));
    assert!(ContentAppend::prepare(&manifest, &content)
        .unwrap()
        .is_none());
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let stored = storage
        .content_manifest(&reopened, content.id(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(stored, manifest);
    assert_eq!(stored.sealed_reference(), Some(sealed));
    reopened.close().unwrap();
}
