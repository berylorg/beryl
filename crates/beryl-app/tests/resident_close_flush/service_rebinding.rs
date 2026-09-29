use super::support::slot_close::retired;
use super::widget_support::fixture::Fixture;
use beryl_app::{
    composer_host::{ComposerHostFlushAdmission, ComposerHostFlushState},
    main_window::MainWindowConversationComposerService as Service,
};
use beryl_home_store::{HomeHealthState, test_faults::FaultPoint};
use beryl_state::BerylState;
use gpui::{AppContext, TestAppContext};
use syndic_storage::SyndicStorage;

#[gpui::test]
fn reconstructed_service_keeps_all_close_gates_through_publication(cx: &mut TestAppContext) {
    let fixture = Fixture::new("rebind-retired-service", 181);
    let mut facts = retired(&fixture, cx.new(|_| ()).entity_id());
    let old_selection = facts.selection();
    let old_close = facts.close_ticket();
    let old_state = BerylState::reacquire(&fixture.store).unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    facts = Service::rebind_candidate(&mut candidate, facts, storage.clone(), &old_state)
        .err()
        .unwrap()
        .0;
    assert_eq!(facts.selection(), old_selection);
    facts = Service::rebind_candidate(&mut candidate, facts, fixture.storage.clone(), &state)
        .err()
        .unwrap()
        .0;
    assert_eq!(facts.close_ticket(), old_close);
    let (service, close, window) =
        Service::rebind_candidate(&mut candidate, facts, storage, &state)
            .unwrap_or_else(|(_, error)| panic!("service reconstruction failed: {error}"));
    let selected = service.selected_identity().unwrap();
    assert_eq!(window.window_id(), selected.window_id());
    assert_eq!(window.selected_thread(), Some(selected.claim()));
    assert_eq!(selected.window_id(), old_selection.window_id());
    assert_eq!(selected.claim(), old_selection.claim());
    assert_ne!(selected.binding(), old_selection.binding());
    let home = service.test_retain_home_reference();
    assert_eq!(home.home_id(), selected.binding().home_id());
    assert_eq!(home.health().state(), HomeHealthState::Reopening);
    assert_eq!(
        home.health().generation(),
        Some(selected.binding().home_generation())
    );
    assert!(home.home_revision().is_err());
    assert!(service.test_window_close_is_current(close));
    assert!(!service.test_window_close_is_current(old_close));
    assert!(service.test_begin_window_close_flush(old_close).is_err());
    assert_eq!(
        service
            .test_release_window_close_gate(old_close, None)
            .unwrap(),
        Some(false)
    );
    assert!(matches!(
        service.test_begin_window_close_flush(close).unwrap(),
        Some(ComposerHostFlushAdmission::Joined {
            state: ComposerHostFlushState::CaptureRequired,
            ..
        })
    ));
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    let store = candidate.publish().unwrap();
    assert!(service.test_window_close_is_current(close));
    assert!(service.test_begin_window_close_gate(close).is_err());
    assert_eq!(store.home_revision().unwrap(), revision);
    drop(home);
    drop(service);
    store.close().unwrap();
}

#[gpui::test]
fn refused_service_reconstruction_retains_facts_for_foreign_or_failed_candidates(
    cx: &mut TestAppContext,
) {
    for fail_read in [false, true] {
        let fixture = Fixture::new("refuse-service-rebind", 183);
        let facts = retired(&fixture, cx.new(|_| ()).entity_id());
        let selection = facts.selection();
        let close = facts.close_ticket();
        let foreign = Fixture::new("foreign-service-rebind", 184);
        let source = if fail_read { fixture } else { foreign };
        source.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(source.store.home_revision().is_err());
        let mut candidate = source.store.recover_same_home().unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        if fail_read {
            source.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        }
        let (facts, _) = Service::rebind_candidate(&mut candidate, facts, storage.clone(), &state)
            .err()
            .unwrap();
        assert_eq!(facts.selection(), selection);
        assert_eq!(facts.close_ticket(), close);
        if fail_read {
            let (facts, _) = Service::rebind_candidate(&mut candidate, facts, storage, &state)
                .err()
                .unwrap();
            assert_eq!(facts.selection(), selection);
            assert_eq!(facts.close_ticket(), close);
        }
        candidate.abort().close().unwrap();
    }
}
