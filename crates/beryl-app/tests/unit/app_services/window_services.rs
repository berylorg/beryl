use super::*;
use crate::{main_window::*, theme_runtime::AppearanceGeneration};
use beryl_model::{RootId, RuntimeId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement};
use beryl_state::RememberedTarget;

fn inputs() -> MainWindowServiceInputs {
    MainWindowServiceInputs {
        request_source: Arc::new(|_, _| panic!("request source must remain uncalled")),
        activation_source: Arc::new(|_| Err("no acquired editor in this fixture".to_owned())),
        restored_activation_source: Arc::new(|_| {
            Err("no restored editor in this fixture".to_owned())
        }),
        configurator_source: Arc::new(|| Box::new(|_| Err("no editor in this fixture".to_owned()))),
    }
}

fn installed() -> (tempfile::TempDir, ProcessServiceOwner, FaultController) {
    let (directory, candidate, state, syndic, faults) = fixture();
    let mut owner = owner(&candidate);
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    (directory, owner, faults)
}

fn appearance(owner: &mut ProcessServiceOwner) -> Arc<AppearanceGeneration> {
    let graph = owner.graph_mut().unwrap();
    let revision = graph.state().settings().revision(graph.home()).unwrap();
    graph.load_theme(revision, None).unwrap();
    graph.theme().unwrap().current().unwrap()
}

fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(0, 0, 800, 600).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn target() -> RememberedTarget {
    RememberedTarget::new(
        RuntimeId::from_bytes([11; 16]),
        RootId::from_bytes([12; 16]),
    )
}

fn dispose(mut work: MainWindowRestoreSet) {
    for _ in 0..32 {
        match work.advance() {
            MainWindowRestoreSetOutcome::Pending(next) => work = next,
            MainWindowRestoreSetOutcome::Failed { .. } => return,
            MainWindowRestoreSetOutcome::Retained { reason, .. } => panic!("retained: {reason:?}"),
            MainWindowRestoreSetOutcome::Prepared(_) => panic!("disposal returned preparation"),
        }
    }
    panic!("restore disposal did not settle");
}

#[test]
fn graph_bundle_transfers_to_worker_and_prepares_an_exact_threadless_set() {
    let (directory, mut owner, _) = installed();
    let appearance = appearance(&mut owner);
    let bundle = owner.window_services(inputs()).unwrap();
    let services = bundle.creation_services();
    assert!(Arc::ptr_eq(
        &services.store,
        &services.acquisition.home_reference()
    ));
    assert_eq!(
        services.store.home_id(),
        owner.graph().unwrap().home().home_id()
    );
    assert!(services.submission_execution.matches_binding(
        services.store.home_id(),
        services.store.health().generation().unwrap()
    ));
    assert!(!services.submission_execution.matches_binding(
        beryl_model::BerylHomeId::from_bytes([99; 16]),
        services.store.health().generation().unwrap()
    ));
    assert_eq!(
        services
            .marker_seals
            .diagnostics()
            .configured_flight_limit(),
        owner
            .graph()
            .unwrap()
            .marker()
            .diagnostics()
            .configured_flight_limit()
    );
    assert_eq!(
        services.turn_start_requirement,
        owner
            .graph()
            .unwrap()
            .cas()
            .config()
            .turn_start_admission_requirement()
    );
    let registry = services.acquisition.process_registry();
    std::thread::spawn(move || {
        let mut work = bundle
            .into_restore_set(appearance, WindowId::from_bytes([31; 16]), placement())
            .unwrap();
        for _ in 0..32 {
            match work.advance() {
                MainWindowRestoreSetOutcome::Pending(next) => work = next,
                MainWindowRestoreSetOutcome::Prepared(prepared) => {
                    assert_eq!(prepared.members().len(), 1);
                    prepared.revalidate().unwrap();
                    dispose(prepared.dispose());
                    return;
                }
                MainWindowRestoreSetOutcome::Failed { error } => panic!("failed: {error}"),
                MainWindowRestoreSetOutcome::Retained { reason, .. } => {
                    panic!("retained: {reason:?}")
                }
            }
        }
        panic!("restore preparation did not settle");
    })
    .join()
    .unwrap();
    assert_eq!(registry.main_window_occupancy(), 0);
    close(&mut owner);
    assert_reopens(&directory);
}

#[test]
fn graph_bundles_share_window_reservations_and_reject_retired_services_after_retry() {
    let (directory, mut owner, _) = installed();
    let appearance = appearance(&mut owner);
    let first = owner.window_services(inputs()).unwrap();
    let old = first.creation_services();
    let second = owner.window_services(inputs()).unwrap().creation_services();
    let id = WindowId::from_bytes([41; 16]);
    let work = MainWindowCreation::admit(old.clone(), id, target()).unwrap();
    assert!(MainWindowCreation::admit(second.clone(), id, target()).is_err());
    assert_eq!(
        second
            .acquisition
            .process_registry()
            .main_window_occupancy(),
        1
    );
    work.cancellation().cancel();
    assert!(matches!(
        work.advance(appearance.clone()),
        MainWindowCreationOutcome::Settled { .. }
    ));
    let registry = old.acquisition.process_registry();
    assert_eq!(registry.main_window_occupancy(), 0);
    close(&mut owner);
    let (candidate, state, syndic) = super::reopening::candidate_at(&directory);
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
        )
        .unwrap();
    let fresh = owner.window_services(inputs()).unwrap().creation_services();
    let reservation = registry.reserve_main_window(id).unwrap();
    assert!(MainWindowCreation::admit(fresh.clone(), id, target()).is_err());
    drop(reservation);
    assert!(MainWindowCreation::admit(old, id, target()).is_err());
    assert_eq!(registry.main_window_occupancy(), 0);
    assert!(first.into_restore_set(appearance, id, placement()).is_err());
    close(&mut owner);
}

#[test]
fn published_window_factory_does_not_read_storage_and_rejects_unavailable_graphs() {
    let (directory, candidate, state, syndic, faults) = fixture();
    let mut owner = owner(&candidate);
    assert!(owner.window_services(inputs()).is_err());
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    let bundle = owner.window_services(inputs()).unwrap();
    let services = bundle.creation_services();
    assert!(services.state.settings().revision(&services.store).is_err());
    assert!(MainWindowCreation::admit(services, WindowId::from_bytes([51; 16]), target()).is_err());
    drop(owner.graph.take());
    assert!(owner.window_services(inputs()).is_err());
    assert_reopens(&directory);
}

#[test]
fn shutdown_rejects_new_bundles_and_cancels_unstarted_creation_without_losing_reservation() {
    let (_directory, mut owner, _) = installed();
    let appearance = appearance(&mut owner);
    let bundle = owner.window_services(inputs()).unwrap();
    let services = bundle.creation_services();
    let registry = services.acquisition.process_registry();
    let creation =
        MainWindowCreation::admit(services, WindowId::from_bytes([61; 16]), target()).unwrap();
    owner.begin_shutdown().unwrap();
    assert!(owner.window_services(inputs()).is_err());
    assert!(
        bundle
            .into_restore_set(
                appearance.clone(),
                WindowId::from_bytes([62; 16]),
                placement()
            )
            .is_err()
    );
    assert!(matches!(
        creation.advance(appearance),
        MainWindowCreationOutcome::Settled { .. }
    ));
    assert_eq!(registry.main_window_occupancy(), 0);
    close(&mut owner);
}

#[test]
fn restoration_worker_rejects_graph_retirement_during_its_first_read() {
    let (_directory, mut owner, faults) = installed();
    let appearance = appearance(&mut owner);
    let bundle = owner.window_services(inputs()).unwrap();
    let registry = bundle.creation_services().acquisition.process_registry();
    let reading = faults.block_next(FaultPoint::BeforeReadConfirmation);
    let worker = std::thread::spawn(move || {
        bundle.into_restore_set(appearance, WindowId::from_bytes([71; 16]), placement())
    });
    let reached = reading.wait_until_reached(Duration::from_secs(5));
    if reached {
        owner.begin_shutdown().unwrap();
    }
    reading.release();
    let result = worker.join().unwrap();
    assert!(reached, "restoration worker never reached its first read");
    assert!(result.is_err());
    assert_eq!(registry.main_window_occupancy(), 0);
    close(&mut owner);
}
