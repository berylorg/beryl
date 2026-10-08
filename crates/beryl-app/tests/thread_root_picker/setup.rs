use super::*;

#[gpui::test]
fn confirmation_uses_owner_eligibility_offscreen_and_fences_refresh_and_duplicate_acceptance(
    cx: &mut gpui::TestAppContext,
) {
    let (picker, cx) = mount(cx);
    admit(&picker, cx);
    let (events, _subscription) = collect(&picker, cx);
    let selected = PickerRowKey("row-1".into());
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            assert!(!picker.confirmation_state().unwrap().can_dispatch());
            picker.activate(&selected, cx);
            assert_eq!(picker.selected_key(), Some(&selected));
            assert!(!picker.confirmation_state().unwrap().can_dispatch());
            picker.set_selection_eligibility(&selected, None, cx);
            assert!(picker.confirmation_state().unwrap().can_dispatch());
            picker.focus_row(1, window, cx);
            picker.navigate(PickerNavigation::End, window, cx);
        })
    });
    admit(&picker, cx);
    assert!(picker.read_with(cx, |picker, _| {
        picker.confirmation_state().unwrap().can_dispatch()
    }));
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.focused_key().cloned()),
        Some(PickerRowKey("row-99999".into()))
    );
    let confirm = PickerCommand::Confirm(selected.clone());
    cx.update(|_, app| {
        picker.update(app, |picker, cx| {
            picker.dispatch_command(confirm.clone(), cx);
            picker.dispatch_command(confirm.clone(), cx);
            assert!(picker.confirmation_state().unwrap().pending);
            picker.finish_command(&confirm, cx);
        })
    });
    draw(cx);
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|event| **event == PickerEvent::Command(confirm.clone()))
            .count(),
        1
    );
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.replace_collection(
                PickerCollectionKey("roots-all".into()),
                2,
                100_000,
                None,
                window,
                cx,
            );
            assert!(!picker.confirmation_state().unwrap().can_dispatch());
        })
    });
    admit(&picker, cx);
    cx.update(|_, app| {
        picker.update(app, |picker, cx| {
            picker.set_selection_eligibility(
                &selected,
                Some("The root is unavailable.".into()),
                cx,
            );
            assert!(!picker.confirmation_state().unwrap().can_dispatch());
            picker.dispatch_command(confirm.clone(), cx);
        })
    });
    draw(cx);
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|event| **event == PickerEvent::Command(confirm.clone()))
            .count(),
        1
    );
}

#[gpui::test]
fn visited_collection_restores_query_stable_focus_and_scroll_and_scope_clear_preserves_pending_selection(
    cx: &mut gpui::TestAppContext,
) {
    let (picker, cx) = mount(cx);
    admit(&picker, cx);
    let input = picker.read_with(cx, |picker, _| picker.search_input());
    cx.update(|window, app| picker.update(app, |picker, cx| picker.focus_search(window, cx)));
    cx.simulate_input("retained search");
    admit(&picker, cx);
    let selected = PickerRowKey("row-2".into());
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.activate(&selected, cx);
            picker.set_selection_eligibility(&selected, None, cx);
            picker.focus_row(8, window, cx);
        })
    });
    draw(cx);
    let before = picker.read_with(cx, |picker, _| picker.diagnostics());
    assert!(before.scroll_offset > 0.);
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.replace_collection(
                PickerCollectionKey("roots-runtime".into()),
                50,
                100_000,
                None,
                window,
                cx,
            );
        })
    });
    admit(&picker, cx);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.query_text().to_owned()),
        ""
    );
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.replace_collection(
                PickerCollectionKey("roots-all".into()),
                51,
                100_000,
                None,
                window,
                cx,
            );
        })
    });
    admit(&picker, cx);
    let restored = picker.read_with(cx, |picker, _| picker.diagnostics());
    assert_eq!(restored.focused_key, before.focused_key);
    assert_eq!(restored.scroll_offset, before.scroll_offset);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.query_text().to_owned()),
        "retained search"
    );
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.selected_key().cloned()),
        Some(selected.clone())
    );
    cx.update(|_, app| picker.update(app, |picker, cx| picker.clear_search(cx)));
    draw(cx);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.query_text().to_owned()),
        ""
    );
    assert_eq!(input.read_with(cx, |input, _| input.text().to_owned()), "");
    cx.update(|_, app| picker.update(app, |picker, cx| picker.set_selected_key(None, cx)));
    assert!(!picker.read_with(cx, |picker, _| {
        picker.confirmation_state().unwrap().can_dispatch()
    }));
}

#[gpui::test]
fn native_dialog_preserves_picker_and_pending_command_and_cancellation_returns_exact_focus(
    cx: &mut gpui::TestAppContext,
) {
    let (picker, cx) = mount(cx);
    admit(&picker, cx);
    let (events, _subscription) = collect(&picker, cx);
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.configure_runtime_section(
                PickerRuntimeSectionConfig {
                    heading: "RUNTIMES & ROOTS".into(),
                    empty_text: "No runtimes".into(),
                    add_runtime: PickerCommandState::enabled("Add runtime"),
                },
                PickerCollectionKey("runtime-registry".into()),
                1,
                0,
                window,
                cx,
            );
        })
    });
    draw(cx);
    let before = picker.read_with(cx, |picker, _| picker.diagnostics());
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.restore_command_focus(&PickerCommand::AddRuntime, window, cx);
            picker.dispatch_command(PickerCommand::AddRuntime, cx);
            picker.set_native_dialog_open(true, cx);
            assert!(
                picker
                    .command_state(&PickerCommand::AddRuntime)
                    .unwrap()
                    .pending
            );
            picker.dispatch_command(PickerCommand::AddRuntime, cx);
            picker.activate(&PickerRowKey("row-1".into()), cx);
            picker.dismiss(window, cx);
        })
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|event| **event == PickerEvent::Command(PickerCommand::AddRuntime))
            .count(),
        1
    );
    assert!(!events.borrow().contains(&PickerEvent::Dismiss));
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.diagnostics().focused_key),
        before.focused_key
    );
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.set_native_dialog_open(false, cx);
            picker.finish_command(&PickerCommand::AddRuntime, cx);
            picker.restore_command_focus(&PickerCommand::AddRuntime, window, cx);
            assert!(
                picker
                    .command_focus_handle(&PickerCommand::AddRuntime)
                    .unwrap()
                    .is_focused(window)
            );
            assert!(
                picker
                    .command_state(&PickerCommand::AddRuntime)
                    .unwrap()
                    .can_dispatch()
            );
        })
    });
}

#[gpui::test]
fn both_large_viewports_remain_bounded_with_fixed_frame_and_visible_retry_feedback(
    cx: &mut gpui::TestAppContext,
) {
    let (picker, cx) = mount(cx);
    admit(&picker, cx);
    let frame = cx.debug_bounds("thread-root-picker").unwrap();
    let (events, _subscription) = collect(&picker, cx);
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.configure_runtime_section(
                PickerRuntimeSectionConfig {
                    heading: "RUNTIMES & ROOTS".into(),
                    empty_text: "No runtimes".into(),
                    add_runtime: PickerCommandState::enabled("Add runtime"),
                },
                PickerCollectionKey("runtime-registry".into()),
                1,
                100_000,
                window,
                cx,
            );
            picker.set_retry_commands(
                Some(PickerCommandState::enabled("Retry")),
                Some(PickerCommandState::enabled("Retry")),
                cx,
            );
        })
    });
    draw(cx);
    let runtime_request = events
        .borrow()
        .iter()
        .find_map(|event| match event {
            PickerEvent::RequestRuntimePage(request) => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.settle_runtime_page(
                PickerRuntimePageOutcome::Success(PickerRuntimePage {
                    request: runtime_request,
                    total_count: 100_000,
                    rows: (0..PICKER_PAGE_ROWS)
                        .map(|position| PickerRuntimeRow {
                            row: row(position),
                            browse_roots: PickerCommandState::enabled("Browse roots"),
                            add_root: PickerCommandState::enabled("Add root"),
                            active_scope: false,
                        })
                        .collect(),
                }),
                window,
                cx,
            );
            picker.focus_row(2, window, cx);
            picker.navigate(PickerNavigation::End, window, cx);
        })
    });
    let request = picker.read_with(cx, |picker, _| {
        picker
            .pending_requests()
            .iter()
            .find(|request| request.range.contains(&99_999))
            .unwrap()
            .clone()
    });
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.settle_page(
                PickerPageOutcome::Failed {
                    request,
                    message: "The page could not load.".into(),
                },
                window,
                cx,
            );
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("thread-root-picker-retry-collection")
            .is_some()
    );
    cx.update(|_, app| {
        picker.update(app, |picker, cx| {
            picker.dispatch_command(PickerCommand::RetryCollection, cx);
            picker.dispatch_command(PickerCommand::RetryCollection, cx);
            assert!(
                picker
                    .command_state(&PickerCommand::RetryCollection)
                    .unwrap()
                    .pending
            );
            assert_eq!(picker.pending_requests().len(), 1);
        })
    });
    draw(cx);
    let primary = picker.read_with(cx, |picker, _| picker.diagnostics());
    let runtimes = picker.read_with(cx, |picker, _| picker.runtime_diagnostics().unwrap());
    assert_eq!(primary.total_count, 100_000);
    assert_eq!(runtimes.total_count, 100_000);
    assert!(primary.realized_row_count <= PICKER_MAX_REALIZED_ROWS);
    assert!(runtimes.realized_row_count <= PICKER_MAX_REALIZED_ROWS);
    assert!(primary.resident_row_count <= PICKER_MAX_RESIDENT_PAGES * PICKER_PAGE_ROWS);
    assert!(runtimes.resident_row_count <= PICKER_MAX_RESIDENT_PAGES * PICKER_PAGE_ROWS);
    assert_eq!(
        cx.debug_bounds("thread-root-picker").unwrap().size,
        frame.size
    );
    assert!(cx.debug_bounds("thread-root-picker-confirm").is_some());
    assert!(cx.debug_bounds("thread-root-picker-add-runtime").is_some());
}

#[gpui::test]
fn owner_unavailable_confirmation_stays_disabled_after_valid_selection(
    cx: &mut gpui::TestAppContext,
) {
    let (picker, cx) = mount(cx);
    admit(&picker, cx);
    let (events, _subscription) = collect(&picker, cx);
    cx.update(|_, app| {
        picker.update(app, |picker, cx| {
            picker.configure_selection(
                PickerSelectionMode::Confirmed {
                    confirm: PickerCommandState::unavailable(
                        "Confirm",
                        "Root confirmation is unavailable.",
                    ),
                },
                cx,
            );
            let key = PickerRowKey("row-1".into());
            picker.activate(&key, cx);
            picker.set_selection_eligibility(&key, None, cx);
            assert!(!picker.confirmation_state().unwrap().can_dispatch());
            picker.dispatch_command(PickerCommand::Confirm(key), cx);
        })
    });
    draw(cx);
    assert!(
        !events
            .borrow()
            .iter()
            .any(|event| matches!(event, PickerEvent::Command(PickerCommand::Confirm(_))))
    );
}
