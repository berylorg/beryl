use super::*;

pub(super) async fn editor_ready(
    window: WindowHandle<MainWindowShellRoot>,
    resident: &Entity<MainWindowConversationComposer>,
    bytes: u64,
    cx: &mut AsyncApp,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        with_native(window, cx, |native, app| {
            native.draw_and_present_for_test(app)
        })
        .unwrap();
        let state = resident
            .read_with(cx, |resident, app| {
                assert!(
                    resident.last_error().is_none(),
                    "composer error: {:?}",
                    resident.last_error()
                );
                assert!(
                    resident.last_mutation_admission_failure().is_none(),
                    "mounted composer mutation refused: {:?}; feedback={:?}",
                    resident.last_mutation_admission_failure(),
                    resident.mutation_feedback()
                );
                (
                    resident
                        .selection_identity()
                        .binding()
                        .logical_extent()
                        .logical_utf8_bytes(),
                    resident.gpui_input().read(app).is_quiescent(),
                    resident.test_edit_position_proof_ready(app),
                    resident.test_has_active_flight(),
                )
            })
            .unwrap();
        if state == (bytes, true, true, false) {
            return;
        }
        if Instant::now() >= deadline {
            let diagnostics = resident
                .read_with(cx, |resident, app| {
                    format!(
                        "{}; failure={:?}; feedback={:?}; terminal={:?}; storage={}",
                        resident.test_edit_position_proof_diagnostics(app),
                        resident.last_mutation_admission_failure(),
                        resident.mutation_feedback(),
                        resident.test_last_mutation_outcome(),
                        resident.test_mutation_storage_diagnostics()
                    )
                })
                .unwrap();
            panic!(
                "mounted composer expected {bytes} bytes and a settled caret, observed {state:?}; {diagnostics}"
            );
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

pub(super) async fn clear_editor(
    window: WindowHandle<MainWindowShellRoot>,
    resident: &Entity<MainWindowConversationComposer>,
    cx: &mut AsyncApp,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings)
        .unwrap();
    let bytes = resident
        .read_with(cx, |resident, _| {
            resident
                .selection_identity()
                .binding()
                .logical_extent()
                .logical_utf8_bytes()
        })
        .unwrap();
    window
        .update(cx, |_, native, app| {
            resident
                .read(app)
                .gpui_input()
                .update(app, |input, _| input.focus(native));
            native.dispatch_action(Box::new(gpui_text_input::SelectAll), app);
        })
        .unwrap();
    editor_ready(window, resident, bytes, cx).await;
    resident
        .read_with(cx, |resident, app| {
            let selection = resident
                .gpui_input()
                .read(app)
                .surface()
                .unwrap()
                .source_selection();
            assert_eq!(selection.anchor.byte_offset.get(), 0);
            assert_eq!(selection.head.byte_offset.get(), bytes);
        })
        .unwrap();
    with_native(window, cx, |native, app| key(native, app, "backspace")).unwrap();
    editor_ready(window, resident, 0, cx).await;
}

pub(super) async fn inspect_marker(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    let root = cx.update(|app| window.entity(app).unwrap()).unwrap();
    let marker = panel
        .read_with(cx, |panel, _| {
            panel
                .snapshot()
                .records
                .iter()
                .flat_map(|record| record.authored_parts())
                .find(|part| matches!(part.kind, ResidentPresentationRecordKind::ImageMarker(_)))
                .unwrap()
                .id
                .clone()
        })
        .unwrap();
    with_native(window, cx, |native, app| {
        let bounds = panel
            .read(app)
            .image_marker_bounds(&marker, native)
            .unwrap();
        assert!(
            panel
                .read(app)
                .viewport_bounds_for_test()
                .unwrap()
                .contains(&bounds.center())
        );
        click(native, app, bounds.center());
    })
    .unwrap();
    wait_until(
        window,
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| root.test_image_marker_inspection().is_some())
                .unwrap()
        },
        "real marker primary inspection must mount",
    )
    .await;
    let inspection = window
        .read_with(cx, |root, _| {
            root.test_image_marker_inspection().unwrap().clone()
        })
        .unwrap();
    assert_eq!(inspection.target.record.record_id, marker);
    with_native(window, cx, |native, app| key(native, app, "escape")).unwrap();
    with_native(window, cx, |native, app| {
        assert!(root.read(app).test_image_marker_inspection().is_none());
        assert!(inspection.focus.is_focused(native));
    })
    .unwrap();
}

pub(super) async fn edit(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) -> crate::syndic_transcript::ResidentHistoricalInputSource {
    let resident = composer(window, cx).await;
    edit_available(window, &resident, cx).await;
    with_native(window, cx, |native, app| {
        let (id, _, _) = record(panel.read(app), "before");
        let position = point_at(panel.read(app), &id, 2);
        pointer(
            native,
            app,
            PlatformInput::MouseDown(gpui::MouseDownEvent {
                position,
                button: gpui::MouseButton::Right,
                click_count: 1,
                ..Default::default()
            }),
        );
        let target = panel
            .read(app)
            .resident_context_menu_target()
            .unwrap()
            .record
            .historical_target()
            .unwrap();
        assert!(
            panel
                .read(app)
                .transcript_menu_rows_for_test()
                .iter()
                .any(|row| row.0 == "Edit message" && row.1)
        );
        key(native, app, "end");
        key(native, app, "enter");
        target
    })
    .unwrap()
}

pub(super) async fn edit_available(
    window: WindowHandle<MainWindowShellRoot>,
    resident: &Entity<MainWindowConversationComposer>,
    cx: &mut AsyncApp,
) {
    editor_ready(window, resident, 0, cx).await;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        with_native(window, cx, |native, app| {
            native.draw_and_present_for_test(app)
        })
        .unwrap();
        let unavailable = resident
            .read_with(cx, |resident, app| {
                resident.test_historical_edit_disabled_reason(app)
            })
            .unwrap();
        if unavailable.is_none() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "historical Edit must qualify the current empty composer selection: {unavailable:?}"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

pub(super) async fn replacement_ready(
    window: WindowHandle<MainWindowShellRoot>,
    resident: &Entity<MainWindowConversationComposer>,
    panel: &Entity<SyndicTranscriptPanel>,
    active: bool,
    cx: &mut AsyncApp,
) {
    wait_until(
        window,
        cx,
        |cx| {
            resident
                .read_with(cx, |resident, app| {
                    assert!(
                        resident.last_error().is_none(),
                        "replacement error: {:?}",
                        resident.last_error()
                    );
                    let state = resident.test_historical_replacement_state();
                    !state.0
                        && state.1.is_some() == active
                        && resident.gpui_input().read(app).is_quiescent()
                        && resident
                            .gpui_input()
                            .read(app)
                            .is_surface_current_and_interactive()
                        && panel.read(app).dimmed_record_ids_for_test().is_empty() != active
                })
                .unwrap()
        },
        "replacement mode and historical dimming must publish together",
    )
    .await;
}
