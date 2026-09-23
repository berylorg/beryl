use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::JobId;
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::{
    DiscussionHandoffMutation, SyndicPointReadLimit, SyndicStorage, ThreadAttributesRevision,
    ThreadCatalogSummaryRecord,
};

use crate::support::{TestHome, batch, commit, id, open, seed_populated, timestamp};

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

fn current_catalog(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
) -> ThreadCatalogSummaryRecord {
    storage
        .thread_catalog_summary(store, id(36), limit())
        .unwrap()
        .unwrap()
}

#[test]
fn stale_attributes_witness_cannot_justify_a_changed_catalog_payload() {
    let home = TestHome::new("current-history-stale-attributes");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let store = store
        .prepare_publication(syndic_storage::SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_populated(&store, storage.clone());
    crate::support::discussion_handoff::admit(
        &store,
        &storage,
        beryl_model::ResolutionIntentId::from_bytes([0x73; 16]),
        JobId::from_bytes([0x74; 16]),
    );
    crate::support::discussion_handoff::complete_resolving_turn(&store, &storage);
    if let syndic_storage::ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) = storage
        .prepare_thread_catalog_summary(&store, id(36))
        .unwrap()
        .unwrap()
    {
        execute(&store, storage.rebuild_thread_catalog_summary(prepared));
    }
    let stale_catalog = current_catalog(&store, storage.clone());
    execute(
        &store,
        storage
            .prepare_discussion_handoff(
                &store,
                DiscussionHandoffMutation::ReleaseAndArchive {
                    expected: storage
                        .discussion_handoff_gate(&store, id(36), limit())
                        .unwrap()
                        .unwrap(),
                    attributes_revision: ThreadAttributesRevision::FIRST,
                    archived_at: timestamp(20),
                },
            )
            .unwrap()
            .contribution(),
    );
    assert!(
        stale_catalog.sources().attributes_revision()
            < storage
                .thread_attributes(&store, id(36), limit())
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
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .to_string()
            .contains("catalog history payload")
    );
}
