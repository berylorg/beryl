use super::support::{self, drive, drive_until};
use beryl_app::main_window::{
    MainWindowConversationComposerCloseAdvance as Advance,
    MainWindowConversationComposerCloseTicket as Ticket,
};
use gpui::TestAppContext;
use std::{cell::Cell, rc::Rc};

#[gpui::test]
fn recovery_transfers_clipboard_capture_once_and_preserves_the_editor(cx: &mut TestAppContext) {
    let capture = Rc::new(Cell::new(0));
    let captured = capture.clone();
    let (fixture, cx) = support::mounted(cx, "resident-clipboard-detachment", 182);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    composer.update(cx, |composer, _| {
        composer.test_set_clipboard_writer(Box::new(move |_, _| {
            captured.set(captured.get() + 1);
            gpui_text_input::ClipboardWriteOutcome::Written
        }));
    });
    assert_eq!(Rc::strong_count(&capture), 2);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let close = cx.update(|window, app| {
        fixture
            .mount
            .update(app, |mount, cx| mount.begin_window_close(window, cx))
            .unwrap()
    });
    assert!(
        composer
            .update(cx, |composer, cx| composer
                .detach_recovery_clipboard_writer(close.ticket, cx))
            .is_err()
    );
    drive_until(cx, "writer close becomes ready", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == Advance::Ready
    });
    drive_until(cx, "writer editor settles", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    assert!(
        composer
            .update(cx, |composer, cx| composer
                .detach_recovery_clipboard_writer(close.ticket, cx))
            .is_err()
    );
    assert_eq!(Rc::strong_count(&capture), 2);
    composer
        .update(cx, |composer, cx| {
            composer.test_set_shutdown_interaction_gated(true, cx)
        })
        .unwrap();
    assert!(
        fixture
            .mount
            .update(cx, |mount, cx| mount
                .fence_interrupted_exit_resident(close.ticket, cx))
            .unwrap()
    );
    let selection = composer.read_with(cx, |composer, _| composer.selection_identity());
    let stale = Ticket::for_test(fixture.mount.entity_id(), 99, selection);
    let before = composer.read_with(cx, |composer, _| {
        composer.recovery_snapshot().unwrap().restoration().clone()
    });
    let writer = fixture.service.test_with_close_slot_locked(|| {
        composer.update(cx, |resident, cx| {
            assert!(
                resident
                    .detach_recovery_clipboard_writer(stale, cx)
                    .is_err()
            );
            let writer = resident
                .detach_recovery_clipboard_writer(close.ticket, cx)
                .unwrap()
                .unwrap();
            assert!(
                resident
                    .detach_recovery_clipboard_writer(close.ticket, cx)
                    .unwrap()
                    .is_none()
            );
            assert!(
                resident
                    .detach_recovery_clipboard_writer(stale, cx)
                    .is_err()
            );
            assert_eq!(resident.gpui_input().entity_id(), input.entity_id());
            writer
        })
    });
    assert_eq!(Rc::strong_count(&capture), 2);
    assert_eq!(capture.get(), 0);
    drop(writer);
    assert_eq!(Rc::strong_count(&capture), 1);
    cx.simulate_keystrokes("x ctrl-z ctrl-v");
    drive(cx, 12);
    assert_eq!(capture.get(), 0);
    composer.read_with(cx, |composer, _| {
        assert_eq!(composer.recovery_snapshot().unwrap().restoration(), &before);
        assert_eq!(composer.gpui_input().entity_id(), input.entity_id());
        assert!(!composer.test_widget_released());
    });
    assert_eq!(
        input
            .update(cx, |input, _| input.export_restoration(Some(
                selection.binding().range_history_frontier()
            )))
            .unwrap(),
        before
    );
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    assert!(fixture.service.test_window_close_is_current(close.ticket));
    drop((input, composer));
    support::finish(fixture, cx);
}
