use beryl_app::main_window::{
    MainWindowConversationComposerCloseAdvance as Advance,
    MainWindowConversationComposerCloseTicket as Ticket,
};
use gpui::{EntityInputHandler, Modifiers, ScrollDelta, ScrollWheelEvent, point, px};

use super::support::{self, Mounted, drive, drive_until};

#[gpui::test]
fn close_preparing_drains_the_gpui_edit_admitted_in_the_same_update(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = support::mounted(cx, "mounted-admitted-edit", 191);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let original = fixture.service.selected_identity().unwrap();
    let text = "admitted before close";
    let close = cx
        .update(|window, app| {
            input.update(app, |input, cx| {
                input.focus(window);
                input.replace_and_mark_text_in_range(None, text, None, window, cx);
            });
            fixture
                .mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
        })
        .unwrap();
    assert_eq!(close.state, Advance::Preparing);
    let joined = cx
        .update(|window, app| {
            fixture
                .mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
        })
        .unwrap();
    assert_eq!(joined.ticket, close.ticket);
    ready(&fixture, close.ticket, cx);
    let saved = fixture.service.selected_identity().unwrap();
    assert_eq!(
        saved.binding().host_generation(),
        original.binding().host_generation()
    );
    assert_eq!(
        saved.binding().candidate().session_id(),
        original.binding().candidate().session_id()
    );
    assert_ne!(saved.binding().history(), original.binding().history());
    assert_eq!(
        saved.binding().logical_extent().logical_utf8_bytes(),
        text.len() as u64
    );
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        composer.entity_id()
    );
    assert_eq!(
        composer
            .read_with(cx, |composer, _| composer.gpui_input())
            .entity_id(),
        input.entity_id()
    );
    assert!(
        cx.update(|window, app| fixture
            .mount
            .update(app, |mount, cx| mount.release_window_close(
                close.ticket,
                window,
                cx
            )))
        .unwrap()
    );
    cx.simulate_keystrokes("ctrl-a");
    drive_until(cx, "select admitted edit for copy", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    cx.simulate_keystrokes("ctrl-c");
    for _ in 0..512 {
        drive(cx, 1);
        if cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref()
            == Some(text)
            || composer.read_with(cx, |composer, _| composer.last_error().is_some())
        {
            break;
        }
    }
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some(text),
        "copy after admitted edit: input={:?}, composer_error={:?}",
        input.read_with(cx, |input, _| (
            input.is_enabled(),
            input.is_quiescent(),
            input.surface().map(|surface| surface.selection())
        )),
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned))
    );
    drop((composer, input));
    support::finish(fixture, cx);
}

#[gpui::test]
fn pending_and_ready_close_preserve_resident_interaction_and_failure_releases_only_its_gate(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = support::mounted(cx, "mounted-readonly", 171);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let entity_id = composer.entity_id();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let input_id = input.entity_id();
    let text = (0..40)
        .map(|index| format!("line {index:02}: preserved resident close text\n"))
        .collect::<String>();
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.focus(window);
            input.replace_and_mark_text_in_range(None, &text, None, window, cx);
        })
    });
    drive_until(cx, "initial long-text edit", |cx| {
        assert!(
            composer.read_with(cx, |composer, _| composer.last_error().is_none()),
            "composer error: {:?}",
            composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned))
        );
        fixture
            .service
            .selected_identity()
            .is_some_and(|selection| {
                selection.binding().logical_extent().logical_utf8_bytes() == text.len() as u64
            })
            && input.read_with(cx, |input, _| input.is_quiescent())
    });
    navigate(&input, "ctrl-home shift-right shift-right", cx);
    let frame = cx.debug_bounds("composer-frame").unwrap();
    cx.simulate_event(ScrollWheelEvent {
        position: frame.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-32.))),
        modifiers: Modifiers::none(),
        ..Default::default()
    });
    drive(cx, 4);
    let initial_selection = fixture.service.selected_identity().unwrap();
    let before = input.read_with(cx, |input, _| {
        let surface = input.surface().unwrap();
        (
            surface.caret(),
            surface.selection(),
            surface.scroll_position(),
        )
    });
    let close = cx
        .update(|window, app| {
            fixture
                .mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
        })
        .unwrap();
    assert!(matches!(
        close.state,
        Advance::Preparing | Advance::Progress(_) | Advance::Ready
    ));
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        entity_id
    );
    assert_eq!(
        composer
            .read_with(cx, |composer, _| composer.gpui_input())
            .entity_id(),
        input_id
    );
    input.read_with(cx, |input, _| {
        let surface = input.surface().unwrap();
        assert_eq!(
            (
                surface.caret(),
                surface.selection(),
                surface.scroll_position()
            ),
            before
        );
    });
    assert!(input.read_with(cx, |input, _| input.is_enabled()));
    for _ in 0..8 {
        let joined = cx
            .update(|window, app| {
                fixture
                    .mount
                    .update(app, |mount, cx| mount.begin_window_close(window, cx))
            })
            .unwrap();
        assert_eq!(joined.ticket, close.ticket);
    }

    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_and_mark_text_in_range(None, "blocked", None, window, cx)
        })
    });
    cx.simulate_keystrokes("backspace shift-enter enter ctrl-z ctrl-y ctrl-x ctrl-v");
    drive(cx, 4);
    let after_rejected = fixture.service.selected_identity().unwrap();
    assert_eq!(
        after_rejected.binding().root(),
        initial_selection.binding().root()
    );
    support::assert_history_preserved(
        after_rejected.binding().history(),
        initial_selection.binding().history(),
    );
    assert!(!fixture.mount.read_with(cx, |mount, _| {
        mount.test_submission_diagnostics().active_ticket()
    }));

    cx.simulate_keystrokes("ctrl-a");
    drive_until(cx, "select text while close is pending", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    cx.simulate_keystrokes("ctrl-c");
    drive_until(cx, "copy while close is pending", |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref()
            == Some(text.as_str())
    });
    navigate(&input, "ctrl-home shift-right shift-right shift-right", cx);
    let selected = input.read_with(cx, |input, _| input.surface().unwrap().selection());
    assert_ne!(selected, before.1);
    let scroll_before = input.read_with(cx, |input, _| input.surface().unwrap().scroll_position());
    cx.simulate_event(ScrollWheelEvent {
        position: frame.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-48.))),
        modifiers: Modifiers::none(),
        ..Default::default()
    });
    drive(cx, 4);
    let interactive_state = input.read_with(cx, |input, _| {
        let surface = input.surface().unwrap();
        (
            surface.caret(),
            surface.selection(),
            surface.scroll_position(),
        )
    });
    assert_ne!(
        interactive_state.2, scroll_before,
        "readonly wheel input must scroll the resident viewport"
    );
    ready(&fixture, close.ticket, cx);
    let saved = fixture.service.selected_identity().unwrap();
    assert_eq!(
        saved.binding().candidate().session_id(),
        initial_selection.binding().candidate().session_id()
    );
    assert_eq!(saved.binding().root(), initial_selection.binding().root());
    support::assert_history_preserved(
        saved.binding().history(),
        initial_selection.binding().history(),
    );
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        entity_id
    );
    input.read_with(cx, |input, _| {
        let surface = input.surface().unwrap();
        assert_eq!(
            (
                surface.caret(),
                surface.selection(),
                surface.scroll_position()
            ),
            interactive_state
        );
    });
    cx.simulate_keystrokes("enter shift-enter backspace ctrl-v");
    drive(cx, 4);
    assert_eq!(fixture.service.selected_identity(), Some(saved));
    assert!(!fixture.mount.read_with(cx, |mount, _| {
        mount.test_submission_diagnostics().active_ticket()
    }));

    assert!(
        cx.update(|window, app| fixture
            .mount
            .update(app, |mount, cx| mount.release_window_close(
                close.ticket,
                window,
                cx
            )))
        .unwrap()
    );
    input.read_with(cx, |input, _| {
        let surface = input.surface().unwrap();
        assert_eq!(
            (
                surface.caret(),
                surface.selection(),
                surface.scroll_position()
            ),
            interactive_state
        );
    });
    cx.simulate_keystrokes("ctrl-z");
    drive_until(cx, "undo after close release", |_| {
        fixture
            .service
            .selected_identity()
            .is_some_and(|current| current.binding().logical_extent().logical_utf8_bytes() == 0)
    });
    cx.simulate_keystrokes("ctrl-y");
    drive_until(cx, "redo after close release", |_| {
        fixture
            .service
            .selected_identity()
            .is_some_and(|current| current.binding().root() == saved.binding().root())
    });
    let restored = fixture.service.selected_identity().unwrap();
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_and_mark_text_in_range(None, "accepted", None, window, cx)
        })
    });
    drive_until(cx, "edit after close release", |_| {
        fixture
            .service
            .selected_identity()
            .is_some_and(|current| current.binding().root() != restored.binding().root())
    });
    let edited = fixture.service.selected_identity().unwrap();
    let second = cx
        .update(|window, app| {
            fixture
                .mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
        })
        .unwrap();
    ready(&fixture, second.ticket, cx);
    input.update(cx, |input, cx| input.set_enabled(false, cx));
    assert!(
        cx.update(|window, app| fixture
            .mount
            .update(app, |mount, cx| mount.release_window_close(
                second.ticket,
                window,
                cx
            )))
        .unwrap()
    );
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_and_mark_text_in_range(None, "still blocked", None, window, cx)
        })
    });
    drive(cx, 4);
    assert_eq!(
        fixture
            .service
            .selected_identity()
            .unwrap()
            .binding()
            .root(),
        edited.binding().root()
    );
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        entity_id
    );
    drop((composer, input));
    support::finish(fixture, cx);
}

#[gpui::test]
fn exact_final_close_disposes_only_after_authorization_and_old_attempts_are_inert(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = support::mounted(cx, "mounted-final-close", 181);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let resident = fixture.service.selected_identity().unwrap();
    let first = cx
        .update(|window, app| {
            fixture
                .mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
        })
        .unwrap();
    ready(&fixture, first.ticket, cx);
    assert!(
        cx.update(|window, app| fixture
            .mount
            .update(app, |mount, cx| mount.release_window_close(
                first.ticket,
                window,
                cx
            )))
        .unwrap()
    );
    let second = cx
        .update(|window, app| {
            fixture
                .mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
        })
        .unwrap();
    assert_ne!(first.ticket, second.ticket);
    ready(&fixture, second.ticket, cx);
    assert!(
        !cx.update(|window, app| fixture.mount.update(app, |mount, cx| mount
            .release_window_close(first.ticket, window, cx)))
            .unwrap()
    );
    assert_eq!(
        cx.update(|window, app| fixture.mount.update(app, |mount, cx| mount
            .authorize_window_close_disposal(first.ticket, window, cx)))
            .unwrap(),
        Advance::Stale
    );
    assert_eq!(fixture.service.selected_identity(), Some(resident));
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        composer.entity_id()
    );
    let mut state = cx
        .update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.authorize_window_close_disposal(second.ticket, window, cx)
            })
        })
        .unwrap();
    for _ in 0..512 {
        if state == Advance::Disposed {
            break;
        }
        drive(cx, 1);
        state = cx
            .update(|window, app| {
                fixture.mount.update(app, |mount, cx| {
                    mount.advance_window_close(second.ticket, window, cx)
                })
            })
            .unwrap();
    }
    assert_eq!(state, Advance::Disposed);
    assert!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.contribution())
            .is_none()
    );
    assert!(fixture.service.selected_identity().is_none());
    assert!(input.read_with(cx, |input, _| input.surface().is_none()));
    assert!(
        !cx.update(|window, app| fixture.mount.update(app, |mount, cx| mount
            .release_window_close(second.ticket, window, cx)))
            .unwrap()
    );
    assert_eq!(
        cx.update(|window, app| fixture
            .mount
            .update(app, |mount, cx| mount.advance_window_close(
                first.ticket,
                window,
                cx
            )))
        .unwrap(),
        Advance::Stale
    );
    drop((composer, input));
    support::finish(fixture, cx);
}

pub(super) fn ready(fixture: &Mounted, ticket: Ticket, cx: &mut gpui::VisualTestContext) {
    for _ in 0..512 {
        drive(cx, 1);
        let state = cx
            .update(|window, app| {
                fixture.mount.update(app, |mount, cx| {
                    mount.advance_window_close(ticket, window, cx)
                })
            })
            .unwrap();
        match state {
            Advance::Ready => return,
            Advance::Preparing | Advance::Progress(_) | Advance::ReconciliationPending => {}
            other => panic!(
                "close did not retain a pending or ready editor: {other:?}; composer_error={:?}",
                fixture.mount.read_with(cx, |mount, app| mount
                    .contribution()
                    .and_then(|composer| composer.read(app).last_error().map(str::to_owned)))
            ),
        }
    }
    panic!("close draft did not become ready within 512 steps");
}

fn navigate(
    input: &gpui::Entity<gpui_text_input::RangeTextInput>,
    keys: &str,
    cx: &mut gpui::VisualTestContext,
) {
    for key in keys.split_whitespace() {
        cx.simulate_keystrokes(key);
        drive_until(cx, "navigation", |cx| {
            input.read_with(cx, |input, _| input.is_quiescent())
        });
    }
}
