use super::*;

#[gpui::test]
fn pending_owner_updates_restore_original_runtime_and_root_commands_after_cancellation(
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
    for command in [
        PickerCommand::AddRuntime,
        PickerCommand::AddRoot(PickerRowKey("row-0".into())),
    ] {
        cx.update(|window, app| {
            picker.update(app, |picker, cx| {
                let original = picker.command_state(&command).unwrap();
                let resident = picker.runtime_diagnostics().unwrap().resident_row_count;
                picker.restore_command_focus(&command, window, cx);
                picker.dispatch_command(command.clone(), cx);
                for label in ["Selecting", "Validating", "Reconciling"] {
                    picker.update_command_state(
                        &command,
                        PickerCommandState {
                            label: label.into(),
                            unavailable_reason: None,
                            pending: true,
                        },
                        cx,
                    );
                    let state = picker.command_state(&command).unwrap();
                    assert_eq!(state.label, label);
                    assert!(state.pending);
                    assert!(!state.can_dispatch());
                    picker.dispatch_command(command.clone(), cx);
                }
                picker.set_native_dialog_open(true, cx);
                picker.set_native_dialog_open(false, cx);
                picker.finish_command(&command, cx);
                picker.restore_command_focus(&command, window, cx);
                assert_eq!(picker.command_state(&command), Some(original));
                assert!(picker.command_state(&command).unwrap().can_dispatch());
                assert_eq!(
                    picker.runtime_diagnostics().unwrap().resident_row_count,
                    resident
                );
                assert!(
                    picker
                        .command_focus_handle(&command)
                        .unwrap()
                        .is_focused(window)
                );
                picker.dispatch_command(command.clone(), cx);
                picker.finish_command(&command, cx);
            })
        });
        draw(cx);
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|event| **event == PickerEvent::Command(command.clone()))
                .count(),
            2
        );
    }
}
