#![cfg(feature = "test-faults")]

use beryl_app::{
    RunningThreadActivation, RunningThreadActivationError, RunningThreadActivationOutcome,
    RunningThreadActivationPreparation,
};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, MutationContribution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    AdmittedHostPath, Availability, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicThreadId, WindowBounds, WindowDisplayState, WindowId,
    WindowPlacement,
};
use beryl_state::{
    AvailabilitySnapshot, BerylState, CatalogClaimKind, CatalogClaimSummary, CatalogPointReadLimit,
    CreateRuntimeWithHomeRoot, InitializeThreadlessWindow, MarkCatalogRowStale, RememberedTarget,
    RootRegistration, RuntimeRegistration, UnixMillis, UpdateWindowPlacement, WindowClaimSelection,
};
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1, SyndicStorage, SyndicTimestamp};

fn thread(seed: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([seed; 16])
}
fn native(path: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path).unwrap()
}
fn host(path: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap()
}
fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(0, 0, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
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
    state: BerylState,
    syndic: SyndicStorage,
    faults: FaultController,
    window: WindowId,
    target: RememberedTarget,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        println!(
            "owned running-thread activation fixture: {}",
            directory.path().display()
        );
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let syndic = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let target =
            RememberedTarget::new(RuntimeId::from_bytes([1; 16]), RootId::from_bytes([2; 16]));
        execute(
            &store,
            state.runtime_roots().create_runtime_with_home_root(
                state.runtime_roots().revision(&store).unwrap(),
                CreateRuntimeWithHomeRoot::new(
                    RuntimeRegistration::new(
                        target.runtime_id(),
                        host(r"C:\Codex\codex.exe"),
                        RuntimeMode::host(),
                        beryl_model::RuntimeLaunchForm::CodexCli,
                        native(r"C:\Codex\codex.exe"),
                        UnixMillis::new(1),
                        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2))
                            .unwrap(),
                    )
                    .unwrap(),
                    RootRegistration::new(
                        target.root_id(),
                        native(r"C:\Work\Beryl"),
                        host(r"C:\Work\Beryl"),
                        UnixMillis::new(1),
                        AvailabilitySnapshot::unknown(),
                    ),
                )
                .unwrap(),
            ),
        );
        for seed in [3, 4] {
            execute(
                &store,
                syndic.create_thread(
                    syndic.revision(&store).unwrap(),
                    CreateThread::ordinary(
                        thread(seed),
                        SyndicDraftId::from_bytes([seed; 16]),
                        ExecutionBinding::new(
                            target.runtime_id(),
                            target.root_id(),
                            native(r"C:\Work\Beryl"),
                        ),
                        SyndicTimestamp::from_unix_millis(u64::from(seed)),
                        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                    ),
                ),
            );
        }
        let window = WindowId::from_bytes([5; 16]);
        execute(
            &store,
            state.session().initialize_threadless(
                state.session().revision(&store).unwrap(),
                InitializeThreadlessWindow::new(window, placement()),
            ),
        );
        Self {
            store,
            state,
            syndic,
            faults,
            window,
            target,
            _directory: directory,
        }
    }

    fn prepare(&self, selected: Option<WindowClaimSelection>, seed: u8) -> RunningThreadActivation {
        let preparation = RunningThreadActivation::prepare(
            &self.store,
            &self.state,
            &self.syndic,
            self.window,
            selected,
            self.target,
            thread(seed),
        )
        .unwrap();
        let RunningThreadActivationPreparation::Prepared(prepared) = preparation else {
            panic!("expected prepared, got {preparation:?}")
        };
        prepared
    }

    fn commit(
        &self,
        prepared: RunningThreadActivation,
    ) -> beryl_app::RunningThreadActivationCommit {
        let outcome = prepared.commit(
            &self.store,
            &self.state,
            &self.syndic,
            CommandCancellation::new(),
        );
        let RunningThreadActivationOutcome::Settled(settled) = outcome else {
            panic!("expected settled, got {outcome:?}")
        };
        settled
    }
}

#[test]
fn atomic_selection_and_catalog_join_survive_other_domain_flush_and_stale_rows() {
    let fixture = Fixture::new();
    let initial = fixture.commit(fixture.prepare(None, 3));
    let prepared = fixture.prepare(Some(initial.selection), 4);
    let expected = prepared.future_selection();
    let prior = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            thread(3),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    execute(
        &fixture.store,
        fixture.state.catalog().mark_stale(
            fixture.state.catalog().revision(&fixture.store).unwrap(),
            MarkCatalogRowStale::new(thread(3), prior.revision()),
        ),
    );
    let session_revision = fixture.state.session().revision(&fixture.store).unwrap();
    let syndic_revision = fixture.syndic.revision(&fixture.store).unwrap();
    let settled = fixture.commit(prepared);
    assert_eq!(settled.selection, expected);
    assert_eq!(settled.window.selected_thread(), Some(expected));
    assert_eq!(settled.claim.thread_id(), thread(4));
    assert_eq!(
        fixture.state.session().revision(&fixture.store).unwrap(),
        session_revision.checked_next().unwrap()
    );
    assert_eq!(
        fixture.syndic.revision(&fixture.store).unwrap(),
        syndic_revision
    );
    let old = fixture
        .state
        .catalog()
        .current_row_source(
            &fixture.store,
            thread(3),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    let new = fixture
        .state
        .catalog()
        .current_row_source(
            &fixture.store,
            thread(4),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(old.row().facts().claim(), CatalogClaimSummary::Unclaimed);
    assert_eq!(old.row().sources().claim(), None);
    assert_eq!(
        new.row().facts().claim(),
        CatalogClaimSummary::claimed(fixture.window, CatalogClaimKind::Active)
    );
    assert_eq!(new.row().sources().claim(), Some(expected.revision()));
    assert_eq!(
        fixture
            .state
            .session()
            .thread_claim_catalog_source(&fixture.store, thread(3))
            .unwrap()
            .claim(),
        None
    );
    assert!(
        matches!(RunningThreadActivation::prepare(&fixture.store,&fixture.state,&fixture.syndic,fixture.window,Some(expected),fixture.target,thread(4)).unwrap(),RunningThreadActivationPreparation::Current { claim,.. } if claim == settled.claim)
    );
}

#[test]
fn cancellation_and_predecessor_drift_leave_target_unclaimed() {
    let fixture = Fixture::new();
    let prepared = fixture.prepare(None, 3);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let revision = fixture.store.home_revision().unwrap();
    assert!(matches!(
        prepared.commit(
            &fixture.store,
            &fixture.state,
            &fixture.syndic,
            cancellation
        ),
        RunningThreadActivationOutcome::NotCommitted(_)
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert_eq!(
        fixture
            .state
            .session()
            .thread_claim_catalog_source(&fixture.store, thread(3))
            .unwrap()
            .claim(),
        None
    );
    let initial = fixture.commit(fixture.prepare(None, 3));
    let prepared = fixture.prepare(Some(initial.selection), 4);
    let original = fixture
        .state
        .session()
        .capture_window_removal(&fixture.store, fixture.window)
        .unwrap();
    execute(
        &fixture.store,
        fixture.state.session().update_placement(
            fixture.state.session().revision(&fixture.store).unwrap(),
            UpdateWindowPlacement::new(
                original.header().revision(),
                fixture.window,
                original.window().revision(),
                WindowPlacement::new(
                    WindowBounds::new(20, 0, 900, 700).unwrap(),
                    WindowDisplayState::Normal,
                    None,
                    None,
                ),
            ),
        ),
    );
    let revision = fixture.store.home_revision().unwrap();
    assert!(matches!(
        prepared.commit(
            &fixture.store,
            &fixture.state,
            &fixture.syndic,
            CommandCancellation::new()
        ),
        RunningThreadActivationOutcome::NotCommitted(RunningThreadActivationError::Session(_))
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert_eq!(
        fixture
            .state
            .session()
            .thread_claim_catalog_source(&fixture.store, thread(4))
            .unwrap()
            .claim(),
        None
    );
}

#[test]
fn acknowledgement_loss_retains_joined_audit_custody_until_exact_new() {
    let fixture = Fixture::new();
    let initial = fixture.commit(fixture.prepare(None, 3));
    let prepared = fixture.prepare(Some(initial.selection), 4);
    let expected = prepared.future_selection();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = prepared.commit(
        &fixture.store,
        &fixture.state,
        &fixture.syndic,
        CommandCancellation::new(),
    );
    let RunningThreadActivationOutcome::Pending(pending) = outcome else {
        panic!("expected retained custody, got {outcome:?}")
    };
    let outcome = pending.reconcile(&fixture.store, &fixture.state);
    let RunningThreadActivationOutcome::Settled(settled) = outcome else {
        panic!("expected exact-new join, got {outcome:?}")
    };
    assert_eq!(settled.selection, expected);
    assert_eq!(
        fixture
            .state
            .session()
            .thread_claim_catalog_source(&fixture.store, thread(3))
            .unwrap()
            .claim(),
        None
    );
    assert_eq!(
        fixture
            .state
            .session()
            .window_claim_catalog_source(&fixture.store, fixture.window)
            .unwrap()
            .claim(),
        Some(settled.claim)
    );
}
