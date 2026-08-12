use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{SyndicDraftId, SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    AcceptedInputAdmission, AdmitStopOperation, BindingState, ComposerAtom, ComposerPayload,
    CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision, PreparedContent,
    SourceEventPayload, StopCause, StopCauseSet, StopOperationId, StopOperationNonce,
    StopOperationRecord, StopOperationTarget, SyndicPointReadLimit, SyndicStorage,
};

use crate::support::{TestHome, exact_cas, open, stage_prepared_content, timestamp};

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
        outcome => panic!("expected clean stop fixture command, got {outcome:?}"),
    }
}

pub struct ActiveStopFixture {
    pub _home: TestHome,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub thread: SyndicThreadId,
    pub turn: SyndicTurnId,
    pub target: StopOperationTarget,
    pub operation_id: StopOperationId,
    pub admission: AdmitStopOperation,
    pub source: syndic_storage::CasTurnSource,
}

impl ActiveStopFixture {
    pub fn gate(&self) -> syndic_storage::InputGateRecord {
        self.storage
            .input_gate(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap()
    }

    pub fn stop(&self) -> StopOperationRecord {
        self.storage
            .stop_operation(&self.store, self.operation_id, point_limit())
            .unwrap()
            .unwrap()
    }

    pub fn admit_stop(&self) {
        match self.store.execute_current(
            self.storage
                .current_admit_stop_operation(self.admission.clone()),
        ) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean stop admission, got {outcome:?}"),
        }
    }

    pub fn reopen(self) -> Self {
        let Self {
            _home,
            store,
            storage: _,
            thread,
            turn,
            target,
            operation_id,
            admission,
            source,
        } = self;
        drop(store);
        let mut store = open(_home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        Self {
            _home,
            store,
            storage,
            thread,
            turn,
            target,
            operation_id,
            admission,
            source,
        }
    }
}

pub fn active_stop_fixture(name: &str) -> ActiveStopFixture {
    let home = TestHome::new(name);
    let store = open(home.path());
    build_active_stop_fixture(home, store, true)
}

pub fn pending_stop_fixture(name: &str) -> ActiveStopFixture {
    let home = TestHome::new(name);
    let store = open(home.path());
    build_active_stop_fixture(home, store, false)
}

#[cfg(feature = "test-faults")]
pub fn active_stop_fixture_with_faults(
    name: &str,
    faults: beryl_home_store::test_faults::FaultController,
) -> ActiveStopFixture {
    let home = TestHome::new(name);
    let store = HomeStore::open_with_faults(
        beryl_home_store::HomeOpenOptions::new(
            home.path(),
            beryl_home_store::HomeSchemaVersion::CURRENT,
        ),
        faults,
    )
    .unwrap();
    build_active_stop_fixture(home, store, true)
}

fn build_active_stop_fixture(
    home: TestHome,
    mut store: HomeStore,
    publish_activation: bool,
) -> ActiveStopFixture {
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([101; 16]);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([102; 16]),
                crate::support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    );
    let turn = exact_cas::submit_current_draft(
        &store,
        storage,
        thread,
        SyndicDraftId::from_bytes([103; 16]),
        SyndicItemId::from_bytes([104; 16]),
        "active stop target",
        timestamp(2),
    );
    let source = exact_cas::establish_turn(&store, storage, thread, turn, timestamp(3));
    if publish_activation {
        exact_cas::admit_event(
            &store,
            storage,
            thread,
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(4),
        );
    }

    let current = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = current.binding().state() else {
        panic!("fixture binding must be active");
    };
    let snapshot = storage
        .execution_snapshot(&store, active.snapshot_id(), point_limit())
        .unwrap()
        .unwrap();
    let cas_turn = storage
        .active_cas_turn(&store, active.snapshot_id(), point_limit())
        .unwrap()
        .unwrap();
    let turn_record = storage.turn(&store, turn, point_limit()).unwrap().unwrap();
    let target = StopOperationTarget::new(
        thread,
        turn,
        turn_record.kind(),
        current.binding().revision(),
        snapshot.id(),
        snapshot.execution().runtime_id(),
        snapshot.loaded_generation(),
        cas_turn.cas_thread_id().clone(),
        cas_turn.cas_turn_id().clone(),
    );
    let operation_id = StopOperationId::new(thread, StopOperationNonce::from_bytes([105; 16]));
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let admission = AdmitStopOperation::new(
        operation_id,
        target.clone(),
        gate.revision(),
        gate.selected_route().unwrap(),
        StopCauseSet::from(StopCause::SelectedOperationControl),
    );
    ActiveStopFixture {
        _home: home,
        store,
        storage,
        thread,
        turn,
        target,
        operation_id,
        admission,
        source,
    }
}

pub fn admit_current_draft_as_accepted(
    fixture: &ActiveStopFixture,
    text: &str,
    next_draft_byte: u8,
    at: u64,
) -> AcceptedInputAdmission {
    let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
    let prepared = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(&fixture.store, fixture.storage, &prepared);
    let current = fixture
        .storage
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &prepared, timestamp(at)).unwrap()
    else {
        panic!("test draft must become nonempty");
    };
    execute(
        &fixture.store,
        fixture
            .storage
            .update_draft_payload(fixture.storage.revision(&fixture.store).unwrap(), update),
    );
    let current = fixture
        .storage
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture.gate();
    let admission = AcceptedInputAdmission::new(
        fixture.thread,
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
        &fixture.store,
        fixture.storage.admit_accepted_input(
            fixture.storage.revision(&fixture.store).unwrap(),
            admission.clone(),
        ),
    );
    admission
}
