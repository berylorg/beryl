use super::*;

pub struct ActiveStopFixture {
    pub _home: TestHome,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub thread: SyndicThreadId,
    pub turn: SyndicTurnId,
    pub item: SyndicItemId,
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
            item,
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
            item,
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
                execution_binding(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
    let current = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = current.binding().selected_path();
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap();
    execute(
        &store,
        storage.publish_valid_binding(
            storage.revision(&store).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                execution_binding(),
                CasThreadId::new("phase65-stop-compaction").unwrap(),
                represented,
                CasNativeTurnCount::ZERO,
                tool_profile(),
                lineage,
            ),
        ),
    );
    let CompactionAdmissionRead::Admissible(candidate) = storage
        .compaction_admission_read(&store, thread, point_limit())
        .unwrap()
    else {
        panic!("idle valid-binding fixture must admit compaction");
    };
    let compaction = candidate.admission(
        CompactionOperationNonce::from_bytes([103; 16]),
        CompactionAttemptNonce::from_bytes([104; 16]),
        loaded_generation(),
        timestamp(1),
    );
    let compaction_id = compaction.operation_id();
    assert_current(store.execute_current(storage.current_admit_compaction_operation(compaction)));
    let operation = storage
        .compaction_operation(&store, compaction_id, point_limit())
        .unwrap()
        .unwrap();
    assert_current(
        store.execute_current(storage.current_claim_compaction_dispatch(
            ClaimCompactionDispatch::new(compaction_id, operation.revision(), operation.attempt()),
        )),
    );
    publish_compaction_provider(
        &store,
        storage,
        compaction_id,
        CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Active),
        1,
    );
    publish_compaction_provider(
        &store,
        storage,
        compaction_id,
        CompactionProviderEvent::TurnStarted(CasTurnId::new("phase65-stop-compaction").unwrap()),
        1,
    );
    let marker = SyndicItemId::from_bytes([105; 16]);
    publish_compaction_provider(
        &store,
        storage,
        compaction_id,
        CompactionProviderEvent::Marker {
            item_id: marker,
            lifecycle: CompactionMarkerLifecycle::Started,
        },
        1,
    );
    publish_compaction_provider(
        &store,
        storage,
        compaction_id,
        CompactionProviderEvent::Marker {
            item_id: marker,
            lifecycle: CompactionMarkerLifecycle::Completed,
        },
        1,
    );
    publish_compaction_provider(
        &store,
        storage,
        compaction_id,
        CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Idle),
        1,
    );
    publish_compaction_provider(
        &store,
        storage,
        compaction_id,
        CompactionProviderEvent::Terminal(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        1,
    );
    let content = lifecycle_content(&store, storage);
    let operation = storage
        .compaction_operation(&store, compaction_id, point_limit())
        .unwrap()
        .unwrap();
    let settlement = SettleLifecycleCompaction::new(&operation, content, timestamp(1));
    let turn = settlement.turn_id();
    let item = settlement.item_id();
    assert_current(store.execute_current(storage.current_settle_lifecycle_compaction(settlement)));
    let source = establish_turn(&store, storage, thread, turn, timestamp(1));
    if publish_activation {
        admit_event(
            &store,
            storage,
            thread,
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(1),
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
    let operation_id = StopOperationId::new(thread, StopOperationNonce::from_bytes([106; 16]));
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
        item,
        target,
        operation_id,
        admission,
        source,
    }
}
