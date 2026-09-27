use super::widget_support::fixture::{Fixture, operation_id};
use beryl_app::{
    composer_host::{
        ComposerHostFlushAdmission, ComposerHostFlushCapture, ComposerHostFlushPurpose,
        ComposerHostFlushState, ComposerHostFlushTicket,
    },
    main_window::{
        MainWindowComposerMarkerMetadataAuthority, MainWindowComposerSlot,
        MainWindowConversationComposerCloseTicket,
    },
};
use beryl_home_store::{CommandCancellation, HomeHealthState, test_faults::FaultPoint};
use gpui::{AppContext, TestAppContext};
use syndic_storage::SyndicTimestamp;

fn slot(fixture: &Fixture) -> Box<MainWindowComposerSlot> {
    let claim = fixture.claims().0;
    Box::new(
        MainWindowComposerSlot::new(
            fixture.window_id,
            claim,
            fixture.activated_host(fixture.selected_thread, 221, 222, 1),
            fixture.storage.clone(),
            MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
        )
        .unwrap(),
    )
}

fn ready(fixture: &Fixture, slot: &mut MainWindowComposerSlot, flush: ComposerHostFlushTicket) {
    let selection = slot.selected_identity().unwrap();
    assert_eq!(
        slot.capture_selected_flush_publication(
            &fixture.store,
            selection,
            flush,
            fixture.assets(),
            &fixture.marker_seals(),
            operation_id(223),
            None,
            SyndicTimestamp::from_unix_millis(5),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::CloseReady)
    );
}

#[gpui::test]
fn clean_slot_retirement_preserves_exact_evidence_after_home_failure(cx: &mut TestAppContext) {
    let fixture = Fixture::new("clean-slot-retirement", 201);
    let mut slot = slot(&fixture);
    let selection = slot.selected_identity().unwrap();
    let owner = cx.new(|_| ()).entity_id();
    let close = MainWindowConversationComposerCloseTicket::for_test(owner, 1, selection);
    slot.test_begin_window_close_gate(close).unwrap();
    let ComposerHostFlushAdmission::Started { ticket: flush, .. } = slot
        .begin_selected_flush(selection, ComposerHostFlushPurpose::WindowClose)
        .unwrap()
    else {
        panic!("expected new close");
    };
    ready(&fixture, &mut slot, flush);
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    assert_eq!(fixture.store.health().state(), HomeHealthState::Failed);
    let evidence = slot.retire_clean_window_close(close, flush).ok().unwrap();
    assert_eq!(evidence.selection(), selection);
    assert_eq!(evidence.close_ticket(), close);
    assert_eq!(evidence.host().binding(), selection.binding());
    assert_eq!(evidence.host().thread_id(), fixture.selected_thread);
    assert_eq!(evidence.host().close_ticket(), flush);
    assert_eq!(fixture.store.health().state(), HomeHealthState::Failed);
}

#[gpui::test]
fn refused_slot_retirement_preserves_selection_gate_and_host(cx: &mut TestAppContext) {
    let fixture = Fixture::new("retained-slot-retirement", 211);
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
    slot = slot.retire_clean_window_close(close, flush).err().unwrap();
    assert_eq!(slot.selected_identity(), Some(selection));
    ready(&fixture, &mut slot, flush);
    slot = slot.retire_clean_window_close(stale, flush).err().unwrap();
    assert_eq!(slot.selected_identity(), Some(selection));
    let evidence = slot.retire_clean_window_close(close, flush).ok().unwrap();
    assert_eq!(evidence.selection(), selection);
}
