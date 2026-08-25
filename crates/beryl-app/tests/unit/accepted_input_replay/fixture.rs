use std::num::NonZeroU64;

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, SidecarByteLimit,
    SidecarNamespace,
};
use beryl_model::{
    AssetId, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicDraftId, SyndicItemId, SyndicThreadId,
};
use beryl_state::{
    AssetMediaType, AssetOwner, BerylState, PublishAssetMetadata,
};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, FirstAcceptanceKind, ImageLabelOrdinal,
    SyndicPointReadLimit, SyndicTimestamp,
};

use super::*;
use super::submission_fixture::{Atom, submit_atoms};

pub(super) struct Fixture {
    _directory: tempfile::TempDir,
    pub(super) store: HomeStore,
    pub(super) storage: SyndicStorage,
    pub(super) state: BerylState,
    thread: SyndicThreadId,
    seed: u8,
}

impl Fixture {
    pub(super) fn new(seed: u8) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        Self::from_store(seed, directory, store)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn with_faults(
        seed: u8,
        faults: beryl_home_store::test_faults::FaultController,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults,
        )
        .unwrap();
        Self::from_store(seed, directory, store)
    }

    fn from_store(seed: u8, directory: tempfile::TempDir, mut store: HomeStore) -> Self {
        let storage = SyndicStorage::register(&mut store).unwrap();
        let state = BerylState::register(&mut store).unwrap();
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        execute_one(
            &store,
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    execution_binding(seed),
                    time(1),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        );
        Self {
            _directory: directory,
            store,
            storage,
            state,
            thread,
            seed,
        }
    }

    fn establish_pending_turn(&self) {
        let (kind, _) = submit_atoms(
            &self.store,
            self.storage,
            self.state.assets(),
            self.thread,
            SyndicDraftId::from_bytes([self.seed.wrapping_add(2); 16]),
            SyndicItemId::from_bytes([self.seed.wrapping_add(3); 16]),
            &[Atom::Text("initial turn")],
            self.seed.wrapping_add(20),
            time(3),
        );
        assert!(matches!(kind, FirstAcceptanceKind::Idle { .. }));
    }

    pub(super) fn accept_text(&self, text: &str) -> AcceptedInputRecord {
        self.establish_pending_turn();
        let (kind, source) = submit_atoms(
            &self.store,
            self.storage,
            self.state.assets(),
            self.thread,
            SyndicDraftId::from_bytes([self.seed.wrapping_add(4); 16]),
            SyndicItemId::from_bytes([self.seed.wrapping_add(5); 16]),
            &[Atom::Text(text)],
            self.seed.wrapping_add(30),
            time(5),
        );
        assert_eq!(kind, FirstAcceptanceKind::Accepted);
        self.storage
            .accepted_input(&self.store, source.accepted_input_id(), point_limit())
            .unwrap()
            .unwrap()
    }

    pub(super) fn replay_factory(
        &self,
        record: AcceptedInputRecord,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<AcceptedInputReplayFactory, AcceptedInputReplayError> {
        let owner_head = self
            .state
            .assets()
            .owner_head(&self.store, AssetOwner::AcceptedInput(record.id()))
            .unwrap();
        AcceptedInputReplayFactory::prepare(
            &self.store,
            self.storage,
            self.state.assets(),
            AcceptedInputReplayContext::new(
                self.store.home_id(),
                self.store.health().generation().unwrap(),
                RuntimeMode::host(),
            ),
            record,
            owner_head,
            cancellation,
        )
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn recover_same_home(self) -> Self {
        let Self {
            _directory,
            store,
            storage: _,
            state: _,
            thread,
            seed,
        } = self;
        let candidate = store.recover_same_home().unwrap();
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let store = candidate.publish();
        Self {
            _directory,
            store,
            storage,
            state,
            thread,
            seed,
        }
    }

    fn publish_asset(&self, bytes: &[u8]) -> AssetId {
        let sidecar = self
            .store
            .admit_sidecar(
                SidecarNamespace::new("images").unwrap(),
                bytes,
                SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap()),
            )
            .unwrap();
        let asset = AssetId::sha256_v1(
            sidecar.address().digest().as_bytes(),
            NonZeroU64::new(sidecar.address().length()).unwrap(),
        );
        let assets = self.state.assets();
        let revision = assets.revision(&self.store).unwrap();
        let metadata = assets
            .publish_metadata(
                revision,
                sidecar,
                PublishAssetMetadata::new(
                    asset,
                    AssetMediaType::new("image/png").unwrap(),
                    None,
                    revision.checked_next().unwrap(),
                ),
            )
            .unwrap();
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        metadata.add_to(&mut command).unwrap();
        match self.store.execute(command) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome @ CommandOutcome::Committed {
                later_failure: Some(_),
                ..
            } => panic!("asset metadata fixture command committed with later failure: {outcome:?}"),
            CommandOutcome::NotCommitted { evidence } => {
                panic!("asset metadata fixture command was not committed: {evidence:?}")
            }
            outcome @ CommandOutcome::Indeterminate { .. } => {
                panic!("asset metadata fixture command was indeterminate: {outcome:?}")
            }
        }
        asset
    }

    pub(super) fn accept_repeated_image(&self, leading_text: &str) -> AcceptedInputRecord {
        self.establish_pending_turn();
        let label = ImageLabelOrdinal::FIRST;
        let asset = self.publish_asset(b"\x89PNG\r\n\x1a\naccepted-input-replay");
        let (kind, source) = submit_atoms(
            &self.store,
            self.storage,
            self.state.assets(),
            self.thread,
            SyndicDraftId::from_bytes([self.seed.wrapping_add(4); 16]),
            SyndicItemId::from_bytes([self.seed.wrapping_add(5); 16]),
            &[
                Atom::Text(leading_text),
                Atom::Image(label, asset),
                Atom::Text(" between "),
                Atom::Image(label, asset),
                Atom::Text(" after"),
            ],
            self.seed.wrapping_add(30),
            time(5),
        );
        assert_eq!(kind, FirstAcceptanceKind::Accepted);
        self.storage
            .accepted_input(&self.store, source.accepted_input_id(), point_limit())
            .unwrap()
            .unwrap()
    }
}

pub(super) fn execute_one(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("fixture contribution command committed with later failure: {outcome:?}"),
        CommandOutcome::NotCommitted { evidence } => {
            panic!("fixture contribution command was not committed: {evidence:?}")
        }
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("fixture contribution command was indeterminate: {outcome:?}")
        }
    }
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub(super) fn time(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

pub(super) fn execution_binding(seed: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([seed; 16]),
        RootId::from_bytes([seed.wrapping_add(2); 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl-accepted-input-replay",
        )
        .unwrap(),
    )
}
