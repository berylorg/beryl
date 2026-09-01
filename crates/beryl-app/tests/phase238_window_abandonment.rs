use std::{sync::Arc, time::Duration};

use beryl_app::{
    catalog_projection::{ThreadCatalogProjectionPreparation, prepare_thread_catalog_projection},
    window_acquisition::{
        RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome,
        RuntimeBackedWindowAbandonmentNotCommitted, RuntimeBackedWindowAbandonmentOutcome,
        RuntimeBackedWindowAbandonmentPreparationOutcome,
        RuntimeBackedWindowAbandonmentReconciliationOutcome,
        RuntimeBackedWindowAcquisitionDisposition, RuntimeBackedWindowAcquisitionOutcome,
        RuntimeBackedWindowAcquisitionRequest, RuntimeBackedWindowAcquisitionService,
        RuntimeBackedWindowProcessRegistry,
    },
};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion,
    HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    AdmittedHostPath, Availability, CasThreadId, CasTurnId, DynamicToolCallId, ExecutionBinding,
    PathFlavor, ResolutionIntentId, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicDraftId, SyndicThreadId, SyndicTurnId, WindowBounds, WindowDisplayState, WindowId,
    WindowPlacement,
};
use beryl_state::{
    AdmitBranchHandoffJob, AvailabilitySnapshot, BerylState, BranchHandoffJobAdmission,
    CatalogClaimSummary, CatalogPointReadLimit, CreateRuntimeWithHomeRoot, DiscussionContextDigest,
    DiscussionContextOwnerId, InitializeThreadlessWindow, MarkCatalogRowStale, ParentQueueOrdinal,
    RememberedTarget, RemoveSessionWindow, ResolutionAttemptOrdinal, ResolutionRequestIdentity,
    ResolutionText, RootRegistration, RuntimeRegistration, UnixMillis,
};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, PristineThreadAudit, SyndicStorage, SyndicTimestamp,
};

struct Fixture {
    _directory: tempfile::TempDir,
    store: Arc<HomeStore>,
    state: BerylState,
    syndic: SyndicStorage,
    process: RuntimeBackedWindowProcessRegistry,
    service: RuntimeBackedWindowAcquisitionService,
    faults: FaultController,
    runtime_id: RuntimeId,
    root_id: RootId,
    execution: ExecutionBinding,
}

impl Fixture {
    fn new(seed: u8) -> Self {
        let directory = tempfile::tempdir().expect("temp home");
        let faults = FaultController::new();
        let mut store = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .expect("open home");
        let state = BerylState::register(&mut store).expect("register Beryl state");
        let syndic = SyndicStorage::register(&mut store).expect("register Syndic");
        let runtime_id = RuntimeId::from_bytes([seed; 16]);
        let root_id = RootId::from_bytes([seed.wrapping_add(1); 16]);
        let mode = RuntimeMode::host();
        let root_path = native_path(mode.clone(), r"C:\Work\Beryl");
        let runtime = RuntimeRegistration::new(
            runtime_id,
            host_path(r"C:\Program Files\Codex\codex.exe"),
            mode.clone(),
            native_path(mode, r"C:\Program Files\Codex\codex.exe"),
            UnixMillis::new(1),
            AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2))
                .expect("runtime availability"),
        )
        .expect("runtime registration");
        let root = RootRegistration::new(
            root_id,
            root_path.clone(),
            host_path(r"C:\Work\Beryl"),
            UnixMillis::new(1),
            AvailabilitySnapshot::unknown(),
        );
        execute_contribution(
            &store,
            state.runtime_roots().create_runtime_with_home_root(
                state
                    .runtime_roots()
                    .revision(&store)
                    .expect("runtime revision"),
                CreateRuntimeWithHomeRoot::new(runtime, root).expect("matching runtime/root"),
            ),
        );
        initialize_empty_session(&store, &state);
        let execution = ExecutionBinding::new(runtime_id, root_id, root_path);
        let store = Arc::new(store);
        let process = RuntimeBackedWindowProcessRegistry::new();
        let service = RuntimeBackedWindowAcquisitionService::new(
            &process,
            Arc::clone(&store),
            state.clone(),
            syndic.clone(),
        );
        Self {
            _directory: directory,
            store,
            state,
            syndic,
            process,
            service,
            faults,
            runtime_id,
            root_id,
            execution,
        }
    }

    fn request(&self, seed: u8) -> RuntimeBackedWindowAcquisitionRequest {
        RuntimeBackedWindowAcquisitionRequest::new(
            WindowId::from_bytes([seed; 16]),
            RememberedTarget::new(self.runtime_id, self.root_id),
            placement(),
            SyndicThreadId::from_bytes([seed.wrapping_add(1); 16]),
            SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]),
            self.execution.clone(),
            SyndicTimestamp::from_unix_millis(u64::from(seed) + 10),
            history_policy(),
        )
        .expect("matching acquisition request")
    }

    fn acquire(&self, seed: u8) -> beryl_app::window_acquisition::RuntimeBackedWindowAcquisition {
        let outcome = self
            .service
            .acquire(self.request(seed), CommandCancellation::new());
        let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = outcome else {
            panic!("acquisition must commit: {outcome:?}")
        };
        acquisition
    }

    fn prepare(
        &self,
        acquisition: beryl_app::window_acquisition::RuntimeBackedWindowAcquisition,
    ) -> beryl_app::window_acquisition::RuntimeBackedWindowAbandonment {
        let outcome = self
            .service
            .prepare_abandonment(acquisition, CommandCancellation::new());
        let RuntimeBackedWindowAbandonmentPreparationOutcome::ExactAcquired { abandonment } =
            outcome
        else {
            panic!("exact acquisition must enter abandonment custody: {outcome:?}")
        };
        abandonment
    }

    fn publish_pristine_thread(&self, thread_seed: u8, created_at: u64) -> SyndicThreadId {
        let thread_id = SyndicThreadId::from_bytes([thread_seed; 16]);
        execute_contribution(
            &self.store,
            self.syndic.create_thread(
                self.syndic.revision(&self.store).expect("Syndic revision"),
                CreateThread::ordinary(
                    thread_id,
                    SyndicDraftId::from_bytes([thread_seed.wrapping_add(1); 16]),
                    self.execution.clone(),
                    SyndicTimestamp::from_unix_millis(created_at),
                    history_policy(),
                ),
            ),
        );
        let preparation =
            prepare_thread_catalog_projection(&self.store, &self.syndic, &self.state, thread_id)
                .expect("prepare catalog projection");
        let ThreadCatalogProjectionPreparation::Publish(command) = preparation else {
            panic!("new thread requires catalog publication")
        };
        execute_command(&self.store, command);
        thread_id
    }

    fn mark_catalog_row_stale(&self, thread_id: SyndicThreadId) {
        let row = self
            .state
            .catalog()
            .row(
                &self.store,
                thread_id,
                CatalogPointReadLimit::schema_maximum(),
            )
            .expect("catalog row read")
            .expect("catalog row");
        execute_contribution(
            &self.store,
            self.state.catalog().mark_stale(
                self.state
                    .catalog()
                    .revision(&self.store)
                    .expect("catalog revision"),
                MarkCatalogRowStale::new(thread_id, row.revision()),
            ),
        );
    }
}

#[test]
fn both_origins_apply_the_exact_atomic_abandonment_rule() {
    let created = Fixture::new(10);
    let acquisition = created.acquire(20);
    let thread_id = acquisition.thread_id();
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Created
    );
    let abandonment = created.prepare(acquisition);
    let seed = abandonment.audit_seed();
    assert!(matches!(
        created
            .service
            .abandon(abandonment, CommandCancellation::new()),
        RuntimeBackedWindowAbandonmentOutcome::Committed { .. }
    ));
    assert!(
        created
            .state
            .session()
            .minimal_bootstrap(&created.store)
            .expect("session")
            .expect("initialized")
            .windows()
            .is_empty()
    );
    assert!(
        created
            .state
            .catalog()
            .row(
                &created.store,
                thread_id,
                CatalogPointReadLimit::schema_maximum()
            )
            .expect("catalog read")
            .is_none()
    );
    assert!(matches!(
        created
            .syndic
            .audit_pristine_thread(&created.store, thread_id, &created.execution)
            .expect("Syndic audit"),
        PristineThreadAudit::Missing
    ));
    assert!(matches!(
        created
            .service
            .reconcile_abandonment(seed, CommandCancellation::new()),
        RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::ExactAbandoned { .. }
    ));

    let reused = Fixture::new(40);
    let thread_id = reused.publish_pristine_thread(50, 10);
    let acquisition = reused.acquire(60);
    assert_eq!(acquisition.thread_id(), thread_id);
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Reused
    );
    let abandonment = reused.prepare(acquisition);
    assert!(matches!(
        reused
            .service
            .abandon(abandonment, CommandCancellation::new()),
        RuntimeBackedWindowAbandonmentOutcome::Committed { .. }
    ));
    let row = reused
        .state
        .catalog()
        .row(
            &reused.store,
            thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .expect("catalog read")
        .expect("reused catalog row remains");
    assert_eq!(row.facts().claim(), CatalogClaimSummary::Unclaimed);
    assert!(matches!(
        reused
            .syndic
            .audit_pristine_thread(&reused.store, thread_id, &reused.execution)
            .expect("Syndic audit"),
        PristineThreadAudit::Exact(_)
    ));
}

#[test]
fn cancellation_duplicate_and_unrelated_progress_preserve_move_only_custody() {
    let fixture = Fixture::new(70);
    let sibling = RuntimeBackedWindowAcquisitionService::new(
        &fixture.process,
        Arc::clone(&fixture.store),
        fixture.state.clone(),
        fixture.syndic.clone(),
    );
    let acquisition = fixture.acquire(80);
    let mut abandonment = fixture.prepare(acquisition);
    let seed = abandonment.audit_seed();
    assert!(matches!(
        sibling.reconcile_abandonment(seed.clone(), CommandCancellation::new()),
        RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAbandonmentNotCommitted::DuplicateWindowIdentity,
            ..
        }
    ));
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    let outcome = fixture.service.abandon(abandonment, cancelled);
    let RuntimeBackedWindowAbandonmentOutcome::NotCommitted {
        abandonment: returned,
        evidence: RuntimeBackedWindowAbandonmentNotCommitted::Cancelled,
    } = outcome
    else {
        panic!("cancellation must return custody: {outcome:?}")
    };
    abandonment = returned;
    let _unrelated_window = fixture.acquire(81);
    let unrelated_thread = fixture.publish_pristine_thread(90, 1);
    fixture.mark_catalog_row_stale(unrelated_thread);
    let outcome = fixture
        .service
        .abandon(abandonment, CommandCancellation::new());
    let RuntimeBackedWindowAbandonmentOutcome::NotCommitted { abandonment, .. } = outcome else {
        panic!("stale Syndic validation must reject atomically: {outcome:?}")
    };
    let refreshed = fixture
        .service
        .reconcile_abandonment(abandonment.into_audit_seed(), CommandCancellation::new());
    let RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::ExactAcquired { abandonment } =
        refreshed
    else {
        panic!("unrelated progress must refresh exact custody: {refreshed:?}")
    };
    let retry = fixture
        .service
        .abandon(abandonment, CommandCancellation::new());
    assert!(
        matches!(
            retry,
            RuntimeBackedWindowAbandonmentOutcome::Committed { .. }
        ),
        "retry must commit: {retry:?}"
    );
}

#[test]
fn acknowledgement_loss_reconciles_exact_new_and_reopens_from_the_natural_seed() {
    let fixture = Fixture::new(100);
    let abandonment = fixture.prepare(fixture.acquire(110));
    let seed = abandonment.audit_seed();
    let block = fixture.faults.block_next(FaultPoint::BeforeCommit);
    let service = fixture.service.clone();
    let worker =
        std::thread::spawn(move || service.abandon(abandonment, CommandCancellation::new()));
    assert!(block.wait_until_reached(Duration::from_secs(10)));
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    block.release();
    let outcome = worker.join().expect("abandonment worker");
    let RuntimeBackedWindowAbandonmentOutcome::Indeterminate { reconciliation, .. } = outcome
    else {
        panic!("ack loss must retain reconciliation custody: {outcome:?}")
    };
    assert!(matches!(
        reconciliation.reconcile(&fixture.store),
        RuntimeBackedWindowAbandonmentReconciliationOutcome::ExactAbandoned { .. }
    ));

    let Fixture {
        _directory,
        store,
        state,
        syndic,
        process,
        service,
        faults,
        ..
    } = fixture;
    drop(service);
    drop(state);
    drop(syndic);
    drop(faults);
    let store = match Arc::try_unwrap(store) {
        Ok(store) => store,
        Err(_) => panic!("all old service handles must release the home"),
    };
    store.close().expect("close old home generation");

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        _directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .expect("reopen home");
    let state = BerylState::register(&mut reopened).expect("re-register Beryl state");
    let syndic = SyndicStorage::register(&mut reopened).expect("re-register Syndic");
    let fresh =
        RuntimeBackedWindowAcquisitionService::new(&process, Arc::new(reopened), state, syndic);
    assert!(matches!(
        fresh.reconcile_abandonment(seed, CommandCancellation::new()),
        RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::ExactAbandoned { .. }
    ));
}

#[test]
fn exact_acquired_seed_rearms_only_after_true_same_home_reopen() {
    let fixture = Fixture::new(120);
    let seed = fixture.prepare(fixture.acquire(121)).into_audit_seed();
    let Fixture {
        _directory,
        store,
        state,
        syndic,
        process,
        service,
        faults,
        ..
    } = fixture;
    drop(service);
    drop(state);
    drop(syndic);
    drop(faults);
    let store = match Arc::try_unwrap(store) {
        Ok(store) => store,
        Err(_) => panic!("all old service handles must release the home"),
    };
    store.close().expect("close exact-acquired home");

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        _directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .expect("reopen exact-acquired home");
    let state = BerylState::register(&mut reopened).expect("re-register Beryl state");
    let syndic = SyndicStorage::register(&mut reopened).expect("re-register Syndic");
    let store = Arc::new(reopened);
    let fresh =
        RuntimeBackedWindowAcquisitionService::new(&process, Arc::clone(&store), state, syndic);
    let outcome = fresh.reconcile_abandonment(seed, CommandCancellation::new());
    let RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::ExactAcquired { abandonment } =
        outcome
    else {
        panic!("fresh handles must reconstruct exact acquired custody: {outcome:?}")
    };
    assert!(matches!(
        fresh.abandon(abandonment, CommandCancellation::new()),
        RuntimeBackedWindowAbandonmentOutcome::Committed { .. }
    ));
}

#[test]
fn repeated_created_cycles_release_the_shared_flight_and_leave_no_partial_closure() {
    let fixture = Fixture::new(130);
    for seed in 140..148 {
        let abandonment = fixture.prepare(fixture.acquire(seed));
        let window_id = abandonment.window_id();
        assert!(matches!(
            fixture
                .service
                .abandon(abandonment, CommandCancellation::new()),
            RuntimeBackedWindowAbandonmentOutcome::Committed {
                window_id: committed,
                ..
            } if committed == window_id
        ));
    }
}

#[test]
fn active_job_and_reverse_claim_disagreement_reject_without_cleanup() {
    let active = Fixture::new(160);
    let acquisition = active.acquire(170);
    let window_id = acquisition.window_id();
    let thread_id = acquisition.thread_id();
    let abandonment = active.prepare(acquisition);
    let admission = BranchHandoffJobAdmission::new(
        ResolutionIntentId::from_bytes([171; 16]),
        ResolutionAttemptOrdinal::new(1).expect("attempt"),
        thread_id,
        SyndicThreadId::from_bytes([172; 16]),
        DiscussionContextOwnerId::Draft(SyndicDraftId::from_bytes([173; 16])),
        DiscussionContextDigest::from_bytes([174; 32]),
        SyndicTurnId::from_bytes([175; 16]),
        ResolutionRequestIdentity::new(
            CasThreadId::new("phase238-child").expect("CAS thread"),
            CasTurnId::new("phase238-turn").expect("CAS turn"),
            DynamicToolCallId::new("phase238-tool").expect("tool call"),
        ),
        ParentQueueOrdinal::new(1),
        ResolutionText::new("Keep this exact acquired thread active.").expect("resolution text"),
    );
    execute_contribution(
        &active.store,
        active.state.durable_jobs().admit_branch_handoff(
            active
                .state
                .durable_jobs()
                .revision(&active.store)
                .expect("durable-job revision"),
            AdmitBranchHandoffJob::new(admission),
        ),
    );
    let outcome = active
        .service
        .abandon(abandonment, CommandCancellation::new());
    assert!(matches!(
        outcome,
        RuntimeBackedWindowAbandonmentOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAbandonmentNotCommitted::ActiveDurableJob,
            ..
        }
    ));
    assert!(
        active
            .state
            .session()
            .minimal_bootstrap(&active.store)
            .expect("session")
            .expect("initialized")
            .windows()
            .iter()
            .any(|window| window.window_id() == window_id)
    );

    let reverse = Fixture::new(180);
    let acquisition = reverse.acquire(190);
    let window_id = acquisition.window_id();
    let thread_id = acquisition.thread_id();
    let seed = reverse.prepare(acquisition).into_audit_seed();
    execute_contribution(
        &reverse.store,
        reverse.state.session().delete_thread_claim_copy_for_test(
            reverse
                .state
                .session()
                .revision(&reverse.store)
                .expect("session revision"),
            window_id,
            thread_id,
        ),
    );
    assert!(matches!(
        reverse
            .service
            .reconcile_abandonment(seed, CommandCancellation::new()),
        RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::Collision {
            window_id: collision
        } if collision == window_id
    ));
    assert!(matches!(
        reverse
            .syndic
            .audit_pristine_thread(&reverse.store, thread_id, &reverse.execution)
            .expect("Syndic audit"),
        PristineThreadAudit::Exact(_)
    ));
}

#[test]
fn before_commit_failure_returns_exact_custody_without_cleanup() {
    let fixture = Fixture::new(200);
    let abandonment = fixture.prepare(fixture.acquire(210));
    let window_id = abandonment.window_id();
    fixture.faults.fail_next(FaultPoint::BeforeCommit);
    let outcome = fixture
        .service
        .abandon(abandonment, CommandCancellation::new());
    let RuntimeBackedWindowAbandonmentOutcome::NotCommitted { abandonment, .. } = outcome else {
        panic!("before-commit failure must return custody: {outcome:?}")
    };
    assert_eq!(abandonment.window_id(), window_id);
}

fn initialize_empty_session(store: &HomeStore, state: &BerylState) {
    let session = state.session();
    let initial_window = WindowId::from_bytes([250; 16]);
    execute_contribution(
        store,
        session.initialize_threadless(
            session.revision(store).expect("session revision"),
            InitializeThreadlessWindow::new(initial_window, placement()),
        ),
    );
    let bootstrap = session
        .minimal_bootstrap(store)
        .expect("session read")
        .expect("initialized session");
    let window = &bootstrap.windows()[0];
    execute_contribution(
        store,
        session.remove_window(
            session.revision(store).expect("session revision"),
            RemoveSessionWindow::new(
                bootstrap.header().revision(),
                initial_window,
                window.revision(),
                window.selected_thread(),
            ),
        ),
    );
}

fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(10, 20, 900, 700).expect("window bounds"),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn history_policy() -> DraftEditHistoryPolicyV1 {
    DraftEditHistoryPolicyV1::new(65_536, 1).expect("history policy")
}

fn host_path(value: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, value).expect("host path")
}

fn native_path(mode: RuntimeMode, value: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(mode, PathFlavor::Windows, value).expect("native path")
}

fn execute_contribution(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().expect("home revision"));
    command.add(contribution).expect("add contribution");
    execute_command(store, command);
}

fn execute_command(store: &HomeStore, command: HomeCommand) {
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("setup command must commit cleanly: {outcome:?}"),
    }
}
