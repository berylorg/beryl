#![cfg(feature = "test-faults")]

use beryl_app::{
    LifecycleYieldOutcome,
    cas_projection::*,
    lifecycle_attention::{LifecycleAttentionAdmission, ProcessLifecycleAttentionPool},
};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    HomeServiceReference, HomeStore,
};
use beryl_model::{
    AdmittedHostPath, Availability, ExecutionBinding, InputGateRevision, PathFlavor, RootId,
    RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId, SyndicThreadId, SyndicTurnId,
    WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{
    AvailabilitySnapshot, BerylState, CatalogNormalizedQuery, CreateRuntimeWithHomeRoot,
    InitializeThreadlessWindow, RememberedTarget, ReplaceWindowClaim, RootRegistration,
    RuntimeRegistration, UnixMillis,
};
use std::sync::Arc;
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, InputGateRecord, InputGateState, SyndicPointReadLimit,
    SyndicStorage, SyndicTimestamp,
    test_faults::{FixtureBatch, FixtureRecord},
};

fn bytes(seed: u64) -> [u8; 16] {
    let mut value = [0; 16];
    value[..8].copy_from_slice(&seed.to_be_bytes());
    value
}
fn thread(seed: u64) -> SyndicThreadId {
    SyndicThreadId::from_bytes(bytes(seed))
}
fn turn(seed: u64) -> SyndicTurnId {
    SyndicTurnId::from_bytes(bytes(seed))
}
fn host(value: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, value).unwrap()
}
fn native(value: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, value).unwrap()
}
fn execute(home: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

struct Fixture {
    service: ProjectionConnectionService,
    sessions: ScheduledExecutionSessions,
    state: BerylState,
    home: HomeServiceReference,
    storage: SyndicStorage,
    attention: Arc<ProcessLifecycleAttentionPool>,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new(count: u64) -> Self {
        let directory = tempfile::tempdir().unwrap();
        println!("owned process-work fixture: {}", directory.path().display());
        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let home = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let runtime_id = RuntimeId::from_bytes([1; 16]);
        let root_id = RootId::from_bytes([2; 16]);
        execute(
            &home,
            state.runtime_roots().create_runtime_with_home_root(
                state.runtime_roots().revision(&home).unwrap(),
                CreateRuntimeWithHomeRoot::new(
                    RuntimeRegistration::new(
                        runtime_id,
                        host(r"C:\Codex\codex.exe"),
                        RuntimeMode::host(),
                        native(r"C:\Codex\codex.exe"),
                        UnixMillis::new(1),
                        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2))
                            .unwrap(),
                    )
                    .unwrap(),
                    RootRegistration::new(
                        root_id,
                        native(r"C:\Work\Straße"),
                        host(r"C:\Work\Straße"),
                        UnixMillis::new(1),
                        AvailabilitySnapshot::unknown(),
                    ),
                )
                .unwrap(),
            ),
        );
        for seed in 1..=count {
            execute(
                &home,
                storage.create_thread(
                    storage.revision(&home).unwrap(),
                    CreateThread::ordinary(
                        thread(seed),
                        SyndicDraftId::from_bytes(bytes(seed)),
                        ExecutionBinding::new(runtime_id, root_id, native(r"C:\Work\Straße")),
                        SyndicTimestamp::from_unix_millis(seed),
                        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                    ),
                ),
            );
        }
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        let retained_home = home.service_reference();
        let service = ProjectionConnectionService::new(
            Default::default(),
            home,
            storage.clone(),
            ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
                .unwrap(),
            Box::new(provider),
        )
        .unwrap();
        Self {
            service,
            sessions,
            state,
            home: retained_home,
            storage,
            attention: Arc::new(ProcessLifecycleAttentionPool::new()),
            _directory: directory,
        }
    }

    fn attention(&self, seed: u64) {
        let attempt = self
            .attention
            .track_accepted_yield(
                self.service.home_id(),
                thread(seed),
                turn(seed),
                LifecycleYieldOutcome::PhaseNeedsReview,
            )
            .unwrap();
        assert!(matches!(
            self.attention.report_terminal(&attempt),
            LifecycleAttentionAdmission::Admitted(_)
        ));
    }

    fn pending(&self, seed: u64) {
        self.gate(seed, InputGateState::PendingTurn(turn(seed)));
    }

    fn gate(&self, seed: u64, state: InputGateState) {
        let prior = self
            .storage
            .input_gate(
                &self.home,
                thread(seed),
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap();
        let mut batch = FixtureBatch::new();
        batch
            .put(FixtureRecord::InputGate(
                InputGateRecord::new(
                    thread(seed),
                    InputGateRevision::new(prior.revision().get() + 1).unwrap(),
                    state,
                    0,
                    None,
                    None,
                    0,
                    0,
                    0,
                )
                .unwrap(),
            ))
            .unwrap();
        execute(
            &self.home,
            self.storage
                .fixture_contribution(self.storage.revision(&self.home).unwrap(), batch),
        );
    }

    fn claim(&self, seed: u64) -> WindowId {
        let window = WindowId::from_bytes([5; 16]);
        let session = self.state.session();
        execute(
            &self.home,
            session.initialize_threadless(
                session.revision(&self.home).unwrap(),
                InitializeThreadlessWindow::new(
                    window,
                    WindowPlacement::new(
                        WindowBounds::new(0, 0, 900, 700).unwrap(),
                        WindowDisplayState::Normal,
                        None,
                        None,
                    ),
                ),
            ),
        );
        let prior = session.minimal_bootstrap(&self.home).unwrap().unwrap();
        execute(
            &self.home,
            session.replace_claim(
                session.revision(&self.home).unwrap(),
                ReplaceWindowClaim::new(
                    prior.header().revision(),
                    window,
                    prior.windows()[0].revision(),
                    None,
                    RememberedTarget::new(
                        RuntimeId::from_bytes([1; 16]),
                        RootId::from_bytes([2; 16]),
                    ),
                    thread(seed),
                ),
            ),
        );
        window
    }
}

#[test]
fn source_query_has_exact_metadata_claim_search_counts_and_no_effects() {
    let fixture = Fixture::new(4);
    fixture.pending(1);
    fixture.attention(1);
    fixture.attention(2);
    let window = fixture.claim(1);
    let reader = fixture
        .service
        .process_work_reader(&fixture.sessions, &fixture.attention);
    let revision = reader.query_revision(&fixture.state).unwrap();
    let query = CatalogNormalizedQuery::new("STRASSE").unwrap();
    let cancellation = ProjectionCancellationToken::new();
    let before = fixture.home.home_revision().unwrap();
    let page = reader
        .query_page(
            &fixture.state,
            &revision,
            &query,
            0,
            ProcessWorkPageLimits::new(256, 65_536).unwrap(),
            &cancellation,
        )
        .unwrap();
    assert_eq!(
        (
            page.total_threads(),
            page.matched_threads(),
            page.attention_threads()
        ),
        (2, 2, 2)
    );
    assert_eq!(page.query(), &query);
    assert_eq!(
        page.records()
            .iter()
            .map(|row| row.thread_id)
            .collect::<Vec<_>>(),
        [thread(2), thread(1)]
    );
    assert_eq!(page.records()[1].claim.unwrap().window_id(), window);
    assert!(page.records()[1].facts.pending);
    assert_eq!(
        page.records()[1]
            .catalog
            .execution()
            .full_root_path()
            .as_str(),
        r"C:\Work\Straße"
    );
    assert_eq!(
        reader
            .query_position(&fixture.state, &revision, &query, thread(1), &cancellation)
            .unwrap(),
        Some(1)
    );
    let missing = reader
        .query_page(
            &fixture.state,
            &revision,
            &CatalogNormalizedQuery::new("absent").unwrap(),
            0,
            ProcessWorkPageLimits::new(256, 65_536).unwrap(),
            &cancellation,
        )
        .unwrap();
    assert_eq!(
        (
            missing.total_threads(),
            missing.matched_threads(),
            missing.attention_threads()
        ),
        (2, 0, 2)
    );
    assert!(missing.records().is_empty());
    assert_eq!(fixture.home.home_revision().unwrap(), before);
    assert_eq!(fixture.attention.snapshot().len(), 2);
    assert_eq!(reader.query_revision(&fixture.state).unwrap(), revision);
    let row_bytes = page.records()[0].bytes();
    let bounded = reader
        .query_page(
            &fixture.state,
            &revision,
            &query,
            0,
            ProcessWorkPageLimits::new(256, row_bytes).unwrap(),
            &cancellation,
        )
        .unwrap();
    assert_eq!(bounded.records().len(), 1);
    assert_eq!(bounded.bytes(), row_bytes);
    assert!(matches!(
        reader.query_page(
            &fixture.state,
            &revision,
            &query,
            0,
            ProcessWorkPageLimits::new(256, row_bytes - 1).unwrap(),
            &cancellation
        ),
        Err(ProcessWorkError::ByteLimit)
    ));
}

#[test]
fn source_queries_reject_cancellation_foreign_state_stale_membership_and_home_drift() {
    let fixture = Fixture::new(3);
    let other = Fixture::new(1);
    fixture.attention(1);
    let reader = fixture
        .service
        .process_work_reader(&fixture.sessions, &fixture.attention);
    let revision = reader.query_revision(&fixture.state).unwrap();
    let query = CatalogNormalizedQuery::new("").unwrap();
    let cancellation = ProjectionCancellationToken::new();
    let limits = ProcessWorkPageLimits::new(16, 65_536).unwrap();
    assert!(reader.query_revision(&other.state).is_err());
    assert!(
        reader
            .query_page(&other.state, &revision, &query, 0, limits, &cancellation)
            .is_err()
    );
    cancellation.cancel();
    assert!(matches!(
        reader.query_page(&fixture.state, &revision, &query, 0, limits, &cancellation),
        Err(ProcessWorkError::Cancelled)
    ));
    let cancellation = ProjectionCancellationToken::new();
    fixture.attention(2);
    assert!(
        reader
            .query_page(&fixture.state, &revision, &query, 0, limits, &cancellation)
            .is_err()
    );
    let revision = reader.query_revision(&fixture.state).unwrap();
    fixture.claim(1);
    assert!(matches!(
        reader.query_page(&fixture.state, &revision, &query, 0, limits, &cancellation),
        Err(ProcessWorkError::StaleRevision)
    ));
    let revision = reader.query_revision(&fixture.state).unwrap();
    assert_eq!(
        reader
            .query_position(&fixture.state, &revision, &query, thread(3), &cancellation)
            .unwrap(),
        None
    );
    drop(fixture);
    assert!(matches!(
        reader.query_revision(&other.state),
        Err(ProcessWorkError::Closed)
    ));
}

#[test]
fn large_source_pages_and_deep_logical_positions_keep_fixed_result_bounds() {
    let fixture = Fixture::new(520);
    for seed in 1..=520 {
        fixture.gate(seed, InputGateState::FinalizingHistory(turn(seed)));
    }
    fixture.attention(17);
    let reader = fixture
        .service
        .process_work_reader(&fixture.sessions, &fixture.attention);
    let revision = reader.query_revision(&fixture.state).unwrap();
    let query = CatalogNormalizedQuery::new("HOST").unwrap();
    let cancellation = ProjectionCancellationToken::new();
    let limits = ProcessWorkPageLimits::new(16, 65_536).unwrap();
    let page = reader
        .query_page(
            &fixture.state,
            &revision,
            &query,
            507,
            limits,
            &cancellation,
        )
        .unwrap();
    assert_eq!(
        (
            page.total_threads(),
            page.matched_threads(),
            page.attention_threads()
        ),
        (520, 520, 1)
    );
    assert_eq!(page.logical_start(), 507);
    assert_eq!(
        page.records()
            .iter()
            .map(|row| row.thread_id)
            .collect::<Vec<_>>(),
        (1..=13).rev().map(thread).collect::<Vec<_>>()
    );
    assert!(page.records().len() <= 16 && page.bytes() <= 65_536);
    assert_eq!(
        reader
            .query_position(&fixture.state, &revision, &query, thread(17), &cancellation)
            .unwrap(),
        Some(503)
    );
    let beyond = reader
        .query_page(
            &fixture.state,
            &revision,
            &query,
            600,
            limits,
            &cancellation,
        )
        .unwrap();
    assert!(beyond.records().is_empty());
    let inventory = fixture
        .service
        .process_work_inventory(&fixture.sessions, &fixture.attention);
    let revision = inventory.revision().unwrap();
    let mut cursor = None;
    let mut count = 0;
    loop {
        let page = inventory
            .page(
                &revision,
                cursor.as_ref(),
                ProcessWorkPageLimits::new(256, 65_536).unwrap(),
                &cancellation,
            )
            .unwrap();
        assert_eq!(page.total_threads(), 520);
        assert!(page.records().len() <= 256 && page.bytes() <= 65_536);
        count += page.records().len();
        cursor = page.next_cursor().cloned();
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(count, 520);
}
