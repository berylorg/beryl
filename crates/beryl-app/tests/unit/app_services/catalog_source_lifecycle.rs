use super::*;
use crate::catalog_readiness::{CatalogSourceCoordinatorError, RetainedCatalogRepair};
use crate::catalog_readiness::{CatalogSourceReadError, CatalogSourceReader};
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{WindowBounds, WindowDisplayState, WindowId, WindowPlacement};
use beryl_state::InitializeThreadlessWindow;

fn seed_missing_catalog_projection(graph: &PublishedAppServices) -> beryl_model::SyndicThreadId {
    use beryl_model::{
        AdmittedHostPath, Availability, ExecutionBinding, PathFlavor, RootId, RuntimeId,
        RuntimeLaunchForm, RuntimeMode, RuntimeNativePath, SyndicDraftId, SyndicThreadId,
    };
    use beryl_state::{
        AvailabilitySnapshot, CreateRuntimeWithHomeRoot, RootRegistration, RuntimeRegistration,
        UnixMillis,
    };
    use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1};

    let thread = SyndicThreadId::from_bytes([170; 16]);
    let runtime = RuntimeId::from_bytes([172; 16]);
    let root = RootId::from_bytes([173; 16]);
    let native = |path| {
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path).unwrap()
    };
    let host = |path| AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap();
    let available =
        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(1)).unwrap();
    let registration = CreateRuntimeWithHomeRoot::new(
        RuntimeRegistration::new(
            runtime,
            host(r"C:\Codex\codex.exe"),
            RuntimeMode::host(),
            RuntimeLaunchForm::CodexCli,
            native(r"C:\Codex\codex.exe"),
            UnixMillis::new(1),
            available,
        )
        .unwrap(),
        RootRegistration::new(
            root,
            native(r"C:\work\beryl"),
            host(r"C:\work\beryl"),
            UnixMillis::new(1),
            available,
        ),
    )
    .unwrap();
    let storage = graph.syndic();
    let home = graph.home();
    let runtime_roots = graph.state().runtime_roots();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(
            runtime_roots
                .create_runtime_with_home_root(runtime_roots.revision(home).unwrap(), registration),
        )
        .unwrap();
    command
        .add(storage.create_thread(
            storage.revision(home).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([171; 16]),
                ExecutionBinding::new(runtime, root, native(r"C:\work\beryl")),
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed { .. }
    ));
    thread
}

#[test]
fn held_initial_catalog_fence_coalesces_wakes_without_repair_before_publication() {
    let (directory, candidate, state, syndic, _) = fixture();
    let mut owner = owner(&candidate);
    let start = owner
        .open_initial_for_startup(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    let graph = owner.graph().unwrap();
    assert!(graph.cas().catalog_source_start_gate().wait());
    let reader = graph.catalog_source_reader();
    let thread = seed_missing_catalog_projection(graph);
    let revision = graph.home().home_revision().unwrap();
    for _ in 0..8 {
        graph.catalog_source.as_ref().unwrap().waker().wake_by_ref();
    }
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::NotReady)
    ));
    assert!(
        graph
            .state()
            .catalog()
            .row(
                graph.home(),
                thread,
                beryl_state::CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(graph.home().home_revision().unwrap(), revision);
    assert!(start.release());
    wait_for_certification(&reader, 1);
    let graph = owner.graph().unwrap();
    assert!(
        graph
            .state()
            .catalog()
            .row(
                graph.home(),
                thread,
                beryl_state::CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .is_some()
    );
    close(&mut owner);
    assert_reopens(&directory);
}

#[test]
fn cancelled_initial_catalog_fence_joins_without_cancelling_published_cas_start() {
    let (directory, candidate, state, syndic, _) = fixture();
    let mut owner = owner(&candidate);
    let start = owner
        .open_initial_for_startup(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    let graph = owner.graph().unwrap();
    let reader = graph.catalog_source_reader();
    let thread = seed_missing_catalog_projection(graph);
    let revision = graph.home().home_revision().unwrap();
    drop(start);
    owner.drain_initial_catalog_source();
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::Retired)
    ));
    let graph = owner.graph().unwrap();
    assert!(graph.cas().catalog_source_start_gate().wait());
    assert_eq!(graph.home().home_revision().unwrap(), revision);
    assert!(
        graph
            .state()
            .catalog()
            .row(
                graph.home(),
                thread,
                beryl_state::CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .is_none()
    );
    close(&mut owner);
    assert_reopens(&directory);
}

#[test]
fn headless_initial_open_certifies_catalog_without_native_publication_fence() {
    let (directory, mut owner, _) = recovery_support::installed();
    let reader = owner.graph().unwrap().catalog_source_reader();
    wait_for_certification(&reader, 0);
    close(&mut owner);
    assert_reopens(&directory);
}

fn wait_for_certification(reader: &CatalogSourceReader, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match reader.certified_threads() {
            Ok(threads) if threads == expected => return,
            Err(CatalogSourceReadError::NotReady) => {
                assert!(Instant::now() < deadline, "catalog did not certify");
                std::thread::sleep(Duration::from_millis(10));
            }
            result => panic!("unexpected catalog certification: {result:?}"),
        }
    }
}

#[test]
fn private_graph_keeps_catalog_worker_gated_and_disposal_retires_escaped_reader() {
    let (directory, candidate, state, syndic, _) = fixture();
    let owner = owner(&candidate);
    let prepared = preparation::PreparedAppServices::prepare(
        &owner,
        candidate,
        state,
        syndic,
        configuration(),
        SyndicTimestamp::from_unix_millis(1),
        &CommandCancellation::new(),
    )
    .unwrap();
    let reader = prepared.catalog_source_reader();
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::NotReady)
    ));
    prepared.dispose().unwrap();
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::Retired)
    ));
    assert_reopens(&directory);
}

#[test]
fn cancelled_graph_start_releases_catalog_and_home_before_escaped_reader_drops() {
    let (directory, candidate, state, syndic, _) = fixture();
    let owner = owner(&candidate);
    let prepared = preparation::PreparedAppServices::prepare(
        &owner,
        candidate,
        state,
        syndic,
        configuration(),
        SyndicTimestamp::from_unix_millis(1),
        &CommandCancellation::new(),
    )
    .unwrap();
    let (graph, start) = prepared.publish(&CommandCancellation::new()).unwrap();
    let reader = graph.catalog_source_reader();
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::NotReady)
    ));
    drop(start);
    graph.dispose_unstarted().unwrap();
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::Retired)
    ));
    assert_reopens(&directory);
}

#[test]
fn published_shutdown_drains_catalog_and_revokes_reader_with_retained_frozen_token() {
    let (directory, mut owner, _) = recovery_support::installed();
    let graph = owner.graph().unwrap();
    let reader = graph.catalog_source_reader();
    wait_for_certification(&reader, 0);
    let retained = reader
        .retain_source(graph.home(), &CommandCancellation::new())
        .unwrap();
    close(&mut owner);
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::Retired)
    ));
    assert_reopens(&directory);
    drop(retained);
}

#[test]
fn failed_graph_retirement_drains_catalog_before_same_home_recovery() {
    let (directory, mut owner, faults) = recovery_support::installed();
    let reader = owner.graph().unwrap().catalog_source_reader();
    wait_for_certification(&reader, 0);
    let expected = owner.graph().unwrap().home().health().generation().unwrap();
    owner
        .graph
        .as_mut()
        .unwrap()
        .catalog_source
        .as_mut()
        .unwrap()
        .stop_and_join()
        .unwrap();
    owner
        .graph
        .as_mut()
        .unwrap()
        .handoff
        .as_mut()
        .unwrap()
        .shutdown()
        .unwrap();
    recovery_support::fail(&owner, &faults);
    owner.retire_failed_service_graph(expected).unwrap();
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::Retired)
    ));
    let candidate = owner.recover_retired_service_home(expected).unwrap();
    assert_ne!(candidate.generation(), expected);
    candidate.abort().close().unwrap();
    assert_reopens(&directory);
}

fn drain_fixture_workers(owner: &mut ProcessServiceOwner) {
    let graph = owner.graph.as_mut().unwrap();
    graph
        .catalog_source
        .as_mut()
        .unwrap()
        .stop_and_join()
        .unwrap();
    graph.handoff.as_mut().unwrap().shutdown().unwrap();
}

#[test]
fn known_original_noncommit_survives_cancelled_recovery_settlement() {
    let (directory, mut owner, faults) = recovery_support::installed();
    drain_fixture_workers(&mut owner);
    let graph = owner.graph().unwrap();
    let expected = graph.home().health().generation().unwrap();
    let CommandOutcome::NotCommitted { evidence } = graph
        .home()
        .execute(HomeCommand::new(graph.home().home_revision().unwrap()))
    else {
        panic!("empty original command must be rejected");
    };
    recovery_support::fail(&owner, &faults);
    owner.retire_failed_service_graph(expected).unwrap();
    owner.test_catalog_retirement_failure(CatalogSourceCoordinatorError::Repair(Box::new(
        RetainedCatalogRepair::NotCommitted(evidence),
    )));
    let mut candidate = owner.recover_retired_service_home(expected).unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        owner.settle_retired_process_work(
            &candidate.recovery_access().unwrap(),
            &state,
            &syndic,
            &cancelled
        ),
        Err(crate::app_services::recovery_retirement::RetiredProcessWorkError::Cancelled)
    ));
    assert!(owner.require_catalog_recovery_settlement(expected).is_err());
    owner
        .settle_retired_process_work(
            &candidate.recovery_access().unwrap(),
            &state,
            &syndic,
            &CommandCancellation::new(),
        )
        .unwrap();
    owner.require_catalog_recovery_settlement(expected).unwrap();
    candidate.abort().close().unwrap();
    assert_reopens(&directory);
}

#[test]
fn installed_original_indeterminate_handle_requires_exact_candidate_reconciliation() {
    let (directory, mut owner, faults) = recovery_support::installed();
    drain_fixture_workers(&mut owner);
    let graph = owner.graph().unwrap();
    let home = graph.home();
    let expected = home.health().generation().unwrap();
    let session = graph.state().session();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(session.initialize_threadless(
            session.revision(home).unwrap(),
            InitializeThreadlessWindow::new(
                WindowId::from_bytes([161; 16]),
                WindowPlacement::new(
                    WindowBounds::new(0, 0, 800, 600).unwrap(),
                    WindowDisplayState::Normal,
                    None,
                    None,
                ),
            ),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate {
        failure,
        reconciliation,
    } = home.execute(command)
    else {
        panic!("faulted original command must retain indeterminate outcome");
    };
    let original = reconciliation.install_and_handle();
    assert_eq!(home.pending_reconciliations().len(), 1);
    assert_eq!(home.health().state(), HomeHealthState::Healthy);
    recovery_support::fail(&owner, &faults);
    owner.retire_failed_service_graph(expected).unwrap();
    owner.test_catalog_retirement_failure(CatalogSourceCoordinatorError::Repair(Box::new(
        RetainedCatalogRepair::Indeterminate {
            failure,
            reconciliation: original,
        },
    )));
    assert!(owner.require_catalog_recovery_settlement(expected).is_err());
    let mut candidate = owner.recover_retired_service_home(expected).unwrap();
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .pending_reconciliations()
            .len(),
        1
    );
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    owner
        .settle_retired_process_work(
            &candidate.recovery_access().unwrap(),
            &state,
            &syndic,
            &CommandCancellation::new(),
        )
        .unwrap();
    owner.require_catalog_recovery_settlement(expected).unwrap();
    candidate.abort().close().unwrap();
    assert_reopens(&directory);
}
