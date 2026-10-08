use super::*;

pub(super) fn selected_window(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    Rc<RefCell<RunningProcessOwner>>,
    gpui::WindowHandle<MainWindowShellRoot>,
    FaultController,
) {
    let (directory, owner, window, faults) = mounted(cx);
    let process = owner.borrow_mut().test_take_services();
    let (process, flight) = std::thread::spawn(move || {
        commit_onboarding(&process, None);
        let flight = process.graph().unwrap().runtime_setup().test_first_flight();
        (process, flight)
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    window
        .update(cx, |root, window, cx| {
            root.test_attach_runtime_setup_flight(flight, window, cx)
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| {
                    root.controller().unwrap().composer_mount().is_some()
                        && !root.test_runtime_setup_state().0
                })
                .unwrap()
        },
        "initial conversation did not publish",
    );
    (directory, owner, window, faults)
}

pub(super) fn selection(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) -> MainWindowComposerSelectionIdentity {
    window
        .read_with(cx, |root, app| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(app)
                .contribution()
                .unwrap()
                .read(app)
                .selection_identity()
        })
        .unwrap()
}

pub(super) fn confirm(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    picker: &gpui::Entity<ThreadRootPicker>,
    root: RootId,
    cx: &mut TestAppContext,
) {
    let invoking = selection(window, cx).window_id();
    let lease =
        cx.update(|app| Arc::new(owner.borrow().admit_thread_creation(invoking, app).unwrap()));
    window
        .update(cx, |root, _, _| root.test_thread_confirmation_lease(lease))
        .unwrap();
    let key = PickerRowKey(format!("root:{root}"));
    picker.update(cx, |picker, pcx| picker.activate(&key, pcx));
    wait(
        cx,
        |cx| {
            cx.update(|app| {
                picker
                    .read(app)
                    .command_state(&PickerCommand::Confirm(key.clone()))
                    .is_some_and(|state| state.can_dispatch())
            })
        },
        "Confirm did not become eligible",
    );
    picker.update(cx, |picker, pcx| {
        picker.dispatch_command(PickerCommand::Confirm(key), pcx)
    });
}

pub(super) fn settled_confirmation(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) {
    let diagnostic_deadline = Instant::now() + Duration::from_secs(3);
    let mut reported = false;
    wait(
        cx,
        |cx| {
            if !reported && Instant::now() >= diagnostic_deadline {
                reported = true;
                eprintln!(
                    "retained confirmation: {:?}",
                    window
                        .read_with(cx, |root, _| root.test_thread_confirmation_diagnostics())
                        .unwrap()
                );
            }
            !owner.borrow().test_services().running_selection_pending()
                && window
                    .read_with(cx, |root, _| !root.test_runtime_setup_state().0)
                    .unwrap()
        },
        "root confirmation did not settle",
    );
}

pub(super) fn edit_prior(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    text: &str,
    cx: &mut TestAppContext,
) {
    window
        .update(cx, |root, window, cx| {
            root.notice_safe_focus(cx).focus(window)
        })
        .unwrap();
    for (index, character) in text.chars().enumerate() {
        gpui::VisualTestContext::from_window(window.into(), cx)
            .simulate_input(&character.to_string());
        wait(
            cx,
            |cx| {
                selection(window, cx)
                    .binding()
                    .logical_extent()
                    .logical_utf8_bytes()
                    == (index + 1) as u64
            },
            "prior edit did not settle",
        );
    }
}

#[gpui::test]
fn edited_prior_is_saved_once_before_same_window_root_creation(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = selected_window(cx);
    let prior = selection(window, cx);
    edit_prior(window, "retained prior draft", cx);
    catalog::populate(&owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    confirm(&owner, window, &picker, RootId::from_bytes([1; 16]), cx);
    settled_confirmation(&owner, window, cx);
    let target = selection(window, cx);
    assert_ne!(target.claim().thread_id(), prior.claim().thread_id());
    assert_eq!(target.binding().logical_extent().logical_utf8_bytes(), 0);
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let text = graph
            .syndic()
            .current_draft_piece_text_demand(
                graph.home(),
                prior.claim().thread_id(),
                syndic_storage::DraftPieceTextDemandV1::Forward(0),
                4096,
            )
            .unwrap()
            .unwrap();
        assert_eq!(text.value().bytes(), b"retained prior draft");
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn returning_to_unoccupied_pristine_root_reuses_original_thread(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = selected_window(cx);
    let prior = selection(window, cx);
    catalog::populate(&owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    confirm(&owner, window, &picker, RootId::from_bytes([1; 16]), cx);
    settled_confirmation(&owner, window, cx);
    let intermediate = selection(window, cx);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    confirm(&owner, window, &picker, RootId::from_bytes([75; 16]), cx);
    settled_confirmation(&owner, window, cx);
    let reused = selection(window, cx);
    assert_eq!(reused.claim().thread_id(), prior.claim().thread_id());
    assert_ne!(reused.claim(), prior.claim());
    assert_ne!(reused.claim().thread_id(), intermediate.claim().thread_id());
    assert_eq!(reused.window_id(), prior.window_id());
    assert_eq!(cx.windows().len(), 1);
    assert!(
        window
            .read_with(cx, |root, app| root
                .test_first_conversation_transcript_claim(reused.claim(), app))
            .unwrap()
    );
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn offpage_root_selection_survives_search_and_scope_reset_clears_selection(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = selected_window(cx);
    let prior = selection(window, cx);
    catalog::populate(&owner, 2);
    let picker = open(window, cx);
    catalog::settled(&picker, 3, cx);
    let key = PickerRowKey(format!("root:{}", RootId::from_bytes([1; 16])));
    picker.update(cx, |picker, cx| picker.activate(&key, cx));
    let input = cx.update(|app| picker.read(app).search_input());
    input.update(cx, |input, cx| input.replace_selected_text("root-002", cx));
    catalog::settled(&picker, 1, cx);
    cx.update(|app| {
        let picker = picker.read(app);
        assert_eq!(picker.selected_key(), Some(&key));
        assert!(
            picker
                .command_state(&PickerCommand::Confirm(key.clone()))
                .unwrap()
                .can_dispatch()
        );
    });
    assert_eq!(selection(window, cx), prior);
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(
            PickerCommand::BrowseRoots(PickerRowKey(format!(
                "runtime:{}",
                RuntimeId::from_bytes([2; 16])
            ))),
            cx,
        )
    });
    catalog::settled(&picker, 1, cx);
    cx.update(|app| {
        let picker = picker.read(app);
        assert!(picker.query_text().is_empty());
        assert!(picker.selected_key().is_none());
    });
    window
        .update(cx, |_, window, cx| {
            picker.update(cx, |picker, cx| picker.dismiss(window, cx))
        })
        .unwrap();
    let picker = open(window, cx);
    catalog::settled(&picker, 3, cx);
    cx.update(|app| {
        let picker = picker.read(app);
        assert!(picker.query_text().is_empty());
        assert!(picker.selected_key().is_none());
    });
    assert_eq!(selection(window, cx), prior);
    window
        .update(cx, |_, window, cx| {
            picker.update(cx, |picker, cx| picker.dismiss(window, cx))
        })
        .unwrap();
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn duplicate_confirm_and_dismissal_before_commit_preserve_prior_claim(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = selected_window(cx);
    let prior = selection(window, cx);
    catalog::populate(&owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    let selected = RootId::from_bytes([1; 16]);
    let key = PickerRowKey(format!("root:{selected}"));
    confirm(&owner, window, &picker, selected, cx);
    assert!(owner.borrow().test_services().running_selection_pending());
    assert_eq!(selection(window, cx).claim(), prior.claim());
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(PickerCommand::Confirm(key.clone()), cx)
    });
    window
        .update(cx, |root, window, cx| {
            root.test_thread_confirmation_command(key, window, cx)
        })
        .unwrap();
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_state().0)
            .unwrap()
    );
    window
        .update(cx, |_, window, cx| {
            picker.update(cx, |picker, cx| picker.dismiss(window, cx))
        })
        .unwrap();
    settled_confirmation(&owner, window, cx);
    assert_eq!(selection(window, cx).claim(), prior.claim());
    assert_eq!(
        selection(window, cx).binding().host_generation(),
        prior.binding().host_generation()
    );
    assert!(!owner.borrow().test_services().running_selection_pending());
    assert_eq!(cx.windows().len(), 1);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn selected_root_confirm_creates_in_same_window_without_running_membership(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = selected_window(cx);
    let original = selection(window, cx);
    catalog::populate(&owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    confirm(&owner, window, &picker, RootId::from_bytes([1; 16]), cx);
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| !root.test_runtime_setup_state().0)
                .unwrap()
        },
        "root confirmation did not settle",
    );
    let current = selection(window, cx);
    settled_confirmation(&owner, window, cx);
    assert_eq!(current.window_id(), original.window_id());
    assert_ne!(current.claim(), original.claim());
    assert_eq!(cx.windows().len(), 1);
    assert!(
        window
            .read_with(cx, |root, app| root.test_runtime_setup_picker().is_none()
                && root
                    .test_first_conversation_transcript_claim(current.claim(), app))
            .unwrap()
    );
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let record = graph
            .state()
            .session()
            .capture_window_removal(graph.home(), current.window_id())
            .unwrap();
        assert_eq!(record.window().selected_thread(), Some(current.claim()));
        assert_eq!(
            record.window().remembered_target(),
            Some(RememberedTarget::new(
                RuntimeId::from_bytes([1; 16]),
                RootId::from_bytes([1; 16])
            ))
        );
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    assert!(!owner.borrow().test_services().running_selection_pending());
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn current_pristine_root_confirm_preserves_editor_claim_and_window(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = selected_window(cx);
    let original = selection(window, cx);
    let picker = open(window, cx);
    catalog::settled(&picker, 1, cx);
    confirm(&owner, window, &picker, RootId::from_bytes([75; 16]), cx);
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| !root.test_runtime_setup_state().0)
                .unwrap()
        },
        "Current confirmation did not settle",
    );
    let current = selection(window, cx);
    assert_eq!(current.claim(), original.claim());
    assert_eq!(
        current.binding().host_generation(),
        original.binding().host_generation()
    );
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_picker().is_none())
            .unwrap()
    );
    assert!(!owner.borrow().test_services().running_selection_pending());
    dispose_recovered(owner, window, cx);
    drop(directory);
}
