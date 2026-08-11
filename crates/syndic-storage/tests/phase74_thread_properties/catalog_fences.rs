use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{JobId, SyndicThreadId};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::{
    ArchiveBranchDiscussionThread, ComposerAtom, ComposerPayload, ContentAppend, ContentBuild,
    CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision, PreparedContent,
    SyndicPointReadLimit, SyndicStorage, ThreadArchiveState, ThreadAttributesRevision,
    ThreadCatalogSummaryRecord,
};

use crate::support::{
    TestHome, batch, commit, draft_id, id, open, populated::populated_records, timestamp,
};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(
    store: &beryl_home_store::HomeStore,
    contribution: beryl_home_store::MutationContribution,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean catalog-fence command, got {outcome:?}"),
    }
}

fn stage_content(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    content: &PreparedContent,
) {
    execute(
        store,
        storage.begin_content(
            storage.revision(store).unwrap(),
            ContentBuild::from_prepared(content),
        ),
    );
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        let next = append.next_manifest().clone();
        execute(
            store,
            storage.append_content(storage.revision(store).unwrap(), append),
        );
        manifest = next;
    }
}

fn current_catalog(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
) -> ThreadCatalogSummaryRecord {
    storage
        .thread_catalog_summary(store, thread_id, limit())
        .unwrap()
        .unwrap()
}

#[test]
fn current_attributes_witness_validates_archive_despite_stale_history() {
    let home = TestHome::new("phase74-current-attributes-stale-history");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread_id = id(60);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread_id,
                draft_id(61),
                crate::support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    );
    let stale_catalog = current_catalog(&store, storage, thread_id);
    let draft = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    let payload = ComposerPayload::new(vec![ComposerAtom::text("new activity").unwrap()]).unwrap();
    let prepared = PreparedContent::composer(&payload).unwrap();
    stage_content(&store, storage, &prepared);
    let update = match DraftPayloadUpdate::prepare(&draft, &prepared, timestamp(2)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute(
        &store,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    );
    assert!(
        stale_catalog.sources().history_summary_revision()
            < storage
                .history_summary(&store, thread_id, limit())
                .unwrap()
                .unwrap()
                .revision()
    );

    let corrupt = ThreadCatalogSummaryRecord::new(
        stale_catalog.thread_id(),
        stale_catalog.revision().checked_next().unwrap(),
        stale_catalog.title().cloned(),
        stale_catalog.execution().clone(),
        ThreadArchiveState::BranchDiscussionOpen,
        stale_catalog.last_activity_at(),
        stale_catalog.complete(),
        stale_catalog.parent_thread_id(),
        stale_catalog.lineage_depth(),
        stale_catalog.lineage_digest(),
        stale_catalog.sources(),
    );
    commit(
        &store,
        storage,
        batch([FixtureRecord::ThreadCatalogSummary(corrupt)]),
    );
    assert!(
        store
            .validate_registered_domains()
            .unwrap_err()
            .to_string()
            .contains("catalog archive")
    );
}

#[test]
fn current_history_witness_validates_payload_despite_stale_attributes() {
    let home = TestHome::new("phase74-current-history-stale-attributes");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    let thread_id = id(36);
    let stale_catalog = current_catalog(&store, storage, thread_id);
    execute(
        &store,
        storage.archive_branch_discussion(
            storage.revision(&store).unwrap(),
            ArchiveBranchDiscussionThread::new(
                thread_id,
                ThreadAttributesRevision::FIRST,
                JobId::from_bytes([0x74; 16]),
                timestamp(20),
            ),
        ),
    );
    assert!(
        stale_catalog.sources().attributes_revision()
            < storage
                .thread_attributes(&store, thread_id, limit())
                .unwrap()
                .unwrap()
                .revision()
    );

    let corrupt = ThreadCatalogSummaryRecord::new(
        stale_catalog.thread_id(),
        stale_catalog.revision().checked_next().unwrap(),
        stale_catalog.title().cloned(),
        stale_catalog.execution().clone(),
        stale_catalog.archive(),
        timestamp(stale_catalog.last_activity_at().unix_millis() + 1),
        stale_catalog.complete(),
        stale_catalog.parent_thread_id(),
        stale_catalog.lineage_depth(),
        stale_catalog.lineage_digest(),
        stale_catalog.sources(),
    );
    commit(
        &store,
        storage,
        batch([FixtureRecord::ThreadCatalogSummary(corrupt)]),
    );
    assert!(
        store
            .validate_registered_domains()
            .unwrap_err()
            .to_string()
            .contains("catalog history payload")
    );
}
