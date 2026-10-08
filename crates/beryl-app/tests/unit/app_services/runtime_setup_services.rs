use super::*;
use crate::runtime_admission::{AdmissionError, RuntimeAdmissionOutcome};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeLaunchForm, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicThreadId, WindowId,
};
use std::path::PathBuf;
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1};
#[path = "runtime_setup_services/first_conversation.rs"]
mod first_conversation;

fn installed() -> (tempfile::TempDir, ProcessServiceOwner) {
    let (directory, candidate, state, syndic, _) = fixture();
    let mut process = owner(&candidate);
    process
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    drop(process.window_services(window_services::inputs()).unwrap());
    (directory, process)
}

fn await_flight(flight: &runtime_setup::RuntimeSetupFlight) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while flight.is_pending() {
        assert!(
            Instant::now() < deadline,
            "original runtime setup worker did not settle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn runtime_setup_captures_invoking_source_on_its_worker_without_partial_registration() {
    let (directory, mut process) = installed();
    let services = process.runtime_setup_services().unwrap();
    let before = services.catalog_store().home_revision().unwrap();
    let flight = services
        .start_add_runtime_for_window(
            WindowId::from_bytes([211; 16]),
            vec![WindowId::from_bytes([211; 16])],
            PathBuf::from("unused.exe"),
            RuntimeLaunchForm::CodexCli,
        )
        .unwrap();
    await_flight(&flight);
    assert!(matches!(
        flight.take_outcome(),
        Some(RuntimeAdmissionOutcome::NotCommitted {
            error: AdmissionError::Source(_)
        })
    ));
    assert_eq!(services.catalog_store().home_revision().unwrap(), before);
    assert!(flight.failure().is_none());
    drop((flight, services));
    close(&mut process);
    drop(process);
    assert_reopens(&directory);
}

#[test]
fn completed_runtime_setup_flights_release_capacity_for_later_commands() {
    let (directory, mut process) = installed();
    let service = process.graph().unwrap().runtime_setup();
    for _ in 0..=beryl_state::MAX_RESTORABLE_WINDOWS {
        let flight = service.retain_test_outcome(RuntimeAdmissionOutcome::Existing {
            runtime_id: RuntimeId::from_bytes([212; 16]),
            root_id: None,
        });
        assert!(flight.take_outcome().is_some());
    }
    let services = process.runtime_setup_services().unwrap();
    let flight = services
        .start_add_root_for_window(
            WindowId::from_bytes([213; 16]),
            vec![WindowId::from_bytes([213; 16])],
            RuntimeId::from_bytes([212; 16]),
            PathBuf::from("unused"),
        )
        .unwrap();
    await_flight(&flight);
    assert!(matches!(
        flight.take_outcome(),
        Some(RuntimeAdmissionOutcome::NotCommitted { .. })
    ));
    drop((flight, services, service));
    close(&mut process);
    drop(process);
    assert_reopens(&directory);
}

#[test]
fn healthy_runtime_setup_retirement_joins_original_worker_and_fences_retained_handles() {
    let (directory, mut process) = installed();
    let services = process.runtime_setup_services().unwrap();
    let service = process.graph().unwrap().runtime_setup();
    let flight = services
        .start_add_runtime_for_window(
            WindowId::from_bytes([214; 16]),
            vec![WindowId::from_bytes([214; 16])],
            PathBuf::from("unused.exe"),
            RuntimeLaunchForm::CodexCli,
        )
        .unwrap();
    flight.cancellation().cancel();
    let mut cleanups = Vec::new();
    service.retire(&mut cleanups).unwrap();
    assert!(cleanups.is_empty());
    assert!(!flight.is_pending());
    assert!(flight.take_outcome().is_none());
    assert!(!services.current());
    assert!(
        services
            .start_add_runtime_for_window(
                WindowId::from_bytes([214; 16]),
                vec![WindowId::from_bytes([214; 16])],
                PathBuf::from("unused.exe"),
                RuntimeLaunchForm::StandaloneAppServer
            )
            .is_err()
    );
    drop((flight, service, services));
    close(&mut process);
    drop(process);
    assert_reopens(&directory);
}

#[test]
fn runtime_setup_reconciliation_refuses_a_settled_outcome_without_discarding_it() {
    let (directory, mut process) = installed();
    let service = process.graph().unwrap().runtime_setup();
    let flight = service.retain_test_outcome(RuntimeAdmissionOutcome::Existing {
        runtime_id: RuntimeId::from_bytes([215; 16]),
        root_id: None,
    });
    flight.start_reconciliation().unwrap();
    await_flight(&flight);
    assert!(flight.failure().is_some());
    assert!(matches!(
        flight.take_outcome(),
        Some(RuntimeAdmissionOutcome::Existing { .. })
    ));
    drop((flight, service));
    close(&mut process);
    drop(process);
    assert_reopens(&directory);
}

#[test]
fn runtime_setup_page_election_refuses_a_mutated_observation_and_accepts_fresh_coherence() {
    let (directory, mut process) = installed();
    process
        .graph_mut()
        .unwrap()
        .catalog_source
        .as_mut()
        .unwrap()
        .stop_and_join()
        .unwrap();
    let services = process.runtime_setup_services().unwrap();
    let original = services.observe().unwrap();
    let graph = process.graph().unwrap();
    let home = graph.home();
    let storage = graph.syndic();
    let thread = CreateThread::ordinary(
        SyndicThreadId::from_bytes([216; 16]),
        SyndicDraftId::from_bytes([217; 16]),
        ExecutionBinding::new(
            RuntimeId::from_bytes([218; 16]),
            RootId::from_bytes([219; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                r"C:\Work\Beryl",
            )
            .unwrap(),
        ),
        SyndicTimestamp::from_unix_millis(1),
        DraftEditHistoryPolicyV1::new(8 * 1024 * 1024, 1).unwrap(),
    );
    let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
    command
        .add(storage.create_thread(storage.revision(home).unwrap(), thread))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    let published = std::cell::Cell::new(false);
    assert!(services.elect(&original, || published.set(true)).is_err());
    assert!(!published.get());
    let fresh = services.observe().unwrap();
    assert_eq!(
        services
            .elect(&fresh, || {
                published.set(true);
                17
            })
            .unwrap(),
        17
    );
    assert!(published.get());
    drop(services);
    close(&mut process);
    drop(process);
    assert_reopens(&directory);
}
