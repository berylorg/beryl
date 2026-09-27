use super::super::widget_support::fixture::{Fixture, operation_id};
use beryl_app::{
    composer_host::{ComposerHostFlushCapture, ComposerHostFlushState, ComposerHostFlushTicket},
    main_window::{MainWindowComposerMarkerMetadataAuthority, MainWindowComposerSlot},
};
use beryl_home_store::CommandCancellation;
use syndic_storage::SyndicTimestamp;
pub fn slot(fixture: &Fixture) -> Box<MainWindowComposerSlot> {
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

pub fn ready(fixture: &Fixture, slot: &mut MainWindowComposerSlot, flush: ComposerHostFlushTicket) {
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

pub fn retired(
    fixture: &Fixture,
    owner: gpui::EntityId,
) -> beryl_app::main_window::MainWindowComposerRetiredClose {
    use beryl_app::{
        composer_host::{ComposerHostFlushAdmission, ComposerHostFlushPurpose},
        main_window::MainWindowConversationComposerCloseTicket,
    };
    let mut slot = slot(fixture);
    let selection = slot.selected_identity().unwrap();
    let close = MainWindowConversationComposerCloseTicket::for_test(owner, 1, selection);
    slot.test_begin_window_close_gate(close).unwrap();
    let ComposerHostFlushAdmission::Started { ticket: flush, .. } = slot
        .begin_selected_flush(selection, ComposerHostFlushPurpose::WindowClose)
        .unwrap()
    else {
        panic!("expected new close");
    };
    ready(fixture, &mut slot, flush);
    slot.retire_clean_window_close(close, flush).ok().unwrap()
}
