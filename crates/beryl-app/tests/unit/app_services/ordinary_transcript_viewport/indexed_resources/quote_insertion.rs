use super::code_long_line::{retire_code_fixture, with_native};
use super::narrative_native::{drag, key, point_at, ready, record, require_fixture_ledger, writer};
use super::*;
use gpui::{EntityInputHandler, Focusable};

#[test]
fn native_transcript_quote_inserts_exact_mapped_markdown_at_saved_position_and_undoes() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            seed_history_text(
                path,
                1,
                Some("prefix **café 🌸 words** suffix\r\nsecond line\r\n"),
            )
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["prefix"]).await;
                let resident = composer(window, cx).await;
                window
                    .update(cx, |_, native, app| {
                        resident.read(app).gpui_input().update(app, |input, cx| {
                            input.focus(native);
                            input.replace_and_mark_text_in_range(None, "AB", None, native, cx);
                        });
                    })
                    .unwrap();
                settled(window, &resident, 2, cx).await;
                eprintln!("Quote fixture initial edit settled");
                with_native(window, cx, |native, app| {
                    native.dispatch_action(Box::new(gpui_text_input::MoveLeft), app);
                    native.draw_and_present_for_test(app);
                })
                .unwrap();
                settled(window, &resident, 2, cx).await;
                let (copied, _) = writer(&panel, cx);
                eprintln!("Quote fixture saved caret settled");
                with_native(window, cx, |native, app| {
                    let (id, _, display) = record(panel.read(app), "prefix");
                    let start = display.find("café 🌸 words").unwrap();
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &id, start),
                        point_at(panel.read(app), &id, start + "café 🌸 words".len()),
                    );
                    assert!(panel.read(app).focus_handle(app).is_focused(native));
                    key(native, app, "shift-f10");
                    key(native, app, "enter");
                    assert!(panel.read(app).focus_handle(app).is_focused(native));
                })
                .unwrap();
                settled(window, &resident, 2 + "**café 🌸 words**".len() as u64, cx).await;
                eprintln!("Quote fixture formatted insertion settled");
                with_native(window, cx, |native, app| {
                    assert!(panel.read(app).focus_handle(app).is_focused(native));
                })
                .unwrap();
                assert!(copied.borrow().is_empty());
                assert_eq!(copy_all(window, &resident, cx).await, "A**café 🌸 words**B");
                eprintln!("Quote fixture copied edited draft");
                window
                    .update(cx, |_, native, app| {
                        native.dispatch_action(Box::new(gpui_text_input::Undo), app);
                    })
                    .unwrap();
                settled(window, &resident, 2, cx).await;
                eprintln!("Quote fixture undo settled");
                assert_eq!(copy_all(window, &resident, cx).await, "AB");
                settled(window, &resident, 2, cx).await;
                window
                    .update(cx, |_, native, app| {
                        native.dispatch_action(Box::new(gpui_text_input::Redo), app);
                    })
                    .unwrap();
                settled(window, &resident, 2 + "**café 🌸 words**".len() as u64, cx).await;
                assert_eq!(copy_all(window, &resident, cx).await, "A**café 🌸 words**B");
                window
                    .update(cx, |_, native, app| {
                        native.dispatch_action(Box::new(gpui_text_input::Undo), app);
                    })
                    .unwrap();
                settled(window, &resident, 2, cx).await;
                assert_eq!(copy_all(window, &resident, cx).await, "AB");
                settled(window, &resident, 2, cx).await;
                ready(window, &panel, cx, &["prefix"]).await;
                with_native(window, cx, |native, app| {
                    let (id, _, display) = record(panel.read(app), "prefix");
                    let start = display.find("fé 🌸").unwrap();
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &id, start + "fé 🌸".len()),
                        point_at(panel.read(app), &id, start),
                    );
                    let position = point_at(panel.read(app), &id, start + "f".len());
                    assert!(panel.read(app).narrative_geometry_current_for_test(native));
                    let before = panel.read(app).quote_pointer_facts_for_test(position, native);
                    super::narrative_native::pointer(
                        native,
                        app,
                        gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                            position,
                            button: gpui::MouseButton::Right,
                            click_count: 1,
                            ..Default::default()
                        }),
                    );
                    assert!(
                        panel.read(app).quote_menu_open_for_test(),
                        "Pointer Quote menu must open for selected text: before={before:?} after={:?}",
                        panel.read(app).quote_pointer_facts_for_test(position, native)
                    );
                    key(native, app, "enter");
                    assert!(panel.read(app).focus_handle(app).is_focused(native));
                })
                .unwrap();
                settled(window, &resident, 2 + "fé 🌸".len() as u64, cx).await;
                assert!(copied.borrow().is_empty());
                assert_eq!(copy_all(window, &resident, cx).await, "ABfé 🌸");
                settled(window, &resident, "ABfé 🌸".len() as u64, cx).await;
                ready(window, &panel, cx, &["prefix"]).await;
                with_native(window, cx, |native, app| {
                    let (id, _, display) = record(panel.read(app), "prefix");
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &id, 0),
                        point_at(
                            panel.read(app),
                            &id,
                            display.find("second").unwrap() + "second".len(),
                        ),
                    );
                    key(native, app, "shift-f10");
                    key(native, app, "enter");
                })
                .unwrap();
                let expected = "ABfé 🌸prefix **café 🌸 words** suffix\r\nsecond";
                settled(window, &resident, expected.len() as u64, cx).await;
                assert!(copied.borrow().is_empty());
                assert_eq!(copy_all(window, &resident, cx).await, expected);
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_transcript_quote_cancels_before_edit_admission_when_selection_geometry_retires() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| seed_history_text(path, 1, Some("prefix **quoted** suffix\n")),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["prefix"]).await;
                let resident = composer(window, cx).await;
                let gate = resident
                    .read_with(cx, |composer, _| {
                        composer.test_block_next_selected_dispatch()
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let (id, _, display) = record(panel.read(app), "prefix");
                    let start = display.find("quoted").unwrap();
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &id, start),
                        point_at(panel.read(app), &id, start + "quoted".len()),
                    );
                    key(native, app, "shift-f10");
                    key(native, app, "enter");
                })
                .unwrap();
                let deadline = Instant::now() + Duration::from_secs(15);
                while !gate.is_blocked() {
                    assert!(
                        Instant::now() < deadline,
                        "Quote staging did not reach its ordinary gate"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                with_native(window, cx, |native, app| key(native, app, "down")).unwrap();
                gate.release();
                settled(window, &resident, 0, cx).await;
                assert_eq!(copy_all(window, &resident, cx).await, "");
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

async fn settled(
    window: WindowHandle<MainWindowShellRoot>,
    resident: &gpui::Entity<MainWindowConversationComposer>,
    bytes: u64,
    cx: &mut AsyncApp,
) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        with_native(window, cx, |native, app| {
            native.draw_and_present_for_test(app);
        })
        .unwrap();
        let state = cx
            .update(|app| {
                let composer = resident.read(app);
                assert!(
                    composer.last_error().is_none(),
                    "Quote edit error: {:?}",
                    composer.last_error()
                );
                (
                    composer
                        .selection_identity()
                        .binding()
                        .logical_extent()
                        .logical_utf8_bytes(),
                    composer.gpui_input().read(app).is_quiescent(),
                    composer.test_edit_position_proof_ready(app),
                    composer.test_has_active_flight(),
                )
            })
            .unwrap();
        if state == (bytes, true, true, false) {
            return;
        }
        if Instant::now() >= deadline {
            let diagnostics = cx
                .update(|app| resident.read(app).test_edit_position_proof_diagnostics(app))
                .unwrap();
            panic!("Quote did not settle: {state:?}, expected {bytes}; {diagnostics}");
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}
