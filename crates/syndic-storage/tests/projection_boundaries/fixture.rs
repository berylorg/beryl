use super::*;

pub(super) struct ProjectedFixture {
    pub(super) store: HomeStore,
    _home: TestHome,
    pub(super) storage: SyndicStorage,
    pub(super) item: SyndicItemId,
    pub(super) generation: ItemProjectionGeneration,
}

pub(super) fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean projection-boundary fixture command, got {outcome:?}"),
    }
}

pub(super) fn project_user_markdown(name: &str, text: &str) -> ProjectedFixture {
    let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
    project_user_payload(name, payload, None)
}

pub(super) fn project_user_payload(
    name: &str,
    payload: ComposerPayload,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
) -> ProjectedFixture {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(1);
    let draft = draft_id(2);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                crate::support::exact_cas::execution_binding(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );

    let content = PreparedContent::composer(&payload).unwrap();
    let item = SyndicItemId::from_bytes([4; 16]);
    let source = content
        .reference(ContentRevision::new(1).unwrap())
        .sealed_marker_summary()
        .unwrap();
    let asset_reference_set = asset_reference_set.or_else(|| {
        (source.sequential().marker_count() != 0).then(|| {
            SealedAssetReferenceSetProof::new(
                AssetReferenceSetId::from_bytes([5; 16]),
                source.sequential(),
                beryl_model::OrderedMarkerAssetSummaryV1::new(
                    [7; 32],
                    source.sequential().marker_count(),
                ),
                source.sequential().marker_count(),
                AssetReferenceSetDigest::from_bytes([6; 32]),
            )
            .unwrap()
        })
    });
    submit_prepared_current_draft(
        &store,
        storage.clone(),
        thread,
        draft_id(3),
        item,
        &content,
        asset_reference_set,
        timestamp(3),
    );

    let canonical = storage
        .canonical_item(&store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
    execute(
        &store,
        storage.start_item_projection_build(
            storage.revision(&store).unwrap(),
            StartItemProjectionBuild::new(item, canonical.revision(), generation),
        ),
    );
    for _ in 0..128 {
        if storage
            .item_projection_set(&store, item, generation, point_limit())
            .unwrap()
            .is_some()
        {
            return ProjectedFixture {
                store,
                _home: home,
                storage,
                item,
                generation,
            };
        }
        let build = storage
            .item_projection_build(&store, item, generation, point_limit())
            .unwrap()
            .unwrap();
        execute(
            &store,
            storage.advance_item_projection_build(
                storage.revision(&store).unwrap(),
                AdvanceItemProjectionBuild::new(item, generation, build.revision()),
            ),
        );
    }
    panic!("bounded Markdown projection did not finish");
}

pub(super) fn projections(fixture: &ProjectedFixture) -> Vec<syndic_storage::ProjectionRecord> {
    let mut output = Vec::new();
    let mut after = None;
    loop {
        let page = fixture
            .storage
            .item_projections(
                &fixture.store,
                fixture.item,
                fixture.generation,
                after,
                CursorReadLimits::new(256, TRANSCRIPT_PAGE_MAX_BYTES).unwrap(),
            )
            .unwrap();
        for index in page.records() {
            output.push(
                fixture
                    .storage
                    .projection(&fixture.store, index.projection_id(), point_limit())
                    .unwrap()
                    .unwrap()
                    .clone(),
            );
            after = Some(index.ordinal());
        }
        if !page.has_more() {
            return output;
        }
    }
}

fn single_projection(name: &str, markdown: &str) -> syndic_storage::ProjectionRecord {
    let fixture = project_user_markdown(name, markdown);
    let mut projected = projections(&fixture);
    assert_eq!(projected.len(), 1, "{name} must produce one block");
    projected.pop().unwrap()
}

pub(super) fn assert_inline_block(name: &str, markdown: &str, expected: MarkdownBlockKind) {
    let projection = single_projection(name, markdown);
    match projection.payload() {
        ProjectionPayload::InlineMarkdown {
            block_kind,
            source,
            source_range,
            ..
        } => {
            assert_eq!(*block_kind, expected);
            assert_eq!(&**source, markdown);
            assert_eq!(source_range.start(), 0);
            assert_eq!(source_range.end(), markdown.len() as u64);
        }
        payload => panic!("expected one inline {expected:?} block, got {payload:?}"),
    }
}

pub(super) fn assert_resource_block(name: &str, markdown: &str, expected: MarkdownBlockKind) {
    let projection = single_projection(name, markdown);
    match projection.payload() {
        ProjectionPayload::ResourceReference {
            block_kind,
            source_range,
            ..
        } => {
            assert_eq!(*block_kind, expected);
            assert!(source_range.end() <= markdown.len() as u64);
        }
        payload => panic!("expected one {expected:?} resource, got {payload:?}"),
    }
}
