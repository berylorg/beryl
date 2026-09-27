use super::support::slot_close::{ready, slot};
use super::widget_support::fixture::Fixture;
use beryl_app::{
    composer_host::{ComposerHostFlushAdmission, ComposerHostFlushPurpose},
    main_window::{
        MainWindowConversationComposerCloseTicket, MainWindowConversationComposerService,
    },
};
use beryl_home_store::{HomeHealthState, test_faults::FaultPoint};
use gpui::{AppContext, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn service_retirement_waits_for_references_and_preserves_failed_home_evidence(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new("clean-service-retirement", 231);
    let mut slot = slot(&fixture);
    let selection = slot.selected_identity().unwrap();
    let close = MainWindowConversationComposerCloseTicket::for_test(
        cx.new(|_| ()).entity_id(),
        1,
        selection,
    );
    slot.test_begin_window_close_gate(close).unwrap();
    let ComposerHostFlushAdmission::Started { ticket: flush, .. } = slot
        .begin_selected_flush(selection, ComposerHostFlushPurpose::WindowClose)
        .unwrap()
    else {
        panic!("expected new close");
    };
    ready(&fixture, &mut slot, flush);
    let mut service = Arc::new(MainWindowConversationComposerService::new(
        fixture.store.service_reference(),
        *slot,
    ));
    service.test_begin_window_close_gate(close).unwrap();
    let worker = service.clone();
    service = service
        .retire_clean_window_close(close, flush)
        .err()
        .unwrap();
    assert!(Arc::ptr_eq(&service, &worker));
    drop(worker);
    let home = service.test_retain_home_reference();
    service = service
        .retire_clean_window_close(close, flush)
        .err()
        .unwrap();
    assert_eq!(service.selected_identity(), Some(selection));
    drop(home);
    let weak = Arc::downgrade(&service);
    service = service
        .retire_clean_window_close(close, flush)
        .err()
        .unwrap();
    assert!(weak.upgrade().is_some());
    drop(weak);
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    assert_eq!(fixture.store.health().state(), HomeHealthState::Failed);
    let evidence = service
        .retire_clean_window_close(close, flush)
        .ok()
        .unwrap();
    assert_eq!(evidence.selection(), selection);
    assert_eq!(evidence.close_ticket(), close);
    assert_eq!(evidence.host().close_ticket(), flush);
}

#[gpui::test]
fn service_retirement_refuses_missing_stale_and_unready_close_without_losing_custody(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new("refused-service-retirement", 241);
    let mut slot = slot(&fixture);
    let selection = slot.selected_identity().unwrap();
    let owner = cx.new(|_| ()).entity_id();
    let close = MainWindowConversationComposerCloseTicket::for_test(owner, 1, selection);
    let stale = MainWindowConversationComposerCloseTicket::for_test(owner, 2, selection);
    slot.test_begin_window_close_gate(close).unwrap();
    let ComposerHostFlushAdmission::Started { ticket: flush, .. } = slot
        .begin_selected_flush(selection, ComposerHostFlushPurpose::WindowClose)
        .unwrap()
    else {
        panic!("expected new close");
    };
    let mut service = Arc::new(MainWindowConversationComposerService::new(
        fixture.store.service_reference(),
        *slot,
    ));
    service = service
        .retire_clean_window_close(close, flush)
        .err()
        .unwrap();
    service.test_begin_window_close_gate(close).unwrap();
    service = service
        .retire_clean_window_close(stale, flush)
        .err()
        .unwrap();
    assert!(service.test_window_close_is_current(close));
    service = service
        .retire_clean_window_close(close, flush)
        .err()
        .unwrap();
    assert_eq!(service.selected_identity(), Some(selection));
    assert!(service.test_window_close_is_current(close));
}
