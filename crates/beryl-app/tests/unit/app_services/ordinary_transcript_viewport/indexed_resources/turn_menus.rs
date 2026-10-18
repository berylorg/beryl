use super::code_long_line::{retire_code_fixture, wait_until, with_native};
use super::narrative_native::{
    drag, key, point_at, pointer, ready, record, require_fixture_ledger,
};
use super::*;
use crate::syndic_transcript::TranscriptNarrativeKind;
use gpui::{EntityInputHandler, Focusable, MouseButton, MouseDownEvent, PlatformInput};

const RETAINED_DRAFT: &str = "draft";

async fn nonempty_composer(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut AsyncApp,
) -> gpui::Entity<MainWindowConversationComposer> {
    let resident = composer(window, cx).await;
    with_native(window, cx, |native, app| {
        resident.read(app).gpui_input().update(app, |input, cx| {
            input.focus(native);
            input.replace_text_in_range(None, RETAINED_DRAFT, native, cx);
        });
    })
    .unwrap();
    wait_until(
        window,
        cx,
        |cx| {
            resident
                .read_with(cx, |resident, app| {
                    assert_eq!(resident.last_error(), None);
                    resident
                        .selection_identity()
                        .binding()
                        .logical_extent()
                        .logical_utf8_bytes()
                        == RETAINED_DRAFT.len() as u64
                        && resident.gpui_input().read(app).is_quiescent()
                        && resident
                            .gpui_input()
                            .read(app)
                            .is_surface_current_and_interactive()
                        && resident.test_edit_position_proof_ready(app)
                        && !resident.test_has_active_flight()
                })
                .unwrap()
        },
        "the real nonempty composer must settle before menu refusal assertions",
    )
    .await;
    resident
}

fn context_pointer(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    position: gpui::Point<gpui::Pixels>,
) {
    pointer(
        native,
        app,
        PlatformInput::MouseDown(MouseDownEvent {
            position,
            button: MouseButton::Right,
            click_count: 1,
            ..Default::default()
        }),
    );
}

fn assert_disabled_edit(panel: &crate::syndic_transcript::SyndicTranscriptPanel, quote: bool) {
    assert!(panel.quote_menu_open_for_test());
    let rows = panel.transcript_menu_rows_for_test();
    assert_eq!(rows.len(), if quote { 2 } else { 1 });
    if quote {
        assert_eq!(rows[0], ("Quote".into(), true, None));
    }
    assert_eq!(
        rows.last().unwrap(),
        &(
            "Edit message".into(),
            false,
            Some("Empty the composer before editing a historical message.".into()),
        )
    );
    assert!(panel.resident_context_menu_target().is_some());
}

#[test]
fn native_user_turn_menu_pointer_keyboard_disabled_edit_and_selection_closure() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| seed_history_text(path, 1, Some("turn **café 🌸** menu\r\n")),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["turn **café"]).await;
                let resident = nonempty_composer(window, cx).await;
                with_native(window, cx, |native, app| {
                    let (id, _, display) = record(panel.read(app), "turn **café");
                    assert_eq!(
                        panel.read(app).narrative_kind_for_test(&id),
                        Some(TranscriptNarrativeKind::UserInput)
                    );
                    let start = display.find("café 🌸").unwrap();
                    let position = point_at(panel.read(app), &id, start);
                    context_pointer(native, app, position);
                    assert_disabled_edit(panel.read(app), false);
                    key(native, app, "enter");
                    key(native, app, "space");
                    let edit = panel
                        .read(app)
                        .transcript_menu_row_position_for_test(0, native)
                        .unwrap();
                    pointer(
                        native,
                        app,
                        PlatformInput::MouseDown(MouseDownEvent {
                            position: edit,
                            button: MouseButton::Left,
                            click_count: 1,
                            ..Default::default()
                        }),
                    );
                    pointer(
                        native,
                        app,
                        PlatformInput::MouseUp(gpui::MouseUpEvent {
                            position: edit,
                            button: MouseButton::Left,
                            click_count: 1,
                            ..Default::default()
                        }),
                    );
                    assert!(
                        !panel.read(app).focus_handle(app).is_focused(native),
                        "Clicking the disabled menu row must retain popup focus rather than focus the transcript underneath",
                    );
                    panel.update(app, |panel, cx| {
                        panel.refuse_edit_menu_pointer_for_test(native, cx)
                    });
                    assert_disabled_edit(panel.read(app), false);
                    assert_eq!(
                        resident
                            .read(app)
                            .selection_identity()
                            .binding()
                            .logical_extent()
                            .logical_utf8_bytes(),
                        RETAINED_DRAFT.len() as u64
                    );
                    key(native, app, "escape");
                    assert!(!panel.read(app).quote_menu_open_for_test());
                    assert!(panel.read(app).resident_context_menu_target().is_none());

                    drag(native, app, position, position);
                    assert!(panel.read(app).focus_handle(app).is_focused(native));
                    key(native, app, "shift-f10");
                    assert_disabled_edit(panel.read(app), false);
                    key(native, app, "escape");

                    drag(
                        native,
                        app,
                        position,
                        point_at(panel.read(app), &id, start + "café 🌸".len()),
                    );
                    key(native, app, "shift-f10");
                    assert_disabled_edit(panel.read(app), true);
                    key(native, app, "end");
                    key(native, app, "enter");
                    key(native, app, "space");
                    assert_disabled_edit(panel.read(app), true);
                    assert_eq!(
                        resident
                            .read(app)
                            .selection_identity()
                            .binding()
                            .logical_extent()
                            .logical_utf8_bytes(),
                        RETAINED_DRAFT.len() as u64
                    );
                    key(native, app, "escape");
                    context_pointer(native, app, point_at(panel.read(app), &id, start + 1));
                    assert_disabled_edit(panel.read(app), true);
                    key(native, app, "escape");

                    context_pointer(
                        native,
                        app,
                        point_at(panel.read(app), &id, display.find("menu").unwrap()),
                    );
                    assert_disabled_edit(panel.read(app), false);
                    drag(native, app, position, position);
                    assert!(!panel.read(app).quote_menu_open_for_test());
                    assert!(panel.read(app).resident_context_menu_target().is_none());

                    resident
                        .read(app)
                        .gpui_input()
                        .update(app, |input, _| input.focus(native));
                    key(native, app, "shift-f10");
                    assert!(!panel.read(app).quote_menu_open_for_test());
                    assert_eq!(
                        resident
                            .read(app)
                            .selection_identity()
                            .binding()
                            .logical_extent()
                            .logical_utf8_bytes(),
                        RETAINED_DRAFT.len() as u64
                    );
                })
                .unwrap();
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_user_turn_menu_geometry_invalidation_and_retirement_close_exact_target() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| seed_history_text(path, 1, Some("retiring exact user menu\n")),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["retiring exact"]).await;
                let _resident = nonempty_composer(window, cx).await;
                with_native(window, cx, |native, app| {
                    let (id, _, _) = record(panel.read(app), "retiring exact");
                    context_pointer(native, app, point_at(panel.read(app), &id, 2));
                    assert_disabled_edit(panel.read(app), false);
                    panel.update(app, |panel, cx| {
                        panel.discard_staged_activation();
                        assert!(panel.resident_context_menu_target().is_none());
                        cx.notify();
                    });
                    native.draw_and_present_for_test(app);
                    assert!(!panel.read(app).quote_menu_open_for_test());
                })
                .unwrap();
                ready(window, &panel, cx, &["retiring exact"]).await;
                with_native(window, cx, |native, app| {
                    let (id, _, _) = record(panel.read(app), "retiring exact");
                    context_pointer(native, app, point_at(panel.read(app), &id, 2));
                    assert_disabled_edit(panel.read(app), false);
                })
                .unwrap();
                retire_code_fixture(window, &panel, cx).await;
                with_native(window, cx, |native, app| {
                    native.draw_and_present_for_test(app);
                    assert!(!panel.read(app).quote_menu_open_for_test());
                    assert!(panel.read(app).resident_context_menu_target().is_none());
                })
                .unwrap();
            })
        },
    );
}
