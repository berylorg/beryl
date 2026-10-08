use super::*;

fn scoped_picker(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    Rc<RefCell<RunningProcessOwner>>,
    gpui::WindowHandle<MainWindowShellRoot>,
    gpui::Entity<ThreadRootPicker>,
    PickerCommand,
) {
    let (directory, owner, window, _) = mounted(cx);
    catalog::populate(&owner, 2);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    let runtime = RuntimeId::from_bytes([1; 16]);
    let command = PickerCommand::AddRoot(PickerRowKey(format!("runtime:{runtime}")));
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(
            PickerCommand::BrowseRoots(PickerRowKey(format!("runtime:{runtime}"))),
            cx,
        )
    });
    catalog::settled(&picker, 1, cx);
    (directory, owner, window, picker, command)
}

fn choose_folder(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    picker: &gpui::Entity<ThreadRootPicker>,
    command: &PickerCommand,
    cx: &mut TestAppContext,
) {
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(command.clone(), cx)
    });
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| {
                    root.test_runtime_setup_path_prompt().is_some()
                })
                .unwrap()
        },
        "native folder picker did not open",
    );
    let options = window
        .read_with(cx, |root, _| root.test_runtime_setup_path_prompt().unwrap())
        .unwrap();
    assert!(!options.files && options.directories && !options.multiple);
    assert_eq!(options.prompt.unwrap().as_ref(), "Choose a root directory");
}

#[gpui::test]
fn scoped_add_root_cancel_preserves_query_selection_and_command_focus(cx: &mut TestAppContext) {
    let (directory, owner, window, picker, command) = scoped_picker(cx);
    let input = cx.update(|app| picker.read(app).search_input());
    input.update(cx, |input, cx| input.replace_selected_text("root-001", cx));
    catalog::settled(&picker, 1, cx);
    let selected = PickerRowKey(format!("root:{}", RootId::from_bytes([1; 16])));
    picker.update(cx, |picker, cx| picker.activate(&selected, cx));
    choose_folder(window, &picker, &command, cx);
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(command.clone(), cx);
        picker.dispatch_command(PickerCommand::Return, cx);
    });
    window
        .update(cx, |root, _, _| root.test_complete_runtime_setup_path(None))
        .unwrap();
    wait(
        cx,
        |cx| {
            !window
                .read_with(cx, |root, _| root.test_runtime_setup_state().0)
                .unwrap()
        },
        "folder cancellation remained pending",
    );
    cx.update(|app| {
        let picker = picker.read(app);
        assert_eq!(picker.query_text(), "root-001");
        assert_eq!(picker.selected_key(), Some(&selected));
        assert!(picker.command_state(&command).unwrap().can_dispatch());
    });
    window
        .update(cx, |_, window, cx| {
            assert!(
                picker
                    .read(cx)
                    .command_focus_handle(&command)
                    .unwrap()
                    .is_focused(window)
            );
        })
        .unwrap();
    close_empty(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn native_folder_error_preserves_scoped_catalog_and_allows_retry(cx: &mut TestAppContext) {
    let (directory, owner, window, picker, command) = scoped_picker(cx);
    choose_folder(window, &picker, &command, cx);
    window
        .update(cx, |root, _, _| {
            root.test_fail_runtime_setup_path("folder chooser failed")
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            !window
                .read_with(cx, |root, _| root.test_runtime_setup_state().0)
                .unwrap()
        },
        "folder error remained pending",
    );
    assert!(cx.update(|app| {
        picker
            .read(app)
            .command_state(&command)
            .unwrap()
            .can_dispatch()
    }));
    assert_eq!(
        cx.update(|app| picker.read(app).diagnostics().total_count),
        1
    );
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_notice_persistent())
            .unwrap()
    );
    choose_folder(window, &picker, &command, cx);
    window
        .update(cx, |root, _, _| root.test_complete_runtime_setup_path(None))
        .unwrap();
    wait(
        cx,
        |cx| {
            !window
                .read_with(cx, |root, _| root.test_runtime_setup_state().0)
                .unwrap()
        },
        "retried folder cancellation remained pending",
    );
    close_empty(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn retired_setup_rejects_late_native_folder_completion(cx: &mut TestAppContext) {
    let (directory, owner, window, picker, command) = scoped_picker(cx);
    choose_folder(window, &picker, &command, cx);
    window
        .update(cx, |root, window, cx| {
            root.retire_notices(window, cx);
            root.test_complete_runtime_setup_path(Some(vec![std::path::PathBuf::from(
                r"C:\Work\late-root",
            )]));
        })
        .unwrap();
    cx.run_until_parked();
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_picker().is_none()
                && !root.test_runtime_setup_state().2)
            .unwrap()
    );
    assert!(
        !owner
            .borrow()
            .test_services()
            .graph()
            .unwrap()
            .runtime_setup()
            .has_pending_flights()
    );
    close_empty(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn committed_root_failed_page_retry_keeps_original_lease_and_mutates_once(cx: &mut TestAppContext) {
    known_root_failed_page_retry(cx, true);
}

#[gpui::test]
fn existing_root_failed_page_retry_restores_command_without_resubmitting_mutation(
    cx: &mut TestAppContext,
) {
    known_root_failed_page_retry(cx, false);
}

fn known_root_failed_page_retry(cx: &mut TestAppContext, committed: bool) {
    let (directory, owner, window, picker, command) = scoped_picker(cx);
    choose_folder(window, &picker, &command, cx);
    let process = owner.borrow_mut().test_take_services();
    let (process, flight) = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let home = graph.home();
        let state = graph.state();
        let id = state
            .session()
            .minimal_bootstrap(home)
            .unwrap()
            .unwrap()
            .windows()[0]
            .window_id();
        let runtime = RuntimeId::from_bytes([1; 16]);
        let flight = if committed {
            let root_id = RootId::from_bytes([3; 16]);
            let path = r"C:\Work\root-added";
            let root = RootRegistration::new(
                root_id,
                RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path)
                    .unwrap(),
                AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap(),
                UnixMillis::new(2),
                AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2))
                    .unwrap(),
            );
            let mut mutation = HomeCommand::new(home.home_revision().unwrap());
            mutation
                .add(state.runtime_roots().add_root(
                    state.runtime_roots().revision(home).unwrap(),
                    beryl_state::AddConfiguredRoot::new(runtime, root),
                ))
                .unwrap();
            graph
                .runtime_setup()
                .test_execute_later(mutation, id, runtime, root_id)
        } else {
            graph.runtime_setup().retain_test_outcome(
                crate::runtime_admission::RuntimeAdmissionOutcome::Existing {
                    runtime_id: runtime,
                    root_id: Some(RootId::from_bytes([1; 16])),
                },
            )
        };
        (process, flight)
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    let original = flight.clone();
    window
        .update(cx, |root, _, _| {
            root.test_runtime_setup_admission(
                flight,
                Arc::new(std::sync::atomic::AtomicBool::new(true)),
            );
            root.test_complete_runtime_setup_path(Some(vec![std::path::PathBuf::from(
                if committed {
                    r"C:\Work\root-added"
                } else {
                    r"C:\Work\root-001"
                },
            )]));
        })
        .unwrap();
    wait(
        cx,
        |cx| cx.update(|app| picker.read(app).diagnostics().collection_failed),
        "postcommit root collection failure did not surface",
    );
    assert_eq!(
        owner.borrow().test_services().running_selection_pending(),
        committed
    );
    cx.update(|app| {
        let picker = picker.read(app);
        assert!(
            picker
                .command_state(&PickerCommand::RetryCollection)
                .unwrap()
                .can_dispatch()
        );
        assert!(!picker.command_state(&command).unwrap().can_dispatch());
        assert!(
            !picker
                .command_state(&PickerCommand::AddRuntime)
                .unwrap()
                .can_dispatch()
        );
    });
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(command.clone(), cx);
        picker.dispatch_command(PickerCommand::AddRuntime, cx);
        picker.dispatch_command(PickerCommand::RetryCollection, cx);
        picker.dispatch_command(PickerCommand::RetryRuntime, cx);
    });
    catalog::settled(&picker, if committed { 2 } else { 1 }, cx);
    wait(
        cx,
        |cx| {
            !window
                .read_with(cx, |root, _| root.test_runtime_setup_state().0)
                .unwrap()
        },
        "original committed refresh did not finish",
    );
    assert!(!owner.borrow().test_services().running_selection_pending());
    assert!(Arc::ptr_eq(
        &owner
            .borrow()
            .test_services()
            .graph()
            .unwrap()
            .runtime_setup()
            .test_first_flight(),
        &original
    ));
    assert!(cx.update(|app| {
        picker
            .read(app)
            .command_state(&command)
            .unwrap()
            .can_dispatch()
    }));
    close_empty(owner, window, cx);
    drop(directory);
}

fn executable_form_cancellation(cx: &mut TestAppContext, choice: &str, expected_prompt: &str) {
    let (directory, owner, window, _) = mounted(cx);
    let picker = open(window, cx);
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(PickerCommand::AddRuntime, cx)
    });
    wait(
        cx,
        |cx| cx.has_pending_prompt(),
        "runtime form prompt did not open",
    );
    cx.simulate_prompt_answer(choice);
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| {
                    root.test_runtime_setup_path_prompt().is_some()
                })
                .unwrap()
        },
        "native file prompt did not open",
    );
    let options = window
        .read_with(cx, |root, _| root.test_runtime_setup_path_prompt().unwrap())
        .unwrap();
    assert!(options.files && !options.directories && !options.multiple);
    assert_eq!(options.prompt.as_ref().unwrap().as_ref(), expected_prompt);
    assert!(cx.update(|app| {
        !picker
            .read(app)
            .command_state(&PickerCommand::AddRuntime)
            .unwrap()
            .can_dispatch()
    }));
    window
        .update(cx, |root, _, _| root.test_complete_runtime_setup_path(None))
        .unwrap();
    wait(
        cx,
        |cx| {
            !window
                .read_with(cx, |root, _| root.test_runtime_setup_state().0)
                .unwrap()
        },
        "file cancellation stayed pending",
    );
    assert_eq!(
        cx.update(|app| picker.read(app).query_text().to_owned()),
        ""
    );
    assert!(cx.update(|app| picker.read(app).selected_key().is_none()));
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_picker().unwrap()
                == picker)
            .unwrap()
    );
    close_empty(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn setup_retains_standalone_form_through_native_file_cancellation(cx: &mut TestAppContext) {
    executable_form_cancellation(
        cx,
        "Codex App Server binary",
        "Choose a Codex App Server binary",
    );
}

#[gpui::test]
fn setup_retains_cli_form_through_native_file_cancellation(cx: &mut TestAppContext) {
    executable_form_cancellation(cx, "Codex CLI binary", "Choose a Codex CLI binary");
}

#[gpui::test]
fn invalid_native_result_preserves_picker_and_restores_original_command(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = mounted(cx);
    let picker = open(window, cx);
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(PickerCommand::AddRuntime, cx)
    });
    wait(
        cx,
        |cx| cx.has_pending_prompt(),
        "runtime choice did not open",
    );
    cx.simulate_prompt_answer("Codex CLI binary");
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| {
                    root.test_runtime_setup_path_prompt().is_some()
                })
                .unwrap()
        },
        "file picker did not open",
    );
    window
        .update(cx, |root, _, _| {
            root.test_complete_runtime_setup_path(Some(vec![
                std::path::PathBuf::from(r"C:\one.exe"),
                std::path::PathBuf::from(r"C:\two.exe"),
            ]))
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            !window
                .read_with(cx, |root, _| root.test_runtime_setup_state().0)
                .unwrap()
        },
        "invalid native result stayed pending",
    );
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_notice_persistent())
            .unwrap()
    );
    assert!(cx.update(|app| {
        picker
            .read(app)
            .command_state(&PickerCommand::AddRuntime)
            .unwrap()
            .can_dispatch()
    }));
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_picker().unwrap()
                == picker
                && root.controller().unwrap().is_threadless())
            .unwrap()
    );
    close_empty(owner, window, cx);
    drop(directory);
}
