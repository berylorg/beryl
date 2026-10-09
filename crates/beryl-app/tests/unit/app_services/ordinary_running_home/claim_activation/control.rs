use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Control {
    Normal,
    DropReady,
    RefusedTarget,
    RefusedPrior,
    CancelledPreparation,
}

#[test]
fn native_original_ordinary_ready_graph_drop_returns_exact_nonempty_editor_and_lease() {
    run_control(Cut::ClaimCommitted, Control::DropReady);
}

#[test]
fn native_original_ordinary_target_mount_refusal_drains_before_same_owner_continuation() {
    run_control(Cut::ClaimCommitted, Control::RefusedTarget);
}

#[test]
fn native_original_ordinary_noncommit_resident_refusal_preserves_prior_and_original_driver() {
    run_control(Cut::ClaimNoncommit, Control::RefusedPrior);
}

#[test]
fn native_original_ordinary_candidate_cancellation_retains_claim_until_exact_continuation() {
    run_control(Cut::ClaimCommitted, Control::CancelledPreparation);
}

pub(super) async fn install(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    control: Control,
    cx: &mut gpui::AsyncApp,
) {
    match control {
        Control::Normal => {}
        Control::DropReady => owner.borrow_mut().test_drop_ready_thread_creation_graph(),
        Control::RefusedTarget => window
            .update(cx, |root, _, _| {
                root.test_reject_thread_creation_recovery_mount()
            })
            .unwrap(),
        Control::RefusedPrior => owner
            .borrow_mut()
            .test_reject_ordinary_recovery_attachment_after(1),
        Control::CancelledPreparation => owner
            .borrow_mut()
            .test_cancel_recovery_after_resident_admission(),
    }
}

pub(super) async fn complete(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    control: Control,
    reached: &AtomicBool,
    failed_generation: beryl_home_store::HomeGeneration,
    cx: &mut gpui::AsyncApp,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let complete = match owner.borrow().automatic_recovery_outcome().as_deref() {
            Some(InterruptedExitRecoveryOutcome::Completed) => {
                assert!(matches!(control, Control::Normal | Control::DropReady));
                reached.load(Ordering::Acquire)
                    && owner.borrow().test_services().graph().is_some_and(|graph| {
                        graph
                            .home()
                            .health()
                            .generation()
                            .is_some_and(|generation| generation != failed_generation)
                            && graph.home().health().state()
                                == beryl_home_store::HomeHealthState::Healthy
                    })
            }
            Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => {
                assert!(
                    matches!(control, Control::RefusedTarget | Control::RefusedPrior),
                    "ordinary {control:?} recovery unavailable: {error}; {}",
                    owner.borrow().test_thread_creation_recovery_stage()
                );
                assert!(
                    error.contains("refused"),
                    "unexpected recovery failure: {error}"
                );
                true
            }
            Some(InterruptedExitRecoveryOutcome::Cancelled) => {
                assert_eq!(control, Control::CancelledPreparation);
                true
            }
            _ => false,
        };
        if complete {
            break;
        }
        if Instant::now() >= deadline {
            let diagnostic = window
                .read_with(cx, |root, _| {
                    (
                        root.test_thread_confirmation_diagnostics(),
                        root.test_running_activation_failure(),
                        root.notice_projection()
                            .map(|projection| projection.content.detail().as_str().to_owned()),
                    )
                })
                .unwrap();
            panic!(
                "original ordinary controlled claim recovery did not settle: control={control:?} fault_hook_reached={} activation={diagnostic:?} recovery_request={:?} recovery_stage={}",
                reached.load(Ordering::Acquire),
                owner
                    .borrow()
                    .test_running_home_recovery_identity()
                    .map(|request| Rc::as_ptr(&request)),
                owner.borrow().test_thread_creation_recovery_stage()
            );
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    if matches!(control, Control::Normal | Control::DropReady) {
        assert_eq!(
            owner.borrow().test_returned_claim_graphs(),
            usize::from(control == Control::DropReady)
        );
        return;
    }
    let request = owner
        .borrow()
        .test_running_home_recovery_identity()
        .unwrap();
    assert!(owner.borrow().test_services().running_selection_pending());
    assert!(
        owner
            .borrow()
            .test_services()
            .test_recovery_process_is_fenced()
    );
    window
        .update(cx, |root, _, _| assert!(!root.test_exit_command_enabled()))
        .unwrap();
    let native_handle = native(window, cx).await;
    assert!(unsafe { PostMessageW(Some(native_handle), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok() });
    cx.background_executor()
        .timer(Duration::from_millis(20))
        .await;
    assert!(!owner.borrow().exit_requested());
    assert!(owner.borrow().shutdown_status().is_none());
    if owner
        .borrow()
        .test_thread_creation_disposed_generation()
        .is_none()
    {
        RunningProcessOwner::test_settle_running_home_recovery_cancellation(owner, cx)
            .await
            .unwrap();
    }
    let disposed = owner
        .borrow()
        .test_thread_creation_disposed_generation()
        .unwrap();
    assert!(Rc::ptr_eq(
        &request,
        &owner
            .borrow()
            .test_running_home_recovery_identity()
            .unwrap()
    ));
    assert!(owner.borrow().test_services().running_selection_pending());
    assert!(
        owner
            .borrow()
            .test_services()
            .test_recovery_process_is_fenced()
    );
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let refusal = RunningProcessOwner::test_continue_retired_running_home_recovery_with(
        owner,
        cancellation,
        cx,
    )
    .await
    .unwrap_err();
    assert!(refusal.contains("continuation was cancelled"), "{refusal}");
    assert_eq!(
        owner.borrow().test_thread_creation_disposed_generation(),
        Some(disposed)
    );
    let driver = owner
        .borrow_mut()
        .test_hold_running_home_recovery_driver()
        .unwrap();
    let refusal = RunningProcessOwner::test_continue_retired_running_home_recovery(owner, cx)
        .await
        .unwrap_err();
    assert!(refusal.contains("already being driven"), "{refusal}");
    assert_eq!(
        owner.borrow().test_thread_creation_disposed_generation(),
        Some(disposed)
    );
    assert!(Rc::ptr_eq(
        &request,
        &owner
            .borrow()
            .test_running_home_recovery_identity()
            .unwrap()
    ));
    drop(driver);
    RunningProcessOwner::test_continue_retired_running_home_recovery(owner, cx)
        .await
        .unwrap();
    assert!(
        !owner
            .borrow()
            .test_services()
            .test_recovery_process_is_fenced()
    );
    assert_eq!(native(window, cx).await, native_handle);
}

pub(super) async fn reject_stale_generation(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    original: beryl_home_store::HomeGeneration,
    cx: &mut gpui::AsyncApp,
) {
    let fresh = owner.borrow().test_ordinary_command_status().3;
    assert_ne!(fresh, Some(original));
    let request = owner.borrow().test_running_home_recovery_identity();
    cx.update(|app| {
        RunningProcessOwner::test_observe_running_home_failure_generation(owner, original, app)
    })
    .unwrap();
    assert_eq!(owner.borrow().test_ordinary_command_status().3, fresh);
    match (
        request,
        owner.borrow().test_running_home_recovery_identity(),
    ) {
        (Some(old), Some(current)) => assert!(Rc::ptr_eq(&old, &current)),
        (None, None) => {}
        _ => panic!("stale generation replaced the original ordinary recovery request"),
    }
    assert!(!owner.borrow().test_services().running_selection_pending());
}
