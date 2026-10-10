use super::*;
use crate::main_window::{MainWindowConversationComposer, MainWindowShellRoot};
use crate::running_owner::{InterruptedExitRecoveryOutcome, RunningProcessOwner};
use crate::theme_runtime::AppearancePublicationTarget;
use beryl_model::WindowId;
use beryl_state::MinimalSessionBootstrap;
use gpui::{AppContext, EntityInputHandler};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::{
            Controls::TDM_CLICK_BUTTON,
            WindowsAndMessaging::{
                FindWindowW, GetForegroundWindow, IDCANCEL, IDOK, IsWindow, PostMessageW, WM_CLOSE,
            },
        },
    },
    core::PCWSTR,
};

#[allow(dead_code)]
#[path = "ordinary_commands_support.rs"]
mod mounted_support;
#[allow(dead_code)]
#[path = "running_resident_recovery_support.rs"]
mod resident_fixture;
use mounted_support::*;
#[path = "ordinary_running_home_cancellation.rs"]
mod cancellation;
#[path = "ordinary_running_home/claim_activation.rs"]
mod claim_activation;
#[path = "../model_menu_native.rs"]
mod model_menu_native;
#[path = "ordinary_running_home_native_support.rs"]
mod native_support;
#[path = "ordinary_running_home_preservation.rs"]
mod preservation;

#[derive(Clone, Copy, PartialEq)]
enum Scenario {
    Observed,
    Duplicate,
    RetryReopen,
    LifecycleFence,
    StaleNotice,
    UncertainEnrollment,
}

#[test]
fn native_ordinary_failed_home_preserves_two_unsaved_residents_without_exit() {
    run_recovery(2, true, Scenario::Observed);
}

#[test]
fn native_ordinary_failed_home_preserves_threadless_window_without_exit() {
    run_recovery(0, false, Scenario::Observed);
}

#[test]
fn native_ordinary_failed_home_duplicate_notices_coalesce_without_exit() {
    run_recovery(2, false, Scenario::Duplicate);
}

#[test]
fn native_ordinary_failed_home_reopen_failure_retries_same_custody_without_exit() {
    run_recovery(2, false, Scenario::RetryReopen);
}

#[test]
fn native_ordinary_failed_home_fence_refuses_exit_and_native_close() {
    run_recovery(2, false, Scenario::LifecycleFence);
}

#[test]
fn native_ordinary_failed_home_old_generation_notice_cannot_replace_current_custody() {
    run_recovery(2, false, Scenario::StaleNotice);
}

#[test]
fn native_ordinary_failed_home_settles_original_uncertain_process_enrollment() {
    run_recovery(1, false, Scenario::UncertainEnrollment);
}

fn run_recovery(count: u8, dirty: bool, scenario: Scenario) {
    run_mounted_with_faults(
        count,
        0,
        move |owner, faults, cx| {
            Box::pin(async move {
                let original_windows = windows(&owner);
                let mut residents = Vec::new();
                if count != 0 {
                    for (index, window) in original_windows.iter().copied().enumerate() {
                        residents.push(
                            preservation::capture_resident(&owner, window, index, dirty, cx).await,
                        );
                    }
                }
                let mut natives = Vec::new();
                let mut placements = Vec::new();
                let mut logical_focus = Vec::new();
                for window in &original_windows {
                    natives.push(native(*window, cx).await);
                    placements.push(capture(*window, cx).await);
                    logical_focus.push(
                        window
                            .update(cx, |_, window, app| window.focused(app))
                            .unwrap(),
                    );
                }
                let native_focus = unsafe { GetForegroundWindow() };
                if scenario == Scenario::UncertainEnrollment {
                    owner
                        .borrow_mut()
                        .test_services_mut()
                        .graph
                        .as_mut()
                        .unwrap()
                        .handoff
                        .as_mut()
                        .unwrap()
                        .shutdown()
                        .unwrap();
                    let retained = owner.borrow();
                    let services = retained.test_services();
                    let graph = services.graph().unwrap();
                    super::super::recovery_support::install_uncertain_enrollment(
                        services,
                        graph.home(),
                        graph.state(),
                        graph.syndic(),
                        &faults,
                    );
                    assert_eq!(services.enrollments.pending_count(), 1);
                    assert_eq!(graph.home().pending_reconciliations().len(), 1);
                    assert_eq!(graph.home().health().state(), HomeHealthState::Healthy);
                }
                let original_session = snapshot(&owner);
                let (home_id, generation, session_revision, old_permit) = {
                    let retained = owner.borrow();
                    let services = retained.test_services();
                    let graph = services.graph().unwrap();
                    (
                        graph.home().home_id(),
                        graph.home().health().generation().unwrap(),
                        graph.state().session().revision(graph.home()).unwrap(),
                        services.process.execution_permit(),
                    )
                };
                assert!(!owner.borrow().exit_requested());
                assert!(owner.borrow().shutdown_status().is_none());
                assert!(owner.borrow().shutdown_session().is_none());
                assert!(owner.borrow().automatic_recovery_outcome().is_none());

                let home = owner
                    .borrow()
                    .test_services()
                    .graph()
                    .unwrap()
                    .home()
                    .service_reference();
                let read_faults = faults.clone();
                cx.background_executor()
                    .spawn(async move {
                        read_faults.fail_next(FaultPoint::BeforeReadConfirmation);
                        assert!(home.home_revision().is_err());
                        assert_eq!(home.health().state(), HomeHealthState::Failed);
                    })
                    .await;
                if scenario == Scenario::RetryReopen {
                    faults.fail_next(FaultPoint::BeforeReopen);
                }
                let mut blocked_reopen =
                    matches!(scenario, Scenario::LifecycleFence | Scenario::RetryReopen)
                        .then(|| faults.block_next(FaultPoint::BeforeReopen));
                let mut admitted_identity = None;
                if scenario != Scenario::Observed {
                    cx.update(|app| {
                        RunningProcessOwner::test_observe_running_home_failure(&owner, app);
                        let identity = owner
                            .borrow()
                            .test_running_home_recovery_identity()
                            .unwrap();
                        RunningProcessOwner::test_observe_running_home_failure(&owner, app);
                        assert!(Rc::ptr_eq(
                            &identity,
                            &owner
                                .borrow()
                                .test_running_home_recovery_identity()
                                .unwrap(),
                        ));
                        admitted_identity = Some(identity);
                        if scenario == Scenario::LifecycleFence {
                            let command = owner
                                .borrow()
                                .window_exit_command(original_session.windows()[0].window_id(), app)
                                .unwrap();
                            assert!(command.disabled_reason().is_some());
                            command.request_exit();
                            assert!(!owner.borrow().exit_requested());
                            assert!(
                                !original_windows[0]
                                    .read(app)
                                    .unwrap()
                                    .test_exit_command_enabled()
                            );
                        }
                    })
                    .unwrap();
                    if scenario == Scenario::LifecycleFence {
                        post_close(natives[0]);
                        let blocked = blocked_reopen.take().unwrap();
                        let blocked = cx
                            .background_executor()
                            .spawn(async move {
                                assert!(
                                    blocked.wait_until_reached(Duration::from_secs(10)),
                                    "ordinary reopening did not reach the blocked worker"
                                );
                                blocked
                            })
                            .await;
                        cx.background_executor()
                            .timer(Duration::from_millis(150))
                            .await;
                        assert!(!owner.borrow().exit_requested());
                        assert!(owner.borrow().shutdown_session().is_none());
                        assert_eq!(windows(&owner), original_windows);
                        for handle in &natives {
                            assert!(unsafe { IsWindow(Some(*handle)).as_bool() });
                        }
                        native_support::wait_for_notice(&original_windows, false, cx).await;
                        blocked.release();
                    }
                }
                if scenario == Scenario::RetryReopen {
                    let blocked = blocked_reopen.take().unwrap();
                    let blocked = cx.background_executor().spawn(async move {
                        assert!(blocked.wait_until_reached(Duration::from_secs(10)), "ordinary retry did not reach the second reopening visit after the queued failure");
                        blocked
                    }).await;
                    native_support::wait_for_notice(&original_windows, false, cx).await;
                    let retained = owner.borrow();
                    assert!(matches!(
                        retained.automatic_recovery_outcome().as_deref(),
                        Some(InterruptedExitRecoveryOutcome::Running)
                    ));
                    assert!(Rc::ptr_eq(
                        admitted_identity.as_ref().unwrap(),
                        &retained.test_running_home_recovery_identity().unwrap()
                    ));
                    assert!(!retained.exit_requested());
                    assert!(retained.shutdown_session().is_none());
                    assert!(retained.test_services_on_worker());
                    assert!(old_permit.reserve().is_err());
                    drop(retained);
                    assert_eq!(windows(&owner), original_windows);
                    blocked.release();
                }
                wait_until(
                    cx,
                    || {
                        let retained = owner.borrow();
                        assert!(
                            !retained.exit_requested(),
                            "ordinary recovery manufactured an Exit request"
                        );
                        assert!(retained.shutdown_session().is_none());
                        match retained.automatic_recovery_outcome().as_deref() {
                            Some(InterruptedExitRecoveryOutcome::Completed) => true,
                            Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => {
                                panic!("ordinary recovery unavailable: {error}")
                            }
                            Some(InterruptedExitRecoveryOutcome::Cancelled) => {
                                panic!("ordinary recovery cancelled unexpectedly")
                            }
                            _ => false,
                        }
                    },
                    "ordinary mounted failed-home recovery",
                )
                .await;
                native_support::wait_for_notice(&original_windows, true, cx).await;
                assert_eq!(windows(&owner), original_windows);
                assert_eq!(snapshot(&owner), original_session);
                assert!(old_permit.reserve().is_err());
                {
                    let retained = owner.borrow();
                    let services = retained.test_services();
                    let graph = services.graph().unwrap();
                    assert_eq!(graph.home().home_id(), home_id);
                    let fresh = graph.home().health().generation().unwrap();
                    assert_ne!(fresh, generation);
                    assert_eq!(graph.home().health().state(), HomeHealthState::Healthy);
                    assert_eq!(
                        graph.state().session().revision(graph.home()).unwrap(),
                        session_revision
                    );
                    assert_eq!(graph.cas().home_generation(), fresh);
                    assert!(graph.handoff.is_some());
                    let _ = graph.sessions();
                    let _ = graph.marker();
                    let _ = graph.activity();
                    let _ = graph.attention();
                    assert!(graph.loaded_theme.is_some());
                    if scenario == Scenario::UncertainEnrollment {
                        assert_eq!(services.enrollments.pending_count(), 0);
                        assert!(graph.home().pending_reconciliations().is_empty());
                    }
                    drop(services.process.execution_permit().reserve().unwrap());
                }
                cx.update(|app| {
                    for window in &original_windows {
                        let root = window.read(app).unwrap();
                        assert!(!root.test_notices_inert());
                        assert!(root.test_exit_command_enabled());
                        if count == 0 {
                            assert!(root.controller().unwrap().composer_mount().is_none());
                        }
                    }
                    let appearance = owner.borrow().test_process_appearance();
                    assert_ne!(
                        appearance
                            .read(app)
                            .target()
                            .snapshot()
                            .current
                            .prepared()
                            .home()
                            .home_generation(),
                        generation
                    );
                })
                .unwrap();
                let observed_native_focus = unsafe { GetForegroundWindow() };
                if observed_native_focus != native_focus {
                    eprintln!(
                        "desktop foreground transition observed: before={native_focus:?}, after={observed_native_focus:?}, before_fixture_native={}, after_fixture_native={}; desktop foreground is externally controlled and this observation does not attribute the transition to recovery",
                        natives.contains(&native_focus),
                        natives.contains(&observed_native_focus),
                    );
                }
                for (window, focus) in original_windows.iter().zip(&logical_focus) {
                    assert_eq!(
                        window
                            .update(cx, |_, window, app| window.focused(app))
                            .unwrap(),
                        *focus,
                    );
                }
                for ((window, native_handle), placement) in
                    original_windows.iter().zip(&natives).zip(&placements)
                {
                    assert!(unsafe { IsWindow(Some(*native_handle)).as_bool() });
                    assert_eq!(native(*window, cx).await, *native_handle);
                    assert_eq!(&capture(*window, cx).await, placement);
                }
                for resident in residents {
                    preservation::verify_resident(&owner, resident, cx).await;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                assert!(!owner.borrow().exit_requested());
                assert_eq!(snapshot(&owner), original_session);
                if scenario == Scenario::Duplicate {
                    let published = owner.borrow().test_ordinary_command_status().3;
                    cx.update(|app| {
                        RunningProcessOwner::test_observe_running_home_failure(&owner, app)
                    })
                    .unwrap();
                    cx.background_executor()
                        .timer(Duration::from_millis(150))
                        .await;
                    assert_eq!(owner.borrow().test_ordinary_command_status().3, published);
                    assert!(matches!(
                        owner.borrow().automatic_recovery_outcome().as_deref(),
                        Some(InterruptedExitRecoveryOutcome::Completed)
                    ));
                }
                if scenario == Scenario::StaleNotice {
                    let first_identity = admitted_identity.unwrap();
                    assert!(
                        owner
                            .borrow()
                            .test_running_home_recovery_identity()
                            .is_none()
                    );
                    let current_generation = owner
                        .borrow()
                        .test_services()
                        .graph()
                        .unwrap()
                        .home()
                        .health()
                        .generation()
                        .unwrap();
                    let home = owner
                        .borrow()
                        .test_services()
                        .graph()
                        .unwrap()
                        .home()
                        .service_reference();
                    let read_faults = faults.clone();
                    cx.background_executor()
                        .spawn(async move {
                            read_faults.fail_next(FaultPoint::BeforeReadConfirmation);
                            assert!(home.home_revision().is_err());
                            assert_eq!(home.health().state(), HomeHealthState::Failed);
                        })
                        .await;
                    cx.update(|app| {
                        RunningProcessOwner::test_observe_running_home_failure_generation(
                            &owner, generation, app,
                        );
                        assert!(
                            owner
                                .borrow()
                                .test_running_home_recovery_identity()
                                .is_none()
                        );
                        assert_eq!(
                            owner
                                .borrow()
                                .test_services()
                                .graph()
                                .unwrap()
                                .home()
                                .health()
                                .generation()
                                .unwrap(),
                            current_generation
                        );
                        assert!(matches!(
                            owner.borrow().automatic_recovery_outcome().as_deref(),
                            Some(InterruptedExitRecoveryOutcome::Completed)
                        ));
                        RunningProcessOwner::test_observe_running_home_failure_generation(
                            &owner,
                            current_generation,
                            app,
                        );
                        assert!(!Rc::ptr_eq(
                            &first_identity,
                            &owner
                                .borrow()
                                .test_running_home_recovery_identity()
                                .unwrap()
                        ));
                    })
                    .unwrap();
                    wait_until(
                        cx,
                        || {
                            let retained = owner.borrow();
                            assert!(!retained.exit_requested());
                            assert!(retained.shutdown_session().is_none());
                            match retained.automatic_recovery_outcome().as_deref() {
                                Some(InterruptedExitRecoveryOutcome::Completed) => true,
                                Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => {
                                    panic!("second ordinary recovery unavailable: {error}")
                                }
                                Some(InterruptedExitRecoveryOutcome::Cancelled) => {
                                    panic!("second ordinary recovery was cancelled")
                                }
                                _ => false,
                            }
                        },
                        "second-generation ordinary failed-home recovery",
                    )
                    .await;
                    assert_eq!(snapshot(&owner), original_session);
                    assert_eq!(windows(&owner), original_windows);
                    let retained = owner.borrow();
                    let graph = retained.test_services().graph().unwrap();
                    assert_ne!(
                        graph.home().health().generation().unwrap(),
                        current_generation
                    );
                    assert_eq!(
                        graph.state().session().revision(graph.home()).unwrap(),
                        session_revision
                    );
                    drop(retained);
                    for (window, handle) in original_windows.iter().zip(&natives) {
                        assert_eq!(native(*window, cx).await, *handle);
                    }
                }
                activate_exit(original_windows[0], cx);
                if scenario == Scenario::UncertainEnrollment {
                    native_support::confirm_fixture_exit(natives[0], cx).await;
                }
            })
        },
        true,
        usize::from(count.max(1)),
    );
}
