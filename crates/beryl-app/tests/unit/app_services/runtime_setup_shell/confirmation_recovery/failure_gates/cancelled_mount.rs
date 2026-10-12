use super::*;

#[gpui::test]
fn refused_fresh_creation_mount_cancels_exact_editor_before_original_target_retry(
    cx: &mut TestAppContext,
) {
    cancelled_fresh_creation_mount(false, cx);
}

#[gpui::test]
fn refused_fresh_transcript_mount_restores_original_subscription_before_target_retry(
    cx: &mut TestAppContext,
) {
    cancelled_fresh_creation_mount(true, cx);
}

fn cancelled_fresh_creation_mount(transcript_replaced: bool, cx: &mut TestAppContext) {
    let (directory, owner, window, faults) = selected_window(cx);
    let original_transcript = resource_wakeups::capture(window, cx);
    let native = window.window_id();
    let original_mount = window
        .read_with(cx, |root, _| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .entity_id()
        })
        .unwrap();
    window
        .update(cx, |root, _, _| {
            if transcript_replaced {
                root.test_reject_thread_creation_transcript_mount();
            } else {
                root.test_reject_thread_creation_recovery_mount();
            }
        })
        .unwrap();
    let target = interrupt_committed_creation(&owner, window, faults, cx);
    wait(
        cx,
        |cx| {
            cx.update(|app| RunningProcessOwner::test_observe_running_home_failure(&owner, app));
            match owner.borrow().automatic_recovery_outcome().as_deref() {
                Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => {
                    let expected = if transcript_replaced {
                        "retained fresh New Thread transcript mount was refused"
                    } else {
                        "retained fresh New Thread mount was refused"
                    };
                    assert!(error.contains(expected));
                    true
                }
                Some(
                    InterruptedExitRecoveryOutcome::Completed
                    | InterruptedExitRecoveryOutcome::Cancelled,
                ) => panic!("refused fresh mount settled without exact cleanup"),
                _ => false,
            }
        },
        "fresh mount failure did not retain original recovery custody",
    );
    let (fresh_mount, fresh_close, gated) = window
        .read_with(cx, |root, app| {
            root.test_thread_creation_recovery_mount(app)
        })
        .unwrap()
        .unwrap();
    assert_ne!(fresh_mount, original_mount);
    let candidate_transcript = resource_wakeups::capture(window, cx);
    assert_eq!(
        candidate_transcript.entity_id() != original_transcript.entity_id(),
        transcript_replaced
    );
    if transcript_replaced {
        resource_wakeups::verify_replacement(window, original_transcript.clone(), cx);
    }
    assert!(gated);
    assert!(owner.borrow().test_services().running_selection_pending());
    assert!(
        owner
            .borrow()
            .test_services()
            .test_recovery_process_is_fenced()
    );
    let settled = Rc::new(RefCell::new(None));
    let returned = settled.clone();
    let retained = owner.clone();
    cx.update(|app| {
        app.spawn(async move |cx| {
            *returned.borrow_mut() = Some(
                RunningProcessOwner::test_settle_running_home_recovery_cancellation(&retained, cx)
                    .await,
            );
        })
        .detach()
    });
    wait(
        cx,
        |_| settled.borrow().is_some(),
        "retained fresh creation editor cleanup did not return",
    );
    settled.borrow_mut().take().unwrap().unwrap();
    let restored_transcript = resource_wakeups::capture(window, cx);
    assert_eq!(
        restored_transcript.entity_id(),
        original_transcript.entity_id()
    );
    resource_wakeups::verify_retained(window, original_transcript.clone(), cx);
    if transcript_replaced {
        resource_wakeups::verify_replacement(window, candidate_transcript.clone(), cx);
    }
    drop(candidate_transcript);
    assert!(
        window
            .read_with(cx, |root, app| root
                .test_thread_creation_recovery_mount(app))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        window
            .read_with(cx, |root, _| root
                .controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .entity_id())
            .unwrap(),
        original_mount
    );
    assert!(owner.borrow().test_services().running_selection_pending());
    assert!(
        owner
            .borrow()
            .test_services()
            .test_recovery_process_is_fenced()
    );
    let continued = Rc::new(RefCell::new(None));
    let returned = continued.clone();
    let retained = owner.clone();
    cx.update(|app| {
        app.spawn(async move |cx| {
            let original_generation = retained
                .borrow()
                .test_thread_creation_disposed_generation()
                .unwrap();
            let cancelled = beryl_home_store::CommandCancellation::new();
            cancelled.cancel();
            let refusal = RunningProcessOwner::test_continue_retired_running_home_recovery_with(
                &retained, cancelled, cx,
            )
            .await;
            assert!(refusal.unwrap_err().contains("continuation was cancelled"));
            assert_eq!(
                retained.borrow().test_thread_creation_disposed_generation(),
                Some(original_generation)
            );
            let driver = retained
                .borrow_mut()
                .test_hold_running_home_recovery_driver()
                .unwrap();
            let refusal =
                RunningProcessOwner::test_continue_retired_running_home_recovery(&retained, cx)
                    .await;
            assert!(refusal.unwrap_err().contains("already being driven"));
            assert_eq!(
                retained.borrow().test_thread_creation_disposed_generation(),
                Some(original_generation)
            );
            drop(driver);
            *returned.borrow_mut() = Some(
                RunningProcessOwner::test_continue_retired_running_home_recovery(&retained, cx)
                    .await,
            );
        })
        .detach()
    });
    wait(
        cx,
        |_| continued.borrow().is_some(),
        "exact original creation target continuation did not return",
    );
    continued.borrow_mut().take().unwrap().unwrap();
    assert_ne!(
        selection(window, cx).binding().home_generation(),
        fresh_close.binding().home_generation()
    );
    assert_eq!(window.window_id(), native);
    assert_eq!(cx.windows().len(), 1);
    assert_original_committed_target(&owner, window, target.lock().unwrap().as_ref().unwrap(), cx);
    resource_wakeups::verify_replacement(window, restored_transcript, cx);
    drop(original_transcript);
    dispose_recovered(owner, window, cx);
    directory.close().unwrap();
}
