use super::*;

#[gpui::test]
fn primary_reuses_oldest_eligible_empty_thread_in_selected_binding(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = selected_window(cx);
    let prior = selection(window, cx);
    edit_prior(window, "edited predecessor", cx);
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let execution = graph
            .syndic()
            .thread_execution(
                graph.home(),
                prior.claim().thread_id(),
                syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap();
        for (seed, timestamp) in [(92, 2), (91, 1)] {
            let thread = SyndicThreadId::from_bytes([seed; 16]);
            let mut command = HomeCommand::new(graph.home().home_revision().unwrap());
            command
                .add(graph.syndic().create_thread(
                    graph.syndic().revision(graph.home()).unwrap(),
                    CreateThread::ordinary(
                        thread,
                        SyndicDraftId::from_bytes([seed; 16]),
                        execution.execution().clone(),
                        syndic_storage::SyndicTimestamp::from_unix_millis(timestamp),
                        DraftEditHistoryPolicyV1::new(8 * 1024 * 1024, 1).unwrap(),
                    ),
                ))
                .unwrap();
            assert!(matches!(
                graph.home().execute(command),
                beryl_home_store::CommandOutcome::Committed { .. }
            ));
            let projected = crate::catalog_projection::prepare_thread_catalog_projection(
                graph.home(),
                graph.syndic(),
                graph.state(),
                thread,
            )
            .unwrap();
            let crate::catalog_projection::ThreadCatalogProjectionPreparation::Publish(command) =
                projected
            else {
                panic!("expected new eligible thread projection")
            };
            assert!(matches!(
                graph.home().execute(command),
                beryl_home_store::CommandOutcome::Committed { .. }
            ));
        }
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    admit(&owner, window, cx);
    invoke(window, cx);
    settled_confirmation(&owner, window, cx);
    let selected = selection(window, cx);
    assert_eq!(
        selected.claim().thread_id(),
        SyndicThreadId::from_bytes([91; 16])
    );
    assert_eq!(selected.window_id(), prior.window_id());
    assert_target(&owner, selected, RootId::from_bytes([75; 16]));
    assert_eq!(cx.windows().len(), 1);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn cancelled_primary_restores_exact_flyout_selection_search_scope_and_primary_focus(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = selected_window(cx);
    edit_prior(window, "saved before cancellation", cx);
    let prior = selection(window, cx);
    let visible = window
        .read_with(cx, |root, app| {
            root.test_thread_confirmation_visible_identity(app)
        })
        .unwrap();
    catalog::populate(&owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    let key = PickerRowKey(format!("root:{}", RootId::from_bytes([1; 16])));
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(
            PickerCommand::BrowseRoots(PickerRowKey(format!(
                "runtime:{}",
                RuntimeId::from_bytes([1; 16])
            ))),
            cx,
        )
    });
    catalog::settled(&picker, 1, cx);
    picker.update(cx, |picker, cx| picker.activate(&key, cx));
    let input = cx.update(|app| picker.read(app).search_input());
    input.update(cx, |input, cx| input.replace_selected_text("001", cx));
    input.update(cx, |input, cx| input.set_selection(1..2, false, cx));
    catalog::settled(&picker, 1, cx);
    let search_before = super::inspection::search_state(&input, cx);
    cx.update(|app| {
        for command in [
            PickerCommand::Confirm(key.clone()),
            PickerCommand::AddRuntime,
            PickerCommand::AddRoot(PickerRowKey(format!(
                "runtime:{}",
                RuntimeId::from_bytes([1; 16])
            ))),
        ] {
            assert!(
                picker
                    .read(app)
                    .command_state(&command)
                    .unwrap()
                    .can_dispatch()
            );
        }
    });
    window
        .update(cx, |root, _, _| {
            root.test_thread_confirmation_hooks(
                Some(Box::new(|cancel| cancel.cancel())),
                None,
                None,
            )
        })
        .unwrap();
    admit(&owner, window, cx);
    invoke(window, cx);
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    picker.update(cx, |picker, cx| {
        for command in [
            PickerCommand::Confirm(key.clone()),
            PickerCommand::AddRuntime,
            PickerCommand::AddRoot(PickerRowKey(format!(
                "runtime:{}",
                RuntimeId::from_bytes([1; 16])
            ))),
        ] {
            assert!(!picker.command_state(&command).unwrap().can_dispatch());
            picker.dispatch_command(command, cx);
        }
        picker.activate(
            &PickerRowKey(format!("root:{}", RootId::from_bytes([75; 16]))),
            cx,
        );
        assert_eq!(picker.selected_key(), Some(&key));
    });
    settled_confirmation(&owner, window, cx);
    assert!(input.read_with(cx, |input, _| input.is_enabled()));
    assert_eq!(super::inspection::search_state(&input, cx), search_before);
    assert_eq!(selection(window, cx).claim(), prior.claim());
    assert_eq!(
        window
            .read_with(cx, |root, app| root
                .test_thread_confirmation_visible_identity(app))
            .unwrap(),
        visible
    );
    cx.update(|app| {
        assert_eq!(picker.read(app).selected_key(), Some(&key));
        assert_eq!(picker.read(app).query_text(), "001");
    });
    window
        .update(cx, |root, window, cx| {
            assert_eq!(
                root.test_runtime_setup_scope(),
                Some(RuntimeId::from_bytes([1; 16]))
            );
            assert!(root.test_new_thread_focus(true).is_focused(window));
            assert!(root.test_primary_thread_reason(cx).is_none());
        })
        .unwrap();
    assert!(cx.update(|app| {
        picker
            .read(app)
            .command_state(&PickerCommand::Confirm(key))
            .unwrap()
            .can_dispatch()
    }));
    confirmation::confirm(&owner, window, &picker, RootId::from_bytes([1; 16]), cx);
    settled_confirmation(&owner, window, cx);
    assert_target(&owner, selection(window, cx), RootId::from_bytes([1; 16]));
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn pending_primary_retains_original_creation_owner_and_refuses_all_duplicate_entrances(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, faults) = selected_window(cx);
    edit_prior(window, "pending original primary", cx);
    let prior = selection(window, cx);
    focus(window, true, cx);
    let before_bounds = gpui::VisualTestContext::from_window(window.into(), cx)
        .debug_bounds("main-window-new-thread-primary")
        .unwrap();
    let visible = window
        .read_with(cx, |root, app| {
            root.test_thread_confirmation_visible_identity(app)
        })
        .unwrap();
    let held = HeldSnapshot(faults.block_next(FaultPoint::BeforeReconciliationSnapshot));
    let commit_faults = faults.clone();
    window
        .update(cx, |root, _, _| {
            root.test_thread_confirmation_real_reconciliation_worker();
            root.test_thread_confirmation_hooks(
                Some(Box::new(move |_| {
                    commit_faults.fail_next(FaultPoint::AfterCommitBeforePersist)
                })),
                None,
                None,
            );
        })
        .unwrap();
    admit(&owner, window, cx);
    invoke(window, cx);
    wait(
        cx,
        |_| held.0.wait_until_reached(Duration::ZERO),
        "original primary reconciliation snapshot was not reached",
    );
    let original = window
        .read_with(cx, |root, _| {
            root.test_thread_confirmation_original_owner().unwrap()
        })
        .unwrap();
    cx.update(|app| {
        app.update_window(window.into(), |_, window, app| {
            window.draw_and_present_for_test(app)
        })
        .unwrap()
    });
    assert_eq!(
        gpui::VisualTestContext::from_window(window.into(), cx)
            .debug_bounds("main-window-new-thread-primary")
            .unwrap(),
        before_bounds
    );
    window
        .update(cx, |root, window, cx| {
            assert!(root.test_primary_thread_reason(cx).is_some());
            root.test_primary_thread_command(window, cx);
            root.test_open_runtime_setup(window, cx);
            assert_eq!(
                root.test_thread_confirmation_original_owner(),
                Some(original)
            );
            assert!(root.test_runtime_setup_picker().is_none());
        })
        .unwrap();
    assert_eq!(selection(window, cx).claim(), prior.claim());
    assert_eq!(
        window
            .read_with(cx, |root, app| root
                .test_thread_confirmation_visible_identity(app))
            .unwrap(),
        visible
    );
    assert!(owner.borrow().test_services().running_selection_pending());
    held.0.release();
    settled_confirmation(&owner, window, cx);
    let selected = selection(window, cx);
    assert_ne!(selected.claim().thread_id(), prior.claim().thread_id());
    assert_target(&owner, selected, RootId::from_bytes([75; 16]));
    assert!(!owner.borrow().test_services().running_selection_pending());
    dispose_recovered(owner, window, cx);
    drop(directory);
}

struct HeldSnapshot(beryl_home_store::test_faults::FaultBlock);

impl Drop for HeldSnapshot {
    fn drop(&mut self) {
        self.0.release();
    }
}
