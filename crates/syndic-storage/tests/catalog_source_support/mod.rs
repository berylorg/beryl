use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore, MutationContribution};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, HistorySummaryRecord,
    PreparedThreadCatalogSummaryReplacement, SyndicPointReadLimit, SyndicStorage,
    ThreadCatalogSummaryPreparation, ThreadCatalogSummaryRecord,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

use crate::support::{TestHome, draft_id, id, open, timestamp};

pub struct Fixture {
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub home: TestHome,
}

impl Fixture {
    pub fn new(name: &str) -> Self {
        let home = TestHome::new(name);
        let mut candidate = open(home.path());
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        Self {
            store,
            storage,
            home,
        }
    }

    pub fn create(&self, seed: u8) {
        let binding = ExecutionBinding::new(
            RuntimeId::from_bytes([1; 16]),
            RootId::from_bytes([2; 16]),
            RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, r"C:\Work")
                .unwrap(),
        );
        self.execute(self.storage.create_thread(
            self.storage.revision(&self.store).unwrap(),
            CreateThread::ordinary(
                id(seed),
                draft_id(seed),
                binding,
                timestamp(u64::from(seed)),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ));
    }

    pub fn execute(&self, contribution: MutationContribution) {
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command.add(contribution).unwrap();
        assert!(matches!(
            self.store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    }

    pub fn batch(&self, batch: FixtureBatch) {
        self.execute(
            self.storage
                .fixture_contribution(self.storage.revision(&self.store).unwrap(), batch),
        );
    }

    pub fn remove_summary(&self, seed: u8) {
        let mut batch = FixtureBatch::new();
        batch
            .delete(FixtureDelete::ThreadCatalogSummary(id(seed)))
            .unwrap();
        self.batch(batch);
    }

    pub fn summary(&self, seed: u8) -> ThreadCatalogSummaryRecord {
        self.storage
            .thread_catalog_summary(
                &self.store,
                id(seed),
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap()
    }

    pub fn replacement(&self, seed: u8) -> PreparedThreadCatalogSummaryReplacement {
        let ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) = self
            .storage
            .prepare_thread_catalog_summary(&self.store, id(seed))
            .unwrap()
            .unwrap()
        else {
            panic!("summary requires repair")
        };
        prepared
    }

    pub fn change_activity(&self, seed: u8, at: u64) {
        let old = self
            .storage
            .history_summary(
                &self.store,
                id(seed),
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap();
        let mut batch = FixtureBatch::new();
        batch
            .put(FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                old.thread_id(),
                old.revision().checked_next().unwrap(),
                old.thread_revision(),
                old.committed_tail(),
                old.selected_path_digest(),
                old.complete(),
                timestamp(at),
            )))
            .unwrap();
        self.batch(batch);
    }
}
