use super::{edit_support::commit_edit, publication_support::*, support::*};
use beryl_model::{SyndicItemId, SyndicTurnId};
use syndic_storage::{
    DRAFT_COMPOSER_INPUT_MAX_BYTES, DRAFT_COMPOSER_READ_MAX_RECORDS,
    DRAFT_COMPOSER_RESIDENT_MAX_BYTES, DRAFT_COMPOSER_WRITE_MAX_RECORDS, DraftComposerBuildKeyV1,
    DraftComposerFormatV1, DraftComposerMaterializationOperationIdV1,
    DraftComposerMaterializationRecordV1, DraftComposerMaterializationStatusV1, FirstAcceptance,
    FirstAcceptanceKind, FirstAcceptanceStatus, InputGateRecord, InputGateState,
    test_faults::{FixtureBatch, FixtureRecord},
};

pub(super) struct Fixture {
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub faults: FaultController,
    pub thread: SyndicThreadId,
    pub source: DraftEditorCandidateSessionV1,
    pub acceptance: FirstAcceptance,
    pub queued: bool,
    pub home: TestHome,
}

impl Fixture {
    pub fn new(name: &str, seed: u8, queued: bool) -> Self {
        Self::build(name, seed, queued, None)
    }

    #[allow(dead_code)]
    pub fn opening(name: &str, seed: u8, queued: bool, populated: bool) -> Self {
        Self::build(name, seed, queued, Some(populated))
    }

    fn build(name: &str, seed: u8, queued: bool, opening: Option<bool>) -> Self {
        let (home, mut store, mut storage, faults, thread) = fault_fixture(name, seed, 65_536);
        let durable = current(&storage, &store, thread);
        let source = if opening == Some(false) {
            None
        } else {
            let opened = open_session(&storage, &store, &durable, seed + 2, seed + 3);
            let edit = commit_edit(&storage, &store, &opened, seed + 4, "submitted checkpoint");
            publish_candidate(&storage, &store, &durable, edit.adopted_session(), seed + 5);
            Some(head(&storage, &store, edit.adopted_session()))
        };
        let source = if let Some(populated) = opening {
            let durable = current(&storage, &store, thread);
            assert_eq!(
                durable.draft().history().candidate_generation() > 0,
                populated
            );
            drop(store);
            store = open(&home);
            storage = SyndicStorage::register(&mut store).unwrap();
            let opened = open_session(&storage, &store, &durable, seed + 11, seed + 12);
            assert_eq!(opened.newest_root(), opened.published_root());
            assert_ne!(opened.newest_history(), opened.published_history());
            opened
        } else {
            let source = source.unwrap();
            assert_eq!(source.newest_root(), source.published_root());
            assert_eq!(source.newest_history(), source.published_history());
            source
        };
        let materialization = materialize(&storage, &store, source.newest_root(), seed + 6);
        let selected = current(&storage, &store, thread);
        let gate = storage
            .input_gate(&store, thread, read_limit())
            .unwrap()
            .unwrap();
        if queued {
            let gate = InputGateRecord::new(
                thread,
                gate.revision().checked_next().unwrap(),
                InputGateState::PendingTurn(SyndicTurnId::from_bytes([seed + 7; 16])),
                0,
                None,
                None,
                0,
                0,
                0,
            )
            .unwrap();
            let mut batch = FixtureBatch::new();
            batch.put(FixtureRecord::InputGate(gate)).unwrap();
            committed(execute(
                &store,
                storage.fixture_contribution(storage.revision(&store).unwrap(), batch),
            ));
        }
        let gate = storage
            .input_gate(&store, thread, read_limit())
            .unwrap()
            .unwrap();
        let authority = storage
            .image_label_authority_head(&store, thread, read_limit())
            .unwrap()
            .unwrap();
        let acceptance = FirstAcceptance::new(
            thread,
            selected.thread().revision(),
            authority,
            selected.draft().id(),
            selected.draft().revision(),
            DraftEditorCandidateActivationBindingV1::from_head(&source),
            materialization,
            gate.revision(),
            gate.state().clone(),
            SyndicDraftId::from_bytes([seed + 8; 16]),
            SyndicItemId::from_bytes([seed + 9; 16]),
            None,
            DraftPieceOperationIdV1::from_bytes([seed + 10; 16]),
            SyndicTimestamp::from_unix_millis(20),
        );
        Self {
            store,
            storage,
            faults,
            thread,
            source,
            acceptance,
            queued,
            home,
        }
    }

    pub fn accept(&self) -> CommandOutcome {
        execute(
            &self.store,
            self.storage.first_acceptance(
                self.storage.revision(&self.store).unwrap(),
                self.acceptance.clone(),
            ),
        )
    }

    pub fn expected_status(&self) -> FirstAcceptanceStatus {
        FirstAcceptanceStatus::ExactNew(if self.queued {
            FirstAcceptanceKind::Accepted
        } else {
            FirstAcceptanceKind::Idle {
                user_item_id: self.acceptance.idle_user_item_id(),
            }
        })
    }

    pub fn recover_if_failed(mut self) -> Self {
        let (store, storage) = recover_if_failed(self.store, self.storage);
        self.store = store;
        self.storage = storage;
        self
    }

    pub fn reopen(mut self) -> Self {
        drop(self.store);
        self.store = open(&self.home);
        self.storage = SyndicStorage::register(&mut self.store).unwrap();
        self
    }
}

pub(super) fn read_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

fn materialize(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    seed: u8,
) -> DraftComposerMaterializationRecordV1 {
    let key = DraftComposerBuildKeyV1::new(
        root,
        DraftComposerFormatV1::ComposerV1,
        DraftComposerMaterializationOperationIdV1::from_bytes([seed; 16]),
    );
    committed(execute(
        store,
        storage.begin_draft_composer_materialization(storage.revision(store).unwrap(), key),
    ));
    for _ in 0..4096 {
        if let DraftComposerMaterializationStatusV1::Sealed(mapping) = storage
            .draft_composer_materialization_status(store, key)
            .unwrap()
        {
            return mapping;
        }
        let step = storage
            .prepare_draft_composer_materialization_step(store, key)
            .unwrap()
            .unwrap();
        assert!(step.records_read() <= DRAFT_COMPOSER_READ_MAX_RECORDS);
        assert!(step.input_payload_bytes() <= DRAFT_COMPOSER_INPUT_MAX_BYTES);
        assert!(step.written_record_count() <= DRAFT_COMPOSER_WRITE_MAX_RECORDS);
        assert!(step.resident_bytes() <= DRAFT_COMPOSER_RESIDENT_MAX_BYTES);
        committed(execute(
            store,
            storage.advance_draft_composer_materialization(storage.revision(store).unwrap(), step),
        ));
    }
    panic!(" materialization exceeded bounded work");
}
