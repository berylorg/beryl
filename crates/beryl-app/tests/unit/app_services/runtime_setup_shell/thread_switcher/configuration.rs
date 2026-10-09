use super::*;

fn configured(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    Rc<RefCell<RunningProcessOwner>>,
    gpui::WindowHandle<MainWindowShellRoot>,
    gpui::Entity<ThreadRootPicker>,
    PickerCommand,
    FaultController,
) {
    let (directory, owner, window, faults) = selected_window(cx);
    catalog::populate(&owner, 1);
    let selected = selection(window, cx);
    seed(&owner, selected, 100, 1);
    prepare_reader(&owner, window, 2, cx);
    let picker = open_switcher(window, cx);
    settled(&picker, 2, cx);
    let command = PickerCommand::AddRoot(PickerRowKey(format!(
        "runtime:{}",
        RuntimeId::from_bytes([1; 16])
    )));
    (directory, owner, window, picker, command, faults)
}
fn choose_folder(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    picker: &gpui::Entity<ThreadRootPicker>,
    command: &PickerCommand,
    cx: &mut TestAppContext,
) {
    picker.update(cx, |picker, pcx| {
        picker.dispatch_command(command.clone(), pcx)
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
        "switcher configuration did not invoke the actual folder prompt",
    );
    let options = window
        .read_with(cx, |root, _| root.test_runtime_setup_path_prompt().unwrap())
        .unwrap();
    assert!(options.directories && !options.files && !options.multiple);
}

#[gpui::test]
fn switcher_native_configuration_cancel_preserves_opening_search_focus_and_selection(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, picker, command, _) = configured(cx);
    let before = selection(window, cx);
    let revision = window
        .read_with(cx, |root, _| root.test_thread_switcher_revision().unwrap())
        .unwrap();
    search(window, &picker, "runtime-001", cx);
    settled(&picker, 0, cx);
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
        "cancelled switcher folder prompt retained command",
    );
    assert_eq!(selection(window, cx), before);
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_thread_switcher_revision().unwrap())
            .unwrap(),
        revision
    );
    assert!(cx.update(|app| {
        picker
            .read(app)
            .command_state(&command)
            .unwrap()
            .can_dispatch()
    }));
    assert_eq!(
        cx.update(|app| picker.read(app).query_text().to_owned()),
        "runtime-001"
    );
    window
        .update(cx, |_, window, app| {
            assert!(
                picker
                    .read(app)
                    .command_focus_handle(&command)
                    .unwrap()
                    .is_focused(window)
            )
        })
        .unwrap();
    dismiss(window, cx);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn switcher_existing_root_acknowledgement_refreshes_same_picker_only_after_frozen_target_proof(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, picker, command, _) = configured(cx);
    let before = selection(window, cx);
    choose_folder(window, &picker, &command, cx);
    let process = owner.borrow_mut().test_take_services();
    let (process, flight) = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let flight = graph.runtime_setup().retain_test_outcome(
            crate::runtime_admission::RuntimeAdmissionOutcome::Existing {
                runtime_id: RuntimeId::from_bytes([1; 16]),
                root_id: Some(RootId::from_bytes([1; 16])),
            },
        );
        (process, flight)
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    window
        .update(cx, |root, _, _| {
            root.test_runtime_setup_admission(
                flight,
                Arc::new(std::sync::atomic::AtomicBool::new(true)),
            );
            root.test_complete_runtime_setup_path(Some(vec![std::path::PathBuf::from(
                r"C:\Work\root-001",
            )]));
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            !window
                .read_with(cx, |root, _| root.test_runtime_setup_state().0)
                .unwrap()
        },
        "acknowledged existing root did not settle frozen presentation",
    );
    settled(&picker, 2, cx);
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_thread_switcher_picker().unwrap())
            .unwrap(),
        picker
    );
    assert_eq!(selection(window, cx), before);
    assert!(cx.update(|app| {
        picker
            .read(app)
            .command_state(&command)
            .unwrap()
            .can_dispatch()
    }));
    dismiss(window, cx);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn switcher_original_noncommit_keeps_frozen_query_and_restores_native_command(
    cx: &mut TestAppContext,
) {
    typed_configuration(cx, false, false);
}

#[gpui::test]
fn switcher_original_committed_configuration_waits_exact_new_frozen_root_before_release(
    cx: &mut TestAppContext,
) {
    typed_configuration(cx, true, false);
}

#[gpui::test]
fn switcher_original_indeterminate_configuration_reconciles_without_a_second_native_mutation(
    cx: &mut TestAppContext,
) {
    typed_configuration(cx, true, true);
}

fn typed_configuration(cx: &mut TestAppContext, committed: bool, indeterminate: bool) {
    let (directory, owner, window, picker, command, faults) = configured(cx);
    let before = selection(window, cx);
    let revision = window
        .read_with(cx, |root, _| root.test_thread_switcher_revision().unwrap())
        .unwrap();
    choose_folder(window, &picker, &command, cx);
    let process = owner.borrow_mut().test_take_services();
    let (process, flight) = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let home = graph.home();
        let state = graph.state();
        let runtime = RuntimeId::from_bytes([1; 16]);
        let root_id = RootId::from_bytes([if committed { 3 } else { 1 }; 16]);
        let path = r"C:\Work\new-switcher-root";
        let root = RootRegistration::new(
            root_id,
            RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path)
                .unwrap(),
            AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap(),
            UnixMillis::new(2),
            AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2)).unwrap(),
        );
        let mut mutation = HomeCommand::new(home.home_revision().unwrap());
        mutation
            .add(state.runtime_roots().add_root(
                state.runtime_roots().revision(home).unwrap(),
                beryl_state::AddConfiguredRoot::new(runtime, root),
            ))
            .unwrap();
        if indeterminate {
            faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        }
        let flight = graph.runtime_setup().test_execute_later(
            mutation,
            before.window_id(),
            runtime,
            root_id,
        );
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
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
            root.test_complete_runtime_setup_path(Some(vec![std::path::PathBuf::from(
                r"C:\Work\new-switcher-root",
            )]));
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            !window
                .read_with(cx, |root, _| root.test_runtime_setup_state().0)
                .unwrap()
        },
        "original native configuration outcome did not settle",
    );
    if committed {
        settled(&picker, 2, cx);
        assert!(
            window
                .read_with(cx, |root, _| root.test_thread_switcher_revision().unwrap()
                    > revision)
                .unwrap()
        );
    } else {
        assert_eq!(
            window
                .read_with(cx, |root, _| root.test_thread_switcher_revision().unwrap())
                .unwrap(),
            revision
        );
        assert!(
            window
                .read_with(cx, |root, _| root.test_runtime_setup_notice_persistent())
                .unwrap()
        );
    }
    assert_eq!(selection(window, cx), before);
    assert!(!original.is_pending());
    assert!(!owner.borrow().test_services().running_selection_pending());
    assert!(cx.update(|app| {
        picker
            .read(app)
            .command_state(&command)
            .unwrap()
            .can_dispatch()
    }));
    dismiss(window, cx);
    drop(original);
    dispose_recovered(owner, window, cx);
    drop(directory);
}
