use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasTurnId,
    SyndicDraftId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    AcceptedInputAdmission, BindingState, ClaimCompactionDispatch, CompactionAdmissionRead,
    CompactionAttemptNonce, CompactionMarkerLifecycle, CompactionOperationId,
    CompactionOperationNonce, CompactionOperationRecord, CompactionProviderEvent,
    CompactionProviderSequence, CompactionRequestDisposition, CompactionThreadStatus, ComposerAtom,
    ComposerPayload, CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision, InputGateRecord,
    PreparedContent, PublishCompactionProviderEvent, PublishCompactionRequestDisposition,
    SealLifecycleContinuationContent, SourceEventPayload, SyndicPointReadLimit, SyndicStorage,
    TurnEndStatus, TurnTerminalOutcome, prepare_lifecycle_continuation_content,
};

use crate::support::{
    TestHome, converge_and_release_terminal_history, exact_cas, open, stage_prepared_content,
    timestamp,
};

pub fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean compaction fixture command, got {outcome:?}"),
    }
}

pub fn loaded_generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(72).unwrap(),
        CasLoadedThreadGeneration::new(1).unwrap(),
    )
}

pub struct CompactionFixture {
    pub home: TestHome,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub thread: SyndicThreadId,
    pub base_turn: SyndicTurnId,
    pub current_draft: SyndicDraftId,
}

impl CompactionFixture {
    pub fn new(name: &str, id_byte: u8) -> Self {
        let home = TestHome::new(name);
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let thread = SyndicThreadId::from_bytes([id_byte; 16]);
        let first_draft = SyndicDraftId::from_bytes([id_byte.wrapping_add(1); 16]);
        execute(
            &store,
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    first_draft,
                    crate::support::exact_cas::execution_binding(),
                    timestamp(1),
                ),
            ),
        );

        let current_draft = SyndicDraftId::from_bytes([id_byte.wrapping_add(2); 16]);
        let submitted_item = SyndicItemId::from_bytes([id_byte.wrapping_add(3); 16]);
        let base_turn = exact_cas::submit_current_draft(
            &store,
            storage,
            thread,
            current_draft,
            submitted_item,
            "compaction fixture base turn",
            timestamp(2),
        );
        let source = exact_cas::establish_turn(&store, storage, thread, base_turn, timestamp(3));
        exact_cas::admit_event(
            &store,
            storage,
            thread,
            base_turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(4),
        );
        exact_cas::correlate_user_item(
            &store,
            storage,
            thread,
            base_turn,
            submitted_item,
            &source,
            timestamp(5),
        );
        exact_cas::admit_event(
            &store,
            storage,
            thread,
            base_turn,
            &source,
            SourceEventPayload::TurnEnded(
                TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
            ),
            timestamp(6),
        );
        converge_and_release_terminal_history(&store, storage, thread, base_turn);
        let fixture = Self {
            home,
            store,
            storage,
            thread,
            base_turn,
            current_draft,
        };
        assert_eq!(
            fixture.gate().state(),
            &syndic_storage::InputGateState::Idle
        );
        assert!(matches!(fixture.binding_state(), BindingState::Valid(_)));
        fixture.store.validate_registered_domains().unwrap();
        fixture
    }

    pub fn gate(&self) -> InputGateRecord {
        self.storage
            .input_gate(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap()
    }

    pub fn binding_state(&self) -> BindingState {
        self.storage
            .current_binding(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .state()
            .clone()
    }

    pub fn operation(&self, id: CompactionOperationId) -> CompactionOperationRecord {
        self.storage
            .compaction_operation(&self.store, id, point_limit())
            .unwrap()
            .unwrap()
    }

    pub fn admit(&self, nonce_byte: u8, at: u64) -> CompactionOperationId {
        let CompactionAdmissionRead::Admissible(candidate) = self
            .storage
            .compaction_admission_read(&self.store, self.thread, point_limit())
            .unwrap()
        else {
            panic!("idle valid-binding fixture must admit compaction");
        };
        let admission = candidate.admission(
            CompactionOperationNonce::from_bytes([nonce_byte; 16]),
            CompactionAttemptNonce::from_bytes([nonce_byte.wrapping_add(1); 16]),
            loaded_generation(),
            timestamp(at),
        );
        let id = admission.operation_id();
        assert_eq!(admission.home_id(), self.store.home_id());
        match self.store
            .execute_current(self.storage.current_admit_compaction_operation(admission))
        {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean compaction admission, got {outcome:?}"),
        }
        assert_eq!(self.operation(id).home_id(), self.store.home_id());
        id
    }

    pub fn claim(&self, id: CompactionOperationId) {
        let operation = self.operation(id);
        match self.store
            .execute_current(self.storage.current_claim_compaction_dispatch(
                ClaimCompactionDispatch::new(id, operation.revision(), operation.attempt()),
            ))
        {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean compaction claim, got {outcome:?}"),
        }
    }

    pub fn publish_request(
        &self,
        id: CompactionOperationId,
        disposition: CompactionRequestDisposition,
    ) {
        let operation = self.operation(id);
        match self.store
            .execute_current(self.storage.current_publish_compaction_request_disposition(
                PublishCompactionRequestDisposition::new(
                    id,
                    operation.revision(),
                    operation.attempt(),
                    disposition,
                ),
            ))
        {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean compaction request publication, got {outcome:?}"),
        }
    }

    pub fn publish_provider(
        &self,
        id: CompactionOperationId,
        event: CompactionProviderEvent,
        at: u64,
    ) {
        let operation = self.operation(id);
        let sequence = operation
            .provider_frontier()
            .map_or(CompactionProviderSequence::FIRST, |frontier| {
                frontier.checked_next().unwrap()
            });
        match self.store
            .execute_current(self.storage.current_publish_compaction_provider_event(
                PublishCompactionProviderEvent::new(
                    id,
                    operation.revision(),
                    sequence,
                    event,
                    timestamp(at),
                ),
            ))
        {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean compaction provider publication, got {outcome:?}"),
        }
    }

    pub fn reopen(self) -> Self {
        let Self {
            home,
            store,
            storage: _,
            thread,
            base_turn,
            current_draft,
        } = self;
        drop(store);
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        Self {
            home,
            store,
            storage,
            thread,
            base_turn,
            current_draft,
        }
    }

    pub fn publish_success(&self, id: CompactionOperationId, at: u64) {
        self.publish_provider(
            id,
            CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Active),
            at,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::TurnStarted(
                CasTurnId::new(format!("compaction-{id:?}")).unwrap(),
            ),
            at + 1,
        );
        let marker = SyndicItemId::from_bytes([id.nonce().as_bytes()[0].wrapping_add(70); 16]);
        self.publish_provider(
            id,
            CompactionProviderEvent::Marker {
                item_id: marker,
                lifecycle: CompactionMarkerLifecycle::Started,
            },
            at + 2,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::Marker {
                item_id: marker,
                lifecycle: CompactionMarkerLifecycle::Completed,
            },
            at + 3,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Idle),
            at + 4,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::Terminal(
                TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
            ),
            at + 5,
        );
    }

    pub fn prepare_lifecycle_content(&self) -> syndic_storage::ContentReference {
        let prepared = prepare_lifecycle_continuation_content().unwrap();
        stage_prepared_content(&self.store, self.storage, &prepared);
        let manifest = self
            .storage
            .content_manifest(&self.store, prepared.id(), point_limit())
            .unwrap()
            .unwrap();
        match self.store
            .execute_current(self.storage.current_seal_lifecycle_continuation_content(
                SealLifecycleContinuationContent::new(manifest),
            ))
        {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean lifecycle content seal, got {outcome:?}"),
        }
        self.storage
            .content_manifest(&self.store, prepared.id(), point_limit())
            .unwrap()
            .unwrap()
            .sealed_reference()
            .unwrap()
    }

    pub fn admit_current_draft_as_accepted(
        &self,
        text: &str,
        next_draft_byte: u8,
        at: u64,
    ) -> AcceptedInputAdmission {
        let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
        let prepared = PreparedContent::composer(&payload).unwrap();
        stage_prepared_content(&self.store, self.storage, &prepared);
        let current = self
            .storage
            .current_draft(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let DraftPayloadUpdateDecision::Update(update) =
            DraftPayloadUpdate::prepare(&current, &prepared, timestamp(at)).unwrap()
        else {
            panic!("test draft must become nonempty");
        };
        execute(
            &self.store,
            self.storage
                .update_draft_payload(self.storage.revision(&self.store).unwrap(), update),
        );
        let current = self
            .storage
            .current_draft(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let gate = self.gate();
        let admission = AcceptedInputAdmission::new(
            self.thread,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            SyndicDraftId::from_bytes([next_draft_byte; 16]),
            None,
            timestamp(at),
        );
        execute(
            &self.store,
            self.storage.admit_accepted_input(
                self.storage.revision(&self.store).unwrap(),
                admission.clone(),
            ),
        );
        admission
    }
}
