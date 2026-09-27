use super::support::{self, drive, drive_until};
use beryl_app::main_window::{
    MainWindowConversationComposerCloseAdvance as Advance,
    MainWindowConversationComposerCloseTicket as Ticket,
};
use gpui::{EntityInputHandler, TestAppContext};

#[gpui::test]
fn recovery_fence_preserves_the_exact_resident_and_refuses_ordinary_release(
    cx: &mut TestAppContext,
) {
    let (fixture, cx) = support::mounted(cx, "resident-recovery-fence", 249);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let close = cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.focus(window);
            input.replace_and_mark_text_in_range(
                None,
                "preserved recovery history",
                None,
                window,
                cx,
            );
        });
        let close = fixture
            .mount
            .update(app, |mount, cx| mount.begin_window_close(window, cx))
            .unwrap();
        assert!(
            !fixture
                .mount
                .update(app, |mount, cx| mount
                    .fence_interrupted_exit_resident(close.ticket, cx))
                .unwrap()
        );
        close
    });
    drive_until(cx, "close becomes ready for recovery", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == Advance::Ready
    });
    cx.simulate_keystrokes("ctrl-home shift-right");
    drive_until(cx, "resident presentation settles", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    let selection = composer.read_with(cx, |composer, _| composer.selection_identity());
    let flush = fixture
        .mount
        .read_with(cx, |mount, _| mount.window_close_flush_ticket(close.ticket))
        .unwrap();
    let before = input.read_with(cx, |input, _| {
        (
            input.surface().unwrap().selection(),
            input.surface().unwrap().scroll_block(),
            input.history_frontier(),
        )
    });
    let restoration = input
        .update(cx, |input, _| {
            input.export_restoration(Some(selection.binding().range_history_frontier()))
        })
        .unwrap();
    let stale = Ticket::for_test(fixture.mount.entity_id(), 99, selection);
    fixture.mount.update(cx, |mount, cx| {
        assert!(mount.fence_interrupted_exit_resident(stale, cx).is_err());
        assert!(
            mount
                .fence_interrupted_exit_resident(close.ticket, cx)
                .is_err()
        );
    });
    assert!(composer.read_with(cx, |composer, _| composer.recovery_snapshot().is_none()));
    composer
        .update(cx, |composer, cx| {
            composer.test_set_shutdown_interaction_gated(true, cx)
        })
        .unwrap();
    fixture.service.test_with_close_slot_locked(|| {
        fixture.mount.update(cx, |mount, cx| {
            assert!(
                mount
                    .fence_interrupted_exit_resident(close.ticket, cx)
                    .unwrap()
            );
            assert!(
                mount
                    .fence_interrupted_exit_resident(close.ticket, cx)
                    .unwrap()
            );
        });
    });
    composer.read_with(cx, |composer, _| {
        let snapshot = composer.recovery_snapshot().unwrap();
        assert_eq!(snapshot.selection(), selection);
        assert_eq!(snapshot.close_ticket(), close.ticket);
        assert_eq!(snapshot.flush_ticket(), flush);
        assert_eq!(snapshot.restoration(), &restoration);
        assert_eq!(composer.gpui_input().entity_id(), input.entity_id());
        assert!(!composer.test_widget_released());
    });
    cx.update(|window, app| {
        fixture.mount.update(app, |mount, cx| {
            assert!(mount.begin_window_close(window, cx).is_err());
            assert!(
                mount
                    .release_window_close(close.ticket, window, cx)
                    .is_err()
            );
            assert!(
                mount
                    .authorize_window_close_disposal(close.ticket, window, cx)
                    .is_err()
            );
            assert!(
                mount
                    .advance_window_close(close.ticket, window, cx)
                    .is_err()
            );
        });
        composer.update(app, |composer, cx| {
            assert!(
                composer
                    .test_set_shutdown_interaction_gated(false, cx)
                    .is_err()
            );
            assert!(composer.begin_widget_release_fence(window, cx).is_err());
            assert!(
                composer
                    .resume_after_widget_release_fence(window, cx)
                    .is_err()
            );
            assert!(composer.release_widget(window, cx).is_err());
        });
    });
    cx.simulate_keystrokes("backspace ctrl-z ctrl-v ctrl-a");
    drive(cx, 12);
    assert_eq!(fixture.service.selected_identity(), Some(selection));
    assert!(fixture.service.test_window_close_is_current(close.ticket));
    assert_eq!(
        input.read_with(cx, |input, _| (
            input.surface().unwrap().selection(),
            input.surface().unwrap().scroll_block(),
            input.history_frontier(),
        )),
        before
    );
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.contribution().unwrap().entity_id()),
        composer.entity_id()
    );
    drop((composer, input));
    support::finish(fixture, cx);
}
