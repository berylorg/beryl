use super::*;

#[gpui::test]
fn fixed_frame_allocates_independent_runtime_viewport_and_remaining_primary_height(
    cx: &mut gpui::TestAppContext,
) {
    let (picker, cx) = mount(cx);
    admit(&picker, cx);
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
    let frame = cx.debug_bounds("thread-root-picker").unwrap();
    assert_eq!(f32::from(frame.size.height), 616.);
    assert_eq!(
        f32::from(
            cx.debug_bounds("thread-root-picker-collection")
                .unwrap()
                .size
                .height
        ),
        168.
    );
    assert_eq!(
        f32::from(
            cx.debug_bounds("thread-root-picker-runtime-collection")
                .unwrap()
                .size
                .height
        ),
        102.
    );
    cx.update(|_, app| {
        picker.update(app, |picker, cx| {
            picker.configure_selection(PickerSelectionMode::Immediate, cx)
        })
    });
    draw(cx);
    assert_eq!(
        cx.debug_bounds("thread-root-picker").unwrap().size,
        frame.size
    );
    assert_eq!(
        f32::from(
            cx.debug_bounds("thread-root-picker-collection")
                .unwrap()
                .size
                .height
        ),
        240.
    );
}

#[gpui::test]
fn pending_admission_preserves_exact_command_and_fences_scope_selection_and_search(
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
                100,
                window,
                cx,
            );
        })
    });
    draw(cx);
    let request = events
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
                    request,
                    total_count: 100,
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
        })
    });
    draw(cx);
    let selected = PickerRowKey("row-2".into());
    let command = PickerCommand::AddRoot(PickerRowKey("row-0".into()));
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.activate(&selected, cx);
            picker.focus_row(2, window, cx);
            picker.focus_search(window, cx);
            picker.dispatch_command(command.clone(), cx);
            let before = picker.runtime_diagnostics().unwrap();
            picker.update_command_state(
                &command,
                PickerCommandState {
                    label: "Reconciling".into(),
                    unavailable_reason: None,
                    pending: true,
                },
                cx,
            );
            assert_eq!(
                picker.runtime_diagnostics().unwrap().resident_row_count,
                before.resident_row_count
            );
            picker.dispatch_command(command.clone(), cx);
            picker.dispatch_command(PickerCommand::BrowseRoots(PickerRowKey("row-1".into())), cx);
            picker.dispatch_command(PickerCommand::AddRuntime, cx);
            picker.set_selected_key(None, cx);
            picker.activate(&PickerRowKey("row-3".into()), cx);
            picker.focus_row(3, window, cx);
            assert_eq!(picker.focused_key(), Some(&selected));
            assert_eq!(picker.selected_key(), Some(&selected));
            assert_eq!(picker.command_state(&command).unwrap().label, "Reconciling");
            assert!(picker.command_state(&command).unwrap().pending);
        })
    });
    let input = picker.read_with(cx, |picker, _| picker.search_input());
    cx.simulate_input("rejected query");
    draw(cx);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.query_text().to_owned()),
        ""
    );
    assert_eq!(input.read_with(cx, |input, _| input.text().to_owned()), "");
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|event| matches!(event, PickerEvent::Command(_)))
            .count(),
        1
    );
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.update_command_state(
                &command,
                PickerCommandState::unavailable("Unavailable", "The outcome cannot be proven."),
                cx,
            );
            picker.finish_command(&command, cx);
            picker.restore_command_focus(&command, window, cx);
            assert!(!picker.command_state(&command).unwrap().can_dispatch());
            assert!(
                picker
                    .command_focus_handle(&command)
                    .unwrap()
                    .is_focused(window)
            );
            assert_eq!(picker.selected_key(), Some(&selected));
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("thread-root-picker-add-root-row-0")
            .is_some()
    );
}

#[gpui::test]
fn pending_refresh_retries_exact_failed_pages_without_releasing_or_resubmitting_mutation(
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
                100_000,
                window,
                cx,
            );
            picker.set_return_command(Some(PickerCommandState::enabled("All runtimes")), cx);
            picker.set_retry_commands(
                Some(PickerCommandState::enabled("Retry")),
                Some(PickerCommandState::enabled("Retry")),
                cx,
            );
            picker.dispatch_command(PickerCommand::AddRuntime, cx);
            picker.replace_collection(
                PickerCollectionKey("roots-all".into()),
                2,
                100_000,
                None,
                window,
                cx,
            );
            picker.replace_runtime_collection(
                PickerCollectionKey("runtime-registry".into()),
                2,
                100_000,
                cx,
            );
        })
    });
    draw(cx);
    let primary = picker.read_with(cx, |picker, _| {
        picker
            .pending_requests()
            .iter()
            .find(|request| request.query_revision == 2)
            .unwrap()
            .clone()
    });
    let runtime = events
        .borrow()
        .iter()
        .find_map(|event| match event {
            PickerEvent::RequestRuntimePage(request) if request.query_revision == 2 => {
                Some(request.clone())
            }
            _ => None,
        })
        .unwrap();
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.settle_page(
                PickerPageOutcome::Failed {
                    request: primary.clone(),
                    message: "Root read failed".into(),
                },
                window,
                cx,
            );
            picker.settle_runtime_page(
                PickerRuntimePageOutcome::Failed {
                    request: runtime.clone(),
                    message: "Runtime read failed".into(),
                },
                window,
                cx,
            );
            assert!(
                !picker
                    .command_state(&PickerCommand::RetryCollection)
                    .unwrap()
                    .can_dispatch()
            );
            assert!(
                !picker
                    .command_state(&PickerCommand::RetryRuntime)
                    .unwrap()
                    .can_dispatch()
            );
            picker.dispatch_command(PickerCommand::RetryCollection, cx);
            picker.dispatch_command(PickerCommand::RetryRuntime, cx);
            assert!(picker.pending_requests().is_empty());
            assert_eq!(picker.runtime_diagnostics().unwrap().pending_page_count, 0);
            picker.set_pending_page_retry_allowed(true, cx);
            assert!(
                picker
                    .command_state(&PickerCommand::RetryCollection)
                    .unwrap()
                    .can_dispatch()
            );
            assert!(
                picker
                    .command_state(&PickerCommand::RetryRuntime)
                    .unwrap()
                    .can_dispatch()
            );
            picker.set_native_dialog_open(true, cx);
            assert!(
                !picker
                    .command_state(&PickerCommand::RetryCollection)
                    .unwrap()
                    .can_dispatch()
            );
            picker.dispatch_command(PickerCommand::RetryCollection, cx);
            picker.set_native_dialog_open(false, cx);
            picker.dispatch_command(PickerCommand::RetryCollection, cx);
            picker.dispatch_command(PickerCommand::RetryCollection, cx);
            picker.dispatch_command(PickerCommand::RetryRuntime, cx);
            picker.dispatch_command(PickerCommand::RetryRuntime, cx);
            picker.dispatch_command(PickerCommand::AddRuntime, cx);
            picker.dispatch_command(PickerCommand::Return, cx);
            assert_eq!(picker.pending_requests().len(), 1);
            assert_eq!(picker.pending_requests()[0].range, primary.range);
            assert_ne!(picker.pending_requests()[0].request_id, primary.request_id);
            assert_eq!(picker.runtime_diagnostics().unwrap().pending_page_count, 1);
            assert!(
                picker
                    .command_state(&PickerCommand::RetryCollection)
                    .unwrap()
                    .pending
            );
            assert!(
                picker
                    .command_state(&PickerCommand::RetryRuntime)
                    .unwrap()
                    .pending
            );
            assert!(
                picker
                    .command_state(&PickerCommand::AddRuntime)
                    .unwrap()
                    .pending
            );
        })
    });
    draw(cx);
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.restore_command_focus(&PickerCommand::AddRuntime, window, cx);
        })
    });
    cx.simulate_keystrokes("tab");
    cx.update(|window, app| {
        assert!(
            picker
                .read(app)
                .command_focus_handle(&PickerCommand::RetryCollection)
                .unwrap()
                .is_focused(window)
        )
    });
    cx.simulate_keystrokes("enter tab");
    cx.update(|window, app| {
        assert!(
            picker
                .read(app)
                .command_focus_handle(&PickerCommand::RetryRuntime)
                .unwrap()
                .is_focused(window)
        )
    });
    cx.simulate_keystrokes("space tab");
    cx.update(|window, app| {
        assert!(
            picker
                .read(app)
                .command_focus_handle(&PickerCommand::AddRuntime)
                .unwrap()
                .is_focused(window)
        )
    });
    cx.simulate_keystrokes("enter");
    draw(cx);
    let primary_retry = picker.read_with(cx, |picker, _| picker.pending_requests()[0].clone());
    let runtime_retry = events
        .borrow()
        .iter()
        .rev()
        .find_map(|event| match event {
            PickerEvent::RequestRuntimePage(request) if request.query_revision == 2 => {
                Some(request.clone())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(runtime_retry.range, runtime.range);
    assert_ne!(runtime_retry.request_id, runtime.request_id);
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|event| matches!(event, PickerEvent::Command(_)))
            .count(),
        1
    );
    assert_eq!(events.borrow().iter().filter(|event| matches!(event, PickerEvent::RequestPage(request) if request.query_revision == 2)).count(), 2);
    assert_eq!(events.borrow().iter().filter(|event| matches!(event, PickerEvent::RequestRuntimePage(request) if request.query_revision == 2)).count(), 2);
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.set_pending_page_retry_allowed(false, cx);
            picker.settle_page(PickerPageOutcome::Cancelled(primary_retry), window, cx);
            picker.settle_runtime_page(
                PickerRuntimePageOutcome::Cancelled(runtime_retry),
                window,
                cx,
            );
            assert!(
                !picker
                    .command_state(&PickerCommand::RetryCollection)
                    .unwrap()
                    .can_dispatch()
            );
            assert!(
                !picker
                    .command_state(&PickerCommand::RetryRuntime)
                    .unwrap()
                    .can_dispatch()
            );
            picker.set_pending_page_retry_allowed(true, cx);
            picker.finish_command(&PickerCommand::AddRuntime, cx);
            picker.dispatch_command(PickerCommand::AddRuntime, cx);
            assert!(
                !picker
                    .command_state(&PickerCommand::RetryCollection)
                    .unwrap()
                    .can_dispatch()
            );
            assert!(
                !picker
                    .command_state(&PickerCommand::RetryRuntime)
                    .unwrap()
                    .can_dispatch()
            );
        })
    });
}
