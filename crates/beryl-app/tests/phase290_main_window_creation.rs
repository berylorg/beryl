#![cfg(feature = "test-faults")]

#[path = "phase186_pending_composer_activation/support.rs"]
mod composer_support;
#[path = "phase290_main_window_creation/gpui.rs"]
mod gpui_cases;
#[path = "phase289_main_window_shell/support.rs"]
mod home_support;
#[path = "phase295_initial_composer/support.rs"]
mod initial_support;
#[path = "phase290_main_window_creation/support.rs"]
mod support;

use beryl_app::main_window::*;
use beryl_app::window_acquisition::*;
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{
    ExecutionBinding, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{BerylState, RememberedTarget};
use initial_support::{Fixture, config, execute, native_path, placement};
use std::sync::Arc;
use support::*;
use syndic_storage::{
    DraftEditHistoryPolicyV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionReadOutcomeV1, SyndicStorage, SyndicTimestamp,
};

#[test]
fn worker_cancellation_releases_exact_acquisition_and_reservation() {
    let fixture = Fixture::new(11);
    let (services, appearance) = services(&fixture);
    let window = WindowId::from_bytes([12; 16]);
    let creation = MainWindowCreation::admit(services.clone(), window, target(&fixture)).unwrap();
    let cancellation = creation.cancellation();
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert!(MainWindowCreation::admit(services.clone(), window, target(&fixture)).is_err());
    let MainWindowCreationOutcome::Prepared { prepared, .. } = creation.advance(appearance.clone())
    else {
        panic!("creation prepares the exact first editor")
    };
    assert_eq!(prepared.window_id(), window);
    cancellation.cancel();
    let abandoned = MainWindowCreation::abandon(
        services.clone(),
        prepared.into_unpublished(),
        "cancelled".to_owned(),
    );
    settle(abandoned, appearance);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    assert!(
        fixture
            .state
            .session()
            .minimal_bootstrap(&fixture.store)
            .unwrap()
            .unwrap()
            .windows()
            .is_empty()
    );
    drop(services);
    cleanup(fixture);
}

#[test]
fn failed_preparation_preserves_reused_pristine_thread_and_releases_window() {
    let fixture = Fixture::new(21);
    let pristine = fixture.seed_pristine(22);
    let (mut services, appearance) = services(&fixture);
    Arc::get_mut(&mut services).unwrap().configurator_source =
        Arc::new(|| Box::new(|_| Err("preparation rejected".to_owned())));
    let creation = MainWindowCreation::admit(
        services.clone(),
        WindowId::from_bytes([23; 16]),
        target(&fixture),
    )
    .unwrap();
    settle(creation, appearance);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    let acquisition = fixture.acquire(24);
    assert_eq!(acquisition.thread_id(), pristine);
    let reservation = fixture
        .process
        .reserve_main_window(acquisition.window_id())
        .unwrap();
    let initial = fixture.from_acquired(acquisition, reservation, 24, false);
    fixture.retire_and_release(initial);
    drop(services);
    cleanup(fixture);
}

#[test]
fn wrong_request_identity_has_no_durable_effect() {
    let fixture = Fixture::new(31);
    let (mut services, appearance) = services(&fixture);
    let original = services.request_source.clone();
    Arc::get_mut(&mut services).unwrap().request_source =
        Arc::new(move |_, target| original(WindowId::from_bytes([99; 16]), target));
    let revision = fixture.store.home_revision().unwrap();
    let creation = MainWindowCreation::admit(
        services.clone(),
        WindowId::from_bytes([32; 16]),
        target(&fixture),
    )
    .unwrap();
    let MainWindowCreationOutcome::Settled {
        error: Some(error), ..
    } = creation.advance(appearance)
    else {
        panic!("mismatched request is refused")
    };
    assert!(error.contains("captured target"));
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    drop(services);
    cleanup(fixture);
}

#[test]
fn cancelled_admission_never_invokes_request_source() {
    let fixture = Fixture::new(41);
    let (mut services, appearance) = services(&fixture);
    Arc::get_mut(&mut services).unwrap().request_source =
        Arc::new(|_, _| panic!("cancelled request source"));
    let revision = fixture.store.home_revision().unwrap();
    let creation = MainWindowCreation::admit(
        services.clone(),
        WindowId::from_bytes([42; 16]),
        target(&fixture),
    )
    .unwrap();
    creation.cancellation().cancel();
    settle(creation, appearance);
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    drop(services);
    cleanup(fixture);
}

#[test]
fn indeterminate_acquisition_retains_custody_through_cancellation_and_reconciliation() {
    let fixture = Fixture::new(51);
    let (services, appearance) = services(&fixture);
    fixture
        .faults
        .fail_next(beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist);
    let creation = MainWindowCreation::admit(
        services.clone(),
        WindowId::from_bytes([52; 16]),
        target(&fixture),
    )
    .unwrap();
    let cancellation = creation.cancellation();
    let MainWindowCreationOutcome::Pending(creation) = creation.advance(appearance.clone()) else {
        panic!("acknowledgement loss preserves operation custody")
    };
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    cancellation.cancel();
    settle(creation, appearance);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    assert!(
        fixture
            .state
            .session()
            .minimal_bootstrap(&fixture.store)
            .unwrap()
            .unwrap()
            .windows()
            .is_empty()
    );
    drop(services);
    cleanup(fixture);
}
