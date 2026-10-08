use super::*;
use beryl_home_store::test_faults::FaultBlock;
use confirmation::{confirm, selected_window, selection, settled_confirmation};

struct HeldReconciliation(FaultBlock);
type VisiblePrior = (gpui::EntityId, usize, Option<RememberedTarget>);

impl Drop for HeldReconciliation {
    fn drop(&mut self) {
        self.0.release();
    }
}

fn pending_original_creation(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    Rc<RefCell<RunningProcessOwner>>,
    gpui::WindowHandle<MainWindowShellRoot>,
    FaultController,
    gpui::Entity<ThreadRootPicker>,
    HeldReconciliation,
    MainWindowComposerSelectionIdentity,
    VisiblePrior,
    beryl_model::HomeRevision,
) {
    let (directory, owner, window, faults) = selected_window(cx);
    cx.update(|app| RunningProcessOwner::mount_ordinary_commands(&owner, app))
        .unwrap();
    catalog::populate(&owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    let prior = selection(window, cx);
    let visible = window
        .read_with(cx, |root, app| {
            root.test_thread_confirmation_visible_identity(app)
        })
        .unwrap();
    let held = HeldReconciliation(faults.block_next(FaultPoint::BeforeReconciliationSnapshot));
    let revision = owner
        .borrow()
        .test_services()
        .graph()
        .unwrap()
        .home()
        .home_revision()
        .unwrap();
    let commit_faults = faults.clone();
    window
        .update(cx, |root, _, _| {
            root.test_thread_confirmation_real_reconciliation_worker();
            root.test_thread_confirmation_hooks(
                Some(Box::new(move |_| {
                    commit_faults.fail_next(FaultPoint::AfterCommitBeforePersist);
                })),
                None,
                None,
            );
        })
        .unwrap();
    confirm(&owner, window, &picker, RootId::from_bytes([1; 16]), cx);
    wait(
        cx,
        |_| held.0.wait_until_reached(Duration::ZERO),
        "original creation reconciliation did not reach its concrete snapshot",
    );
    (
        directory, owner, window, faults, picker, held, prior, visible, revision,
    )
}

fn assert_original_creation_retained(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    picker: &gpui::Entity<ThreadRootPicker>,
    original: (usize, usize),
    prior: MainWindowComposerSelectionIdentity,
    cx: &mut TestAppContext,
) {
    assert_eq!(selection(window, cx).claim(), prior.claim());
    assert_eq!(
        selection(window, cx).binding().candidate().session_id(),
        prior.binding().candidate().session_id()
    );
    let key = PickerRowKey(format!("root:{}", RootId::from_bytes([1; 16])));
    cx.update(|app| {
        let picker = picker.read(app);
        assert_eq!(picker.selected_key(), Some(&key));
        assert!(
            !picker
                .command_state(&PickerCommand::Confirm(key.clone()))
                .unwrap()
                .can_dispatch()
        );
    });
    let close = window
        .read_with(cx, |root, _| {
            assert_eq!(
                root.test_thread_confirmation_original_owner(),
                Some(original)
            );
            assert_eq!(
                root.test_thread_confirmation_visible_transcript_claim(),
                Some(prior.claim())
            );
            root.test_bound_running_window_command().unwrap()
        })
        .unwrap();
    assert!(close.disabled_reason().is_none());
    close.request_close();
    assert!(owner.borrow().exit_requested());
    wait(
        cx,
        |_| !owner.borrow().exit_requested(),
        "actual mounted close did not refuse original selection exclusion",
    );
    assert!(!owner.borrow().test_ordinary_command_status().2);
    assert_eq!(
        owner.borrow().test_last_ordinary_command_failure(),
        Some((Some(prior.window_id()), "window close registry is busy"))
    );
    assert_eq!(cx.windows().len(), 1);
    window
        .update(cx, |root, window, cx| {
            root.test_thread_confirmation_command(key, window, cx)
        })
        .unwrap();
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_thread_confirmation_original_owner())
            .unwrap(),
        Some(original)
    );
    assert!(owner.borrow().test_services().running_selection_pending());
}

#[gpui::test]
fn mounted_pending_creation_retains_original_owner_and_refuses_duplicate_confirm_and_close(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _, picker, held, prior, visible, revision) =
        pending_original_creation(cx);
    let original = window
        .read_with(cx, |root, _| {
            root.test_thread_confirmation_original_owner().unwrap()
        })
        .unwrap();
    assert_eq!(visible.2.unwrap().root_id(), RootId::from_bytes([75; 16]));
    {
        let owner = owner.borrow();
        let home = owner.test_services().graph().unwrap().home();
        assert_eq!(home.pending_reconciliations().len(), 1);
    }
    assert_original_creation_retained(&owner, window, &picker, original, prior, cx);
    assert_eq!(
        window
            .read_with(cx, |root, app| root
                .test_thread_confirmation_visible_identity(app))
            .unwrap(),
        visible
    );
    drop(held);
    settled_confirmation(&owner, window, cx);
    assert_ne!(
        owner
            .borrow()
            .test_services()
            .graph()
            .unwrap()
            .home()
            .home_revision()
            .unwrap(),
        revision
    );
    let target = selection(window, cx);
    assert_ne!(target.claim().thread_id(), prior.claim().thread_id());
    assert_eq!(target.window_id(), prior.window_id());
    assert_eq!(cx.windows().len(), 1);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn mounted_terminal_creation_collision_retains_original_owner_and_visible_prior_until_teardown(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, faults, picker, reconciliation, prior, visible, _) =
        pending_original_creation(cx);
    let original = window
        .read_with(cx, |root, _| {
            root.test_thread_confirmation_original_owner().unwrap()
        })
        .unwrap();
    assert_original_creation_retained(&owner, window, &picker, original, prior, cx);
    let audit = HeldReconciliation(faults.block_next(FaultPoint::BeforeReadConfirmation));
    drop(reconciliation);
    wait(
        cx,
        |_| audit.0.wait_until_reached(Duration::ZERO),
        "original creation audit did not reach its admitted read",
    );
    {
        let owner = owner.borrow();
        let graph = owner.test_services().graph().unwrap();
        let home = graph.home();
        let state = graph.state();
        assert!(home.pending_reconciliations().is_empty());
        let snapshot = state.session().minimal_bootstrap(home).unwrap().unwrap();
        let current = &snapshot.windows()[0];
        assert_ne!(current.selected_thread(), Some(prior.claim()));
        let target = current.selected_thread();
        let placement = beryl_model::WindowPlacement::new(
            beryl_model::WindowBounds::new(20, 20, 840, 620).unwrap(),
            beryl_model::WindowDisplayState::Normal,
            None,
            None,
        );
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command
            .add(state.session().update_placement(
                state.session().revision(home).unwrap(),
                beryl_state::UpdateWindowPlacement::new(
                    snapshot.header().revision(),
                    current.window_id(),
                    current.revision(),
                    placement,
                ),
            ))
            .unwrap();
        assert!(matches!(
            home.execute(command),
            beryl_home_store::CommandOutcome::Committed { .. }
        ));
        let changed = state
            .session()
            .capture_window_removal(home, prior.window_id())
            .unwrap();
        assert_eq!(changed.window().selected_thread(), target);
        assert_ne!(changed.window().revision(), current.revision());
    }
    drop(audit);
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| {
                    root.test_thread_confirmation_original_outcome() == Some("Unavailable")
                        && root.test_runtime_setup_state().1
                })
                .unwrap()
        },
        "original audited collision did not retain terminal Unavailable",
    );
    let revision = owner
        .borrow()
        .test_services()
        .graph()
        .unwrap()
        .home()
        .home_revision()
        .unwrap();
    assert!(
        window
            .read_with(cx, |root, _| root
                .test_thread_confirmation_failure_notice_retained())
            .unwrap()
    );
    let command = PickerCommand::Confirm(PickerRowKey(format!(
        "root:{}",
        RootId::from_bytes([1; 16])
    )));
    let explanation = cx.update(|app| {
        picker
            .read(app)
            .command_state(&command)
            .unwrap()
            .unavailable_reason
            .unwrap()
    });
    assert!(!explanation.is_empty());
    assert_original_creation_retained(&owner, window, &picker, original, prior, cx);
    assert!(
        window
            .read_with(cx, |root, _| root
                .test_thread_confirmation_failure_notice_retained())
            .unwrap()
    );
    assert_eq!(
        cx.update(|app| picker
            .read(app)
            .command_state(&command)
            .unwrap()
            .unavailable_reason
            .unwrap()),
        explanation
    );
    assert_eq!(
        window
            .read_with(cx, |root, app| root
                .test_thread_confirmation_visible_identity(app))
            .unwrap(),
        visible
    );
    assert_eq!(
        owner
            .borrow()
            .test_services()
            .graph()
            .unwrap()
            .home()
            .home_revision()
            .unwrap(),
        revision
    );

    // Terminal fixture teardown preserves the unresolved original outcome.
    stop_observers(&owner, cx);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        owner
            .borrow()
            .test_services()
            .graph()
            .unwrap()
            .home()
            .home_revision()
            .is_err()
    );
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| {
                    root.test_thread_confirmation_workers_drained()
                })
                .unwrap()
        },
        "terminal fixture original activation and setup workers did not drain",
    );
    window
        .read_with(cx, |root, _| {
            assert_eq!(
                root.test_thread_confirmation_original_outcome(),
                Some("Unavailable")
            );
            assert_eq!(
                root.test_thread_confirmation_original_owner(),
                Some(original)
            );
        })
        .unwrap();
    assert_eq!(
        owner
            .borrow()
            .test_services()
            .graph()
            .unwrap()
            .home()
            .health()
            .state(),
        beryl_home_store::HomeHealthState::Failed
    );
    assert!(owner.borrow().test_services().running_selection_pending());
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    drop(picker);
    drop(owner);
    cx.run_until_parked();
    drop(directory);
}

#[gpui::test]
fn cancelled_original_claim_restores_root_search_scope_and_prior_editor(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = selected_window(cx);
    let prior = selection(window, cx);
    catalog::populate(&owner, 2);
    let picker = open(window, cx);
    catalog::settled(&picker, 3, cx);
    window
        .update(cx, |root, window, cx| {
            root.test_runtime_setup_root_scope(Some(RuntimeId::from_bytes([1; 16])), window, cx);
            root.test_thread_confirmation_hooks(
                Some(Box::new(|cancellation| cancellation.cancel())),
                None,
                None,
            );
        })
        .unwrap();
    catalog::settled(&picker, 1, cx);
    let input = cx.update(|app| picker.read(app).search_input());
    input.update(cx, |input, cx| input.replace_selected_text("root-001", cx));
    catalog::settled(&picker, 1, cx);
    let root = RootId::from_bytes([1; 16]);
    confirm(&owner, window, &picker, root, cx);
    settled_confirmation(&owner, window, cx);
    assert_eq!(selection(window, cx).claim(), prior.claim());
    assert_eq!(
        selection(window, cx).binding().host_generation(),
        prior.binding().host_generation()
    );
    let key = PickerRowKey(format!("root:{root}"));
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
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_query().to_owned())
            .unwrap(),
        "root-001"
    );
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_scope())
            .unwrap(),
        Some(RuntimeId::from_bytes([1; 16]))
    );
    assert_eq!(cx.windows().len(), 1);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn opening_again_resets_pending_root_query_and_scope_without_changing_prior_claim(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = selected_window(cx);
    let prior = selection(window, cx);
    catalog::populate(&owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    let key = PickerRowKey(format!("root:{}", RootId::from_bytes([1; 16])));
    picker.update(cx, |picker, cx| picker.activate(&key, cx));
    let input = cx.update(|app| picker.read(app).search_input());
    input.update(cx, |input, cx| input.replace_selected_text("root-001", cx));
    catalog::settled(&picker, 1, cx);
    window
        .update(cx, |_, window, cx| {
            picker.update(cx, |picker, cx| picker.dismiss(window, cx))
        })
        .unwrap();
    let reopened = open(window, cx);
    catalog::settled(&reopened, 2, cx);
    cx.update(|app| {
        let picker = reopened.read(app);
        assert!(picker.selected_key().is_none());
        assert!(
            picker
                .command_state(&PickerCommand::Confirm(key))
                .is_none_or(|state| !state.can_dispatch())
        );
    });
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_query().to_owned())
            .unwrap(),
        ""
    );
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_scope())
            .unwrap()
            .is_none()
    );
    assert_eq!(selection(window, cx).claim(), prior.claim());
    assert!(!owner.borrow().test_services().running_selection_pending());
    dispose_recovered(owner, window, cx);
    drop(directory);
}
