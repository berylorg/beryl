use super::*;

fn converge_catalog_summary(
    store: &HomeStore,
    storage: SyndicStorage,
) -> ThreadCatalogSummaryRecord {
    let prepared = storage
        .prepare_thread_catalog_summary(store, id(30))
        .unwrap()
        .unwrap();
    if let ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) = prepared {
        execute(store, storage.rebuild_thread_catalog_summary(prepared));
    }
    match storage
        .prepare_thread_catalog_summary(store, id(30))
        .unwrap()
        .unwrap()
    {
        ThreadCatalogSummaryPreparation::ExactCurrent(exact) => exact.summary().clone(),
        ThreadCatalogSummaryPreparation::PreparedReplacement(_) => {
            panic!("one catalog rebuild must converge")
        }
    }
}

#[test]
fn accepted_replacement_rebuilds_the_selected_path_title_once_then_is_exact() {
    let home = TestHome::new("phase75-replacement-catalog-title");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, replacement_seed());

    let initial = converge_catalog_summary(&store, storage);
    assert!(initial.title().is_none());

    start_edit(storage, &store);
    let content = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text("Replacement title").unwrap()]).unwrap(),
    )
    .unwrap();
    stage_prepared_content(&store, storage, &content);
    let editing = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&editing, &content, timestamp(6)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => panic!("replacement payload must change"),
    };
    execute(
        &store,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    );

    let before_replacement = converge_catalog_summary(&store, storage);
    assert!(before_replacement.title().is_none());
    let current = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.submit_idle_draft(
            storage.revision(&store).unwrap(),
            IdleSubmission::new(
                id(30),
                current.thread().revision(),
                current.draft().id(),
                current.draft().revision(),
                current.draft().content(),
                InputGateRevision::new(1).unwrap(),
                draft_id(70),
                SyndicItemId::from_bytes([71; 16]),
                None,
                timestamp(7),
            ),
        ),
    );

    let prepared = storage
        .prepare_thread_catalog_summary(&store, id(30))
        .unwrap()
        .unwrap();
    let ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) = prepared else {
        panic!("selected-path replacement must invalidate the catalog summary")
    };
    let replacement_title = prepared.replacement().title().unwrap();
    assert_eq!(replacement_title.text(), "Replacement title");
    assert_eq!(
        replacement_title.source(),
        ThreadCatalogTitleSource::HistoryDerived,
    );
    assert_eq!(
        prepared.replacement().revision().get(),
        before_replacement.revision().get() + 1,
    );
    execute(&store, storage.rebuild_thread_catalog_summary(prepared));

    let exact = storage
        .prepare_thread_catalog_summary(&store, id(30))
        .unwrap()
        .unwrap();
    let ThreadCatalogSummaryPreparation::ExactCurrent(exact) = exact else {
        panic!("one replacement rebuild must converge to an exact no-op")
    };
    assert_eq!(exact.summary().title().unwrap().text(), "Replacement title");
    assert_eq!(
        exact.summary().revision().get(),
        before_replacement.revision().get() + 1,
    );
    store.validate_registered_domains().unwrap();
}
