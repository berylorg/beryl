use super::*;
use crate::main_window::MainWindowCreationRequestContext;
use beryl_home_store::{HomeStore, test_faults::FaultPoint};
use beryl_model::{PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath};
use beryl_state::{BerylState, RememberedTarget};
use syndic_storage::SyndicStorage;

#[path = "../main_window_shell/support.rs"]
mod home_support;

fn native_path(mode: RuntimeMode, path: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(mode, PathFlavor::Windows, path).unwrap()
}
fn placement() -> beryl_model::WindowPlacement {
    inputs::placement()
}

pub(super) fn selected_home() -> (
    tempfile::TempDir,
    beryl_model::WindowId,
    beryl_model::SyndicThreadId,
) {
    let (directory, home, state, storage, faults) = home_support::open_home(91);
    let process =
        crate::window_acquisition::RuntimeBackedWindowProcessRegistry::new(Default::default());
    let reference = home.service_reference();
    let service = crate::window_acquisition::RuntimeBackedWindowAcquisitionService::new(
        &process,
        Arc::new(home.service_reference()),
        state.clone(),
        storage.clone(),
    );
    let window = beryl_model::WindowId::from_bytes([93; 16]);
    let request = (inputs::windows().request_source)(
        window,
        RememberedTarget::new(
            RuntimeId::from_bytes([91; 16]),
            RootId::from_bytes([92; 16]),
        ),
        MainWindowCreationRequestContext::new(&reference, &state),
    )
    .unwrap();
    let crate::window_acquisition::RuntimeBackedWindowAcquisitionOutcome::Committed {
        acquisition,
        ..
    } = service.acquire(request, beryl_home_store::CommandCancellation::new())
    else {
        panic!("selected home acquisition must commit")
    };
    let thread = acquisition.thread_id();
    drop((
        acquisition,
        service,
        process,
        reference,
        state,
        storage,
        faults,
    ));
    Arc::try_unwrap(home).ok().unwrap().close().unwrap();
    (directory, window, thread)
}

#[test]
fn immutable_production_source_rebinds_to_recovered_graph_and_refuses_old_or_foreign_state() {
    let (directory, home, state, storage, faults) = home_support::open_home(81);
    let (foreign_directory, foreign_home, foreign_state, foreign_storage, foreign_faults) =
        home_support::open_home(82);
    let source = inputs::windows().request_source;
    let target = RememberedTarget::new(
        RuntimeId::from_bytes([81; 16]),
        RootId::from_bytes([82; 16]),
    );
    let window = beryl_model::WindowId::from_bytes([83; 16]);
    let reference = home.service_reference();
    let original = source(
        window,
        target,
        MainWindowCreationRequestContext::new(&reference, &state),
    )
    .unwrap();
    let generation = home.health().generation();
    assert_eq!(
        original.fallback_execution().root_path(),
        &native_path(RuntimeMode::host(), r"C:\Work\Beryl")
    );
    assert!(
        source(
            window,
            target,
            MainWindowCreationRequestContext::new(&reference, &foreign_state)
        )
        .is_err()
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    assert!(
        source(
            window,
            target,
            MainWindowCreationRequestContext::new(&reference, &state)
        )
        .is_err()
    );
    drop(reference);
    drop(storage);
    let candidate = Arc::try_unwrap(home)
        .ok()
        .unwrap()
        .recover_same_home()
        .unwrap();
    let replacement_state = BerylState::reacquire_candidate(&candidate).unwrap();
    let replacement_storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let replacement = candidate.publish().unwrap();
    assert_ne!(replacement.health().generation(), generation);
    let reference = replacement.service_reference();
    assert!(
        source(
            window,
            target,
            MainWindowCreationRequestContext::new(&reference, &state)
        )
        .is_err()
    );
    let restored = source(
        window,
        target,
        MainWindowCreationRequestContext::new(&reference, &replacement_state),
    )
    .unwrap();
    assert_eq!(restored.fallback_execution(), original.fallback_execution());
    assert_ne!(restored.fallback_thread_id(), original.fallback_thread_id());
    drop((reference, replacement_state, replacement_storage, state));
    replacement.close().unwrap();
    drop((foreign_state, foreign_storage, foreign_faults));
    Arc::try_unwrap(foreign_home).ok().unwrap().close().unwrap();
    directory.close().unwrap();
    foreign_directory.close().unwrap();
}
