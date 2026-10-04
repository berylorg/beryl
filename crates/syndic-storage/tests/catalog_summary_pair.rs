#![cfg(feature = "test-faults")]
mod support;

use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, MutationContribution,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
use beryl_state::{BerylState, InitializeThreadlessWindow};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, HistorySummaryRecord, SyndicPointReadLimit,
    SyndicStorage, SyndicTimestamp, ThreadCatalogSummaryPreparation,
    test_faults::{FixtureBatch, FixtureRecord},
};

fn thread(seed: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([seed; 16])
}
fn execute(store: &HomeStore, contribution: MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

struct Fixture {
    store: HomeStore,
    storage: SyndicStorage,
    state: BerylState,
    _directory: support::TestHome,
}
impl Fixture {
    fn new() -> Self {
        let directory = support::TestHome::new("catalog-summary-pair");
        println!(
            "owned catalog-summary pair fixture: {}",
            directory.path().display()
        );
        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(
                SyndicStorage::required_domains()
                    .unwrap()
                    .merge(BerylState::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        for seed in [1, 2] {
            execute(
                &store,
                storage.create_thread(
                    storage.revision(&store).unwrap(),
                    CreateThread::ordinary(
                        thread(seed),
                        SyndicDraftId::from_bytes([seed; 16]),
                        ExecutionBinding::new(
                            RuntimeId::from_bytes([3; 16]),
                            RootId::from_bytes([4; 16]),
                            RuntimeNativePath::from_admitted(
                                RuntimeMode::host(),
                                PathFlavor::Windows,
                                r"C:\Work\Beryl",
                            )
                            .unwrap(),
                        ),
                        SyndicTimestamp::from_unix_millis(u64::from(seed)),
                        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                    ),
                ),
            );
        }
        Self {
            store,
            storage,
            state,
            _directory: directory,
        }
    }
    fn source(&self, seed: u8) -> ThreadCatalogSummaryPreparation {
        self.storage
            .prepare_thread_catalog_summary(&self.store, thread(seed))
            .unwrap()
            .unwrap()
    }
    fn change_history_activity(&self) {
        let mut batch = FixtureBatch::new();
        for seed in [1, 2] {
            let old = self
                .storage
                .history_summary(
                    &self.store,
                    thread(seed),
                    SyndicPointReadLimit::new(65_536).unwrap(),
                )
                .unwrap()
                .unwrap();
            batch
                .put(FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                    thread(seed),
                    old.revision().checked_next().unwrap(),
                    old.thread_revision(),
                    old.committed_tail(),
                    old.selected_path_digest(),
                    old.complete(),
                    SyndicTimestamp::from_unix_millis(10 + u64::from(seed)),
                )))
                .unwrap();
        }
        execute(
            &self.store,
            self.storage
                .fixture_contribution(self.storage.revision(&self.store).unwrap(), batch),
        );
    }
}

#[test]
fn fixed_pair_rebuilds_both_summaries_atomically_and_rejects_duplicate_or_mixed_revisions() {
    let fixture = Fixture::new();
    let stale = fixture.source(1);
    fixture.change_history_activity();
    let first = fixture.source(1);
    let second = fixture.source(2);
    assert!(matches!(
        first,
        ThreadCatalogSummaryPreparation::PreparedReplacement(_)
    ));
    assert!(matches!(
        second,
        ThreadCatalogSummaryPreparation::PreparedReplacement(_)
    ));
    assert!(
        fixture
            .storage
            .publish_thread_catalog_summary_pair(first.clone(), Some(first.clone()))
            .is_err()
    );
    assert!(
        fixture
            .storage
            .publish_thread_catalog_summary_pair(stale, Some(second.clone()))
            .is_err()
    );
    let first_expected = match &first {
        ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => {
            prepared.replacement().clone()
        }
        _ => unreachable!(),
    };
    let second_expected = match &second {
        ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => {
            prepared.replacement().clone()
        }
        _ => unreachable!(),
    };
    execute(
        &fixture.store,
        fixture
            .storage
            .publish_thread_catalog_summary_pair(first, Some(second))
            .unwrap(),
    );
    let ThreadCatalogSummaryPreparation::ExactCurrent(first) = fixture.source(1) else {
        panic!("first summary not current")
    };
    let ThreadCatalogSummaryPreparation::ExactCurrent(second) = fixture.source(2) else {
        panic!("second summary not current")
    };
    assert_eq!(first.summary(), &first_expected);
    assert_eq!(second.summary(), &second_expected);
    let revision = fixture.storage.revision(&fixture.store).unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add_validation(
            fixture
                .storage
                .validate_thread_catalog_summary_pair(first, Some(second))
                .unwrap(),
        )
        .unwrap();
    command
        .add(fixture.state.session().initialize_threadless(
            fixture.state.session().revision(&fixture.store).unwrap(),
            InitializeThreadlessWindow::new(
                beryl_model::WindowId::from_bytes([7; 16]),
                beryl_model::WindowPlacement::new(
                    beryl_model::WindowBounds::new(0, 0, 900, 700).unwrap(),
                    beryl_model::WindowDisplayState::Normal,
                    None,
                    None,
                ),
            ),
        ))
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed { .. }
    ));
    assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), revision);
}

#[test]
fn canceled_pair_keeps_both_original_summaries() {
    let fixture = Fixture::new();
    fixture.change_history_activity();
    let first = fixture.source(1);
    let second = fixture.source(2);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let revision = fixture.store.home_revision().unwrap();
    let mut command = HomeCommand::new(revision).with_cancellation(cancellation);
    command
        .add(
            fixture
                .storage
                .publish_thread_catalog_summary_pair(first, Some(second))
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert!(matches!(
        fixture.source(1),
        ThreadCatalogSummaryPreparation::PreparedReplacement(_)
    ));
    assert!(matches!(
        fixture.source(2),
        ThreadCatalogSummaryPreparation::PreparedReplacement(_)
    ));
}
