use std::{
    io::Write,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use beryl_app::{
    catalog_projection::{ThreadCatalogProjectionPreparation, prepare_thread_catalog_projection},
    window_acquisition::{
        RuntimeBackedWindowAcquisitionDisposition,
        RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome,
        RuntimeBackedWindowAcquisitionNotCommitted, RuntimeBackedWindowAcquisitionOutcome,
        RuntimeBackedWindowAcquisitionReconciliationOutcome,
        RuntimeBackedWindowAcquisitionRepairReconciliationOutcome,
        RuntimeBackedWindowAcquisitionRequest, RuntimeBackedWindowAcquisitionService,
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
    CatalogPointReadLimit, CreateRuntimeWithHomeRoot, DiscussionContextDigest,
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
    service: RuntimeBackedWindowAcquisitionService,
    faults: FaultController,
    runtime_id: RuntimeId,
    root_id: RootId,
    execution: ExecutionBinding,
}

impl Fixture {
    fn new(seed: u8) -> Self {
        let directory = tempfile::tempdir().expect("temp home");
        Self::from_directory(directory, seed)
    }

    fn from_directory(directory: tempfile::TempDir, seed: u8) -> Self {
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
        let service = RuntimeBackedWindowAcquisitionService::new(
            Arc::clone(&store),
            state.clone(),
            syndic.clone(),
        );
        Self {
            _directory: directory,
            store,
            state,
            syndic,
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

    fn publish_pristine_thread(&self, thread_seed: u8, created_at: u64) -> SyndicThreadId {
        let thread_id = SyndicThreadId::from_bytes([thread_seed; 16]);
        let creation = CreateThread::ordinary(
            thread_id,
            SyndicDraftId::from_bytes([thread_seed.wrapping_add(1); 16]),
            self.execution.clone(),
            SyndicTimestamp::from_unix_millis(created_at),
            history_policy(),
        );
        execute_contribution(
            &self.store,
            self.syndic.create_thread(
                self.syndic.revision(&self.store).expect("Syndic revision"),
                creation,
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
fn acquisition_deterministically_creates_or_reuses_the_oldest_pristine_thread() {
    let created = Fixture::new(10);
    let outcome = created
        .service
        .acquire(created.request(20), CommandCancellation::new());
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = outcome else {
        panic!("fallback creation must commit: {outcome:?}")
    };
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Created
    );
    assert_eq!(
        acquisition.thread_id(),
        SyndicThreadId::from_bytes([21; 16])
    );

    let reused = Fixture::new(40);
    let newer = reused.publish_pristine_thread(51, 20);
    let oldest = reused.publish_pristine_thread(50, 10);
    let outcome = reused
        .service
        .acquire(reused.request(60), CommandCancellation::new());
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = outcome else {
        panic!("pristine reuse must commit: {outcome:?}")
    };
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Reused
    );
    assert_eq!(acquisition.thread_id(), oldest);
    assert_ne!(acquisition.thread_id(), newer);
}

#[test]
fn cancellation_is_definitive_and_releases_the_window_identity() {
    let fixture = Fixture::new(70);
    let request = fixture.request(80);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        fixture.service.acquire(request.clone(), cancellation),
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::Cancelled,
            ..
        }
    ));
    assert!(matches!(
        fixture.service.acquire(request, CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::Committed { .. }
    ));
}

#[test]
fn concurrent_fixed_reuse_intents_never_create_a_substitute() {
    let fixture = Fixture::new(110);
    let candidate = fixture.publish_pristine_thread(120, 10);
    let (reached_sender, reached_receiver) = mpsc::channel();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    fixture.service.test_set_before_execute({
        let reached_sender = reached_sender.clone();
        let gate = Arc::clone(&gate);
        move || {
            reached_sender.send(()).expect("report prepared command");
            let (lock, changed) = &*gate;
            let mut released = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            while !*released {
                released = changed
                    .wait(released)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        }
    });
    let first_service = fixture.service.clone();
    let first_request = fixture.request(130);
    let first = std::thread::spawn(move || {
        first_service.acquire(first_request, CommandCancellation::new())
    });
    let second_service = fixture.service.clone();
    let second_request = fixture.request(140);
    let second = std::thread::spawn(move || {
        second_service.acquire(second_request, CommandCancellation::new())
    });
    reached_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("first fixed intent prepared");
    let second_reached = reached_receiver
        .recv_timeout(Duration::from_secs(10))
        .is_ok();
    let (lock, changed) = &*gate;
    *lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
    changed.notify_all();
    let outcomes = [
        first.join().expect("first acquisition"),
        second.join().expect("second acquisition"),
    ];
    assert!(
        second_reached,
        "second fixed intent did not prepare: {outcomes:?}"
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(
                |outcome| matches!(outcome, RuntimeBackedWindowAcquisitionOutcome::Committed {
                acquisition,
                ..
            } if acquisition.thread_id() == candidate && acquisition.disposition()
                == RuntimeBackedWindowAcquisitionDisposition::Reused)
            )
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
                    evidence: RuntimeBackedWindowAcquisitionNotCommitted::Command(_),
                    ..
                }
            ))
            .count(),
        1
    );
    for fallback in [
        SyndicThreadId::from_bytes([131; 16]),
        SyndicThreadId::from_bytes([141; 16]),
    ] {
        assert!(
            fixture
                .state
                .catalog()
                .row(
                    &fixture.store,
                    fallback,
                    CatalogPointReadLimit::schema_maximum(),
                )
                .expect("catalog row read")
                .is_none()
        );
    }
}

#[test]
fn ack_loss_retains_sole_identity_custody_until_exact_new_reconciliation() {
    let fixture = Fixture::new(90);
    let request = fixture.request(100);
    let block = fixture.faults.block_next(FaultPoint::BeforeCommit);
    let service = fixture.service.clone();
    let worker_request = request.clone();
    let worker =
        std::thread::spawn(move || service.acquire(worker_request, CommandCancellation::new()));
    assert!(block.wait_until_reached(Duration::from_secs(10)));
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    block.release();
    let outcome = worker.join().expect("acquisition worker");
    let RuntimeBackedWindowAcquisitionOutcome::Indeterminate { reconciliation, .. } = outcome
    else {
        panic!("ack loss must retain indeterminate custody: {outcome:?}")
    };
    let fresh = RuntimeBackedWindowAcquisitionService::new(
        Arc::clone(&fixture.store),
        fixture.state.clone(),
        fixture.syndic.clone(),
    );
    assert!(matches!(
        fresh.acquire(request.clone(), CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::ExactCommitted { acquisition }
            if acquisition.window_id() == request.window_id()
                && acquisition.disposition()
                    == RuntimeBackedWindowAcquisitionDisposition::Created
    ));
    assert!(matches!(
        fixture
            .service
            .acquire(fixture.request(100), CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::DuplicateWindowIdentity,
            ..
        }
    ));
    assert!(matches!(
        reconciliation.reconcile(&fixture.store),
        RuntimeBackedWindowAcquisitionReconciliationOutcome::ExactNew { .. }
    ));
}

#[test]
fn ack_loss_is_exactly_reconstructed_after_process_exit_and_reopen() {
    let parent = tempfile::tempdir().expect("parent temp home");
    let output = std::process::Command::new(std::env::current_exe().expect("current test binary"))
        .arg("--exact")
        .arg("phase236_ack_loss_subprocess_child")
        .arg("--nocapture")
        .env("BERYL_PHASE236_CRASH_PARENT", parent.path())
        .output()
        .expect("run crash child");
    assert!(
        output.status.success(),
        "crash child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("child stdout");
    let home = stdout
        .lines()
        .find_map(|line| line.strip_prefix("PHASE236_HOME="))
        .expect("child home path");
    let mut reopened = HomeStore::open(HomeOpenOptions::new(home, HomeSchemaVersion::CURRENT))
        .expect("recover and reopen crashed home");
    let state = BerylState::register(&mut reopened).expect("re-register Beryl state");
    let syndic = SyndicStorage::register(&mut reopened).expect("re-register Syndic");
    let request = RuntimeBackedWindowAcquisitionRequest::new(
        WindowId::from_bytes([130; 16]),
        RememberedTarget::new(
            RuntimeId::from_bytes([120; 16]),
            RootId::from_bytes([121; 16]),
        ),
        placement(),
        SyndicThreadId::from_bytes([131; 16]),
        SyndicDraftId::from_bytes([132; 16]),
        ExecutionBinding::new(
            RuntimeId::from_bytes([120; 16]),
            RootId::from_bytes([121; 16]),
            native_path(RuntimeMode::host(), r"C:\Work\Beryl"),
        ),
        SyndicTimestamp::from_unix_millis(140),
        history_policy(),
    )
    .expect("reopened request");
    let fresh = RuntimeBackedWindowAcquisitionService::new(Arc::new(reopened), state, syndic);
    let RuntimeBackedWindowAcquisitionOutcome::ExactCommitted { acquisition } =
        fresh.acquire(request.clone(), CommandCancellation::new())
    else {
        panic!("reopened service must reconstruct the durable acquisition")
    };
    assert_eq!(acquisition.window_id(), request.window_id());
    assert_eq!(acquisition.thread_id(), request.fallback_thread_id());
    assert_eq!(acquisition.draft_id(), request.fallback_draft_id());
    assert_eq!(acquisition.target(), request.target());
    assert_eq!(acquisition.placement(), request.placement());
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Created
    );
}

#[test]
fn phase236_ack_loss_subprocess_child() {
    let Some(parent) = std::env::var_os("BERYL_PHASE236_CRASH_PARENT") else {
        return;
    };
    let directory = tempfile::tempdir_in(parent).expect("child temp home");
    let fixture = Fixture::from_directory(directory, 120);
    println!("PHASE236_HOME={}", fixture._directory.path().display());
    std::io::stdout().flush().expect("flush child home path");
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = fixture
        .service
        .acquire(fixture.request(130), CommandCancellation::new());
    assert!(matches!(
        outcome,
        RuntimeBackedWindowAcquisitionOutcome::Indeterminate { .. }
    ));
    std::process::exit(0);
}

#[test]
fn stale_catalog_is_repaired_before_the_fixed_reuse_intent() {
    let fixture = Fixture::new(150);
    let candidate = fixture.publish_pristine_thread(160, 10);
    fixture.mark_catalog_row_stale(candidate);

    let outcome = fixture
        .service
        .acquire(fixture.request(170), CommandCancellation::new());
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = outcome else {
        panic!("stale catalog repair must precede acquisition: {outcome:?}")
    };
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Reused
    );
    assert_eq!(acquisition.thread_id(), candidate);
}

#[test]
fn repair_budget_exhaustion_is_definitive_atomic_and_releases_identity() {
    let fixture = Fixture::new(135);
    let first = fixture.publish_pristine_thread(145, 10);
    let second = fixture.publish_pristine_thread(146, 20);
    fixture.mark_catalog_row_stale(first);
    fixture.mark_catalog_row_stale(second);
    let request = fixture.request(155);
    let bounded = fixture.service.clone().test_with_catalog_repair_budget(1);
    assert!(matches!(
        bounded.acquire(request.clone(), CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::CatalogRepairBudgetExhausted,
            ..
        }
    ));
    let bootstrap = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .expect("session read")
        .expect("initialized session");
    assert!(
        bootstrap
            .windows()
            .iter()
            .all(|window| window.window_id() != request.window_id())
    );
    assert!(
        fixture
            .state
            .catalog()
            .row(
                &fixture.store,
                request.fallback_thread_id(),
                CatalogPointReadLimit::schema_maximum(),
            )
            .expect("fallback catalog read")
            .is_none()
    );
    assert!(matches!(
        fixture
            .syndic
            .audit_pristine_thread(
                &fixture.store,
                request.fallback_thread_id(),
                request.fallback_execution(),
            )
            .expect("fallback Syndic audit"),
        PristineThreadAudit::Missing
    ));
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        bounded.acquire(request, cancelled),
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::Cancelled,
            ..
        }
    ));
}

#[test]
fn repair_ack_loss_retains_identity_custody_until_exact_classification() {
    let fixture = Fixture::new(180);
    let candidate = fixture.publish_pristine_thread(190, 10);
    fixture.mark_catalog_row_stale(candidate);
    let request = fixture.request(200);
    let block = fixture.faults.block_next(FaultPoint::BeforeCommit);
    let service = fixture.service.clone();
    let worker = std::thread::spawn(move || service.acquire(request, CommandCancellation::new()));
    assert!(block.wait_until_reached(Duration::from_secs(10)));
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    block.release();

    let outcome = worker.join().expect("repair worker");
    let RuntimeBackedWindowAcquisitionOutcome::RepairIndeterminate { reconciliation, .. } = outcome
    else {
        panic!("repair ack loss must retain indeterminate custody: {outcome:?}")
    };
    assert!(matches!(
        fixture
            .service
            .acquire(fixture.request(200), CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::DuplicateWindowIdentity,
            ..
        }
    ));
    assert!(matches!(
        reconciliation.reconcile(&fixture.store),
        RuntimeBackedWindowAcquisitionRepairReconciliationOutcome::ExactNew { .. }
    ));

    let transient = fixture
        .service
        .acquire(fixture.request(200), CommandCancellation::new());
    assert!(matches!(
        transient,
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::Preparation(_),
            ..
        }
    ));
}

#[test]
fn fresh_service_classifies_exact_old_created_and_request_collision() {
    let fixture = Fixture::new(210);
    let request = fixture.request(220);
    assert!(matches!(
        fixture.service.reconcile_natural_state(&request),
        RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::ExactOld { .. }
    ));
    assert!(matches!(
        fixture
            .service
            .acquire(request.clone(), CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. }
            if acquisition.disposition() == RuntimeBackedWindowAcquisitionDisposition::Created
    ));

    let fresh = RuntimeBackedWindowAcquisitionService::new(
        Arc::clone(&fixture.store),
        fixture.state.clone(),
        fixture.syndic.clone(),
    );
    assert!(matches!(
        fresh.reconcile_natural_state(&request),
        RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::ExactCommitted {
            acquisition,
        } if acquisition.window_id() == request.window_id()
            && acquisition.thread_id() == request.fallback_thread_id()
            && acquisition.draft_id() == request.fallback_draft_id()
            && acquisition.disposition() == RuntimeBackedWindowAcquisitionDisposition::Created
    ));
    assert!(matches!(
        fresh.acquire(request.clone(), CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::ExactCommitted { acquisition }
            if acquisition.window_id() == request.window_id()
                && acquisition.disposition()
                    == RuntimeBackedWindowAcquisitionDisposition::Created
    ));

    let collision = RuntimeBackedWindowAcquisitionRequest::new(
        request.window_id(),
        request.target(),
        request.placement().clone(),
        request.fallback_thread_id(),
        SyndicDraftId::from_bytes([223; 16]),
        request.fallback_execution().clone(),
        request.fallback_created_at(),
        request.fallback_history_policy(),
    )
    .expect("collision request");
    assert!(matches!(
        fresh.reconcile_natural_state(&collision),
        RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::Collision { .. }
    ));
    assert!(matches!(
        fresh.acquire(collision, CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::WindowIdentityCollision,
            ..
        }
    ));
    for ordinal in 0_u16..256 {
        let mut draft = [0_u8; 16];
        draft[0] = ordinal as u8;
        draft[15] = 1;
        let collision = RuntimeBackedWindowAcquisitionRequest::new(
            request.window_id(),
            request.target(),
            request.placement().clone(),
            request.fallback_thread_id(),
            SyndicDraftId::from_bytes(draft),
            request.fallback_execution().clone(),
            request.fallback_created_at(),
            request.fallback_history_policy(),
        )
        .expect("collision request");
        assert!(matches!(
            fresh.reconcile_natural_state(&collision),
            RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::Collision { .. }
        ));
    }
    assert!(matches!(
        fresh.acquire(fixture.request(230), CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::Committed { .. }
    ));
}

#[test]
fn absent_window_with_exact_fallback_is_collision_before_and_after_reopen() {
    let fixture = Fixture::new(40);
    let request = fixture.request(50);
    let creation = CreateThread::ordinary(
        request.fallback_thread_id(),
        request.fallback_draft_id(),
        request.fallback_execution().clone(),
        request.fallback_created_at(),
        request.fallback_history_policy(),
    );
    execute_contribution(
        &fixture.store,
        fixture.syndic.create_thread(
            fixture
                .syndic
                .revision(&fixture.store)
                .expect("Syndic revision"),
            creation,
        ),
    );
    assert!(matches!(
        fixture.service.reconcile_natural_state(&request),
        RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::Collision { .. }
    ));

    let Fixture {
        _directory,
        store,
        service,
        ..
    } = fixture;
    drop(service);
    let store = match Arc::try_unwrap(store) {
        Ok(store) => store,
        Err(_) => panic!("all service generations must release HomeStore ownership"),
    };
    if let Err(error) = store.close() {
        panic!("close exact-fallback home: {error}")
    }
    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        _directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .expect("reopen exact-fallback home");
    let state = BerylState::register(&mut reopened).expect("re-register Beryl state");
    let syndic = SyndicStorage::register(&mut reopened).expect("re-register Syndic");
    let fresh = RuntimeBackedWindowAcquisitionService::new(Arc::new(reopened), state, syndic);
    assert!(matches!(
        fresh.reconcile_natural_state(&request),
        RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::Collision { .. }
    ));
}

#[test]
fn absent_window_with_conflicting_fallback_is_collision() {
    let fixture = Fixture::new(60);
    let request = fixture.request(70);
    let conflicting = CreateThread::ordinary(
        request.fallback_thread_id(),
        request.fallback_draft_id(),
        ExecutionBinding::new(
            RuntimeId::from_bytes([99; 16]),
            RootId::from_bytes([98; 16]),
            native_path(RuntimeMode::host(), r"C:\Other"),
        ),
        request.fallback_created_at(),
        request.fallback_history_policy(),
    );
    execute_contribution(
        &fixture.store,
        fixture.syndic.create_thread(
            fixture
                .syndic
                .revision(&fixture.store)
                .expect("Syndic revision"),
            conflicting,
        ),
    );
    assert!(matches!(
        fixture.service.reconcile_natural_state(&request),
        RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::Collision { .. }
    ));
    assert!(matches!(
        fixture.service.acquire(request, CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::WindowIdentityCollision,
            ..
        }
    ));
}

#[test]
fn fresh_service_reconstructs_the_exact_reused_result() {
    let fixture = Fixture::new(15);
    let candidate = fixture.publish_pristine_thread(25, 10);
    let request = fixture.request(35);
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = fixture
        .service
        .acquire(request.clone(), CommandCancellation::new())
    else {
        panic!("reuse must commit")
    };
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Reused
    );
    assert_eq!(acquisition.thread_id(), candidate);

    let fresh = RuntimeBackedWindowAcquisitionService::new(
        Arc::clone(&fixture.store),
        fixture.state.clone(),
        fixture.syndic.clone(),
    );
    assert!(matches!(
        fresh.reconcile_natural_state(&request),
        RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::ExactCommitted {
            acquisition,
        } if acquisition.thread_id() == candidate
            && acquisition.disposition() == RuntimeBackedWindowAcquisitionDisposition::Reused
    ));
}

#[test]
fn reverse_claim_disagreement_is_collision_and_never_creates_a_substitute() {
    let fixture = Fixture::new(25);
    let candidate = fixture.publish_pristine_thread(35, 10);
    let request = fixture.request(45);
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = fixture
        .service
        .acquire(request.clone(), CommandCancellation::new())
    else {
        panic!("reuse must commit before claim corruption")
    };
    assert_eq!(acquisition.thread_id(), candidate);
    execute_contribution(
        &fixture.store,
        fixture.state.session().delete_thread_claim_copy_for_test(
            fixture
                .state
                .session()
                .revision(&fixture.store)
                .expect("session revision"),
            request.window_id(),
            candidate,
        ),
    );

    let fresh = RuntimeBackedWindowAcquisitionService::new(
        Arc::clone(&fixture.store),
        fixture.state.clone(),
        fixture.syndic.clone(),
    );
    assert!(matches!(
        fresh.acquire(request.clone(), CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::WindowIdentityCollision,
            ..
        }
    ));
    assert!(
        fixture
            .state
            .catalog()
            .row(
                &fixture.store,
                request.fallback_thread_id(),
                CatalogPointReadLimit::schema_maximum(),
            )
            .expect("fallback catalog read")
            .is_none()
    );
    assert!(matches!(
        fixture
            .syndic
            .audit_pristine_thread(
                &fixture.store,
                request.fallback_thread_id(),
                request.fallback_execution(),
            )
            .expect("fallback Syndic audit"),
        PristineThreadAudit::Missing
    ));
}

#[test]
fn request_rejects_a_fallback_execution_for_another_target() {
    let fixture = Fixture::new(45);
    let other = ExecutionBinding::new(
        RuntimeId::from_bytes([46; 16]),
        fixture.root_id,
        native_path(RuntimeMode::host(), r"C:\Work\Beryl"),
    );
    assert!(
        RuntimeBackedWindowAcquisitionRequest::new(
            WindowId::from_bytes([47; 16]),
            RememberedTarget::new(fixture.runtime_id, fixture.root_id),
            placement(),
            SyndicThreadId::from_bytes([48; 16]),
            SyndicDraftId::from_bytes([49; 16]),
            other,
            SyndicTimestamp::from_unix_millis(10),
            history_policy(),
        )
        .is_err()
    );
}

#[test]
fn a_live_durable_job_makes_the_pristine_thread_ineligible_without_substitution() {
    let fixture = Fixture::new(55);
    let candidate = fixture.publish_pristine_thread(65, 10);
    let admission = BranchHandoffJobAdmission::new(
        ResolutionIntentId::from_bytes([66; 16]),
        ResolutionAttemptOrdinal::new(1).expect("attempt"),
        candidate,
        SyndicThreadId::from_bytes([67; 16]),
        DiscussionContextOwnerId::Draft(SyndicDraftId::from_bytes([68; 16])),
        DiscussionContextDigest::from_bytes([69; 32]),
        SyndicTurnId::from_bytes([70; 16]),
        ResolutionRequestIdentity::new(
            CasThreadId::new("phase236-child").expect("CAS thread"),
            CasTurnId::new("phase236-turn").expect("CAS turn"),
            DynamicToolCallId::new("phase236-tool").expect("tool call"),
        ),
        ParentQueueOrdinal::new(1),
        ResolutionText::new("Keep the exact durable job live.").expect("resolution text"),
    );
    execute_contribution(
        &fixture.store,
        fixture.state.durable_jobs().admit_branch_handoff(
            fixture
                .state
                .durable_jobs()
                .revision(&fixture.store)
                .expect("durable-job revision"),
            AdmitBranchHandoffJob::new(admission),
        ),
    );

    let request = fixture.request(75);
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = fixture
        .service
        .acquire(request.clone(), CommandCancellation::new())
    else {
        panic!("fallback must commit when the only candidate has a live job")
    };
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Created
    );
    assert_eq!(acquisition.thread_id(), request.fallback_thread_id());
    assert_ne!(acquisition.thread_id(), candidate);
}

#[test]
fn exhaustive_multi_page_scan_selects_the_global_oldest_candidate() {
    let fixture = Fixture::new(5);
    let mut oldest = None;
    for index in 0_u8..17 {
        let thread_id = fixture.publish_pristine_thread(80 + index, u64::from(17 - index));
        if index == 16 {
            oldest = Some(thread_id);
        }
    }
    let oldest = oldest.expect("oldest candidate");
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = fixture
        .service
        .acquire(fixture.request(110), CommandCancellation::new())
    else {
        panic!("multi-page reuse must commit")
    };
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Reused
    );
    assert_eq!(acquisition.thread_id(), oldest);
}

#[test]
fn cancellation_after_the_first_natural_audit_page_releases_the_flight() {
    let fixture = Fixture::new(6);
    for index in 0_u8..17 {
        fixture.publish_pristine_thread(100 + index, u64::from(index) + 1);
    }
    let request = fixture.request(125);
    let cancellation = CommandCancellation::new();
    let hook_cancellation = cancellation.clone();
    let observed = Arc::new(AtomicBool::new(false));
    let hook_observed = Arc::clone(&observed);
    let window_id = request.window_id();
    fixture
        .state
        .set_window_acquisition_audit_page_hook_for_test(Some(Arc::new(
            move |observed_window, page_ordinal| {
                if observed_window == window_id && page_ordinal == 1 {
                    hook_observed.store(true, Ordering::Release);
                    hook_cancellation.cancel();
                }
            },
        )));
    let outcome = fixture.service.acquire(request.clone(), cancellation);
    fixture
        .state
        .set_window_acquisition_audit_page_hook_for_test(None);
    assert!(observed.load(Ordering::Acquire));
    assert!(matches!(
        outcome,
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::Cancelled,
            ..
        }
    ));
    assert!(matches!(
        fixture.service.acquire(request, CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::Committed { .. }
    ));
}

#[test]
fn the_257th_live_identity_is_rejected_before_any_syndic_side_effect() {
    let fixture = Fixture::new(115);
    let (reached_sender, reached_receiver) = mpsc::channel();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    fixture.service.test_set_before_execute({
        let gate = Arc::clone(&gate);
        move || {
            reached_sender.send(()).expect("report prepared command");
            let (lock, changed) = &*gate;
            let mut released = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            while !*released {
                released = changed
                    .wait(released)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        }
    });
    let mut workers = Vec::with_capacity(256);
    for seed in 0_u8..=u8::MAX {
        let service = fixture.service.clone();
        let request = fixture.request(seed);
        workers.push(std::thread::spawn(move || {
            service.acquire(request, CommandCancellation::new())
        }));
        reached_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("live flight reached writer boundary");
    }

    let mut window_bytes = [1_u8; 16];
    window_bytes[15] = 2;
    let mut thread_bytes = [3_u8; 16];
    thread_bytes[15] = 4;
    let extra_thread = SyndicThreadId::from_bytes(thread_bytes);
    let extra = RuntimeBackedWindowAcquisitionRequest::new(
        WindowId::from_bytes(window_bytes),
        RememberedTarget::new(fixture.runtime_id, fixture.root_id),
        placement(),
        extra_thread,
        SyndicDraftId::from_bytes([5; 16]),
        fixture.execution.clone(),
        SyndicTimestamp::from_unix_millis(500),
        history_policy(),
    )
    .expect("extra request");
    assert!(matches!(
        fixture.service.acquire(extra, CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::FlightCapacity,
            ..
        }
    ));
    assert!(matches!(
        fixture
            .syndic
            .audit_pristine_thread(&fixture.store, extra_thread, &fixture.execution)
            .expect("Syndic audit"),
        PristineThreadAudit::Missing
    ));

    let (lock, changed) = &*gate;
    *lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
    changed.notify_all();
    for worker in workers {
        let _ = worker.join().expect("acquisition worker");
    }
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
