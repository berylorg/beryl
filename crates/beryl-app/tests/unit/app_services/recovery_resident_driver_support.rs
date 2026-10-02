use crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters;
use crate::main_window::*;
use crate::running_owner::{ResidentPreparationKey, RunningShutdownDrafts};
use crate::startup_owner::RunningExitRequest;
use beryl_home_store::CommandCancellation;
use gpui::{AsyncApp, Entity, WindowHandle};

#[test]
fn native_recovery_driver_attaches_resident_after_abandoned_wait() {
    resident_run(ResidentScenario::DrivenAttachment);
}

#[test]
fn native_recovery_driver_preserves_adoption_after_appearance_refusal() {
    resident_run(ResidentScenario::DrivenAppearanceRefusal);
}

#[test]
fn native_recovery_driver_cancels_ready_resident_attachment() {
    resident_run(ResidentScenario::DrivenCancelledAttachment);
}

#[test]
fn native_recovery_driver_cancels_while_waiting_for_resident() {
    resident_run(ResidentScenario::DrivenPendingCancellation);
}

#[test]
fn native_recovery_driver_refuses_stale_resident_attachment() {
    resident_run(ResidentScenario::DrivenStaleAttachment);
}

#[test]
fn native_recovery_driver_retains_resident_after_capacity_refusal() {
    resident_run(ResidentScenario::DrivenCapacityAttachment);
}

pub(super) async fn interrupt_pending(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &RunningExitRequest,
    admit: impl FnOnce(&mut gpui::App) -> Result<ResidentPreparationKey, String>,
    window: WindowHandle<MainWindowShellRoot>,
    appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
    adapters: &mut Option<PreparedComposerRecoveryAdapters>,
    cancel: bool,
    cx: &mut AsyncApp,
) -> ResidentPreparationKey {
    use std::future::Future;
    let mut preparation = None;
    let mut configurator: Option<MainWindowConversationComposerConfigurator> =
        Some(Box::new(resident_fixture::configure));
    let cancellation = CommandCancellation::new();
    let foreign = request.test_foreign();
    for pre_cancelled in [false, true] {
        let token = CommandCancellation::new();
        if pre_cancelled {
            token.cancel();
        }
        let error = RunningProcessOwner::prepare_and_attach_interrupted_exit_resident_window(
            owner,
            if pre_cancelled { request } else { &foreign },
            &mut preparation,
            |_| panic!("refused driver admitted preparation"),
            window,
            appearance,
            adapters,
            &mut configurator,
            |_, _| panic!("refused driver reached attachment"),
            token,
            cx,
        )
        .await
        .unwrap_err();
        assert!(error.contains(if pre_cancelled {
            "cancelled"
        } else {
            "request changed"
        }));
        assert!(preparation.is_none());
    }
    assert_eq!(
        RunningProcessOwner::prepare_and_attach_interrupted_exit_resident_window(
            owner,
            request,
            &mut preparation,
            |_| Err("injected admission refusal".into()),
            window,
            appearance,
            adapters,
            &mut configurator,
            |_, _| panic!("failed admission reached attachment"),
            CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap_err(),
        "injected admission refusal"
    );
    assert!(preparation.is_none());
    owner
        .borrow()
        .interrupted_exit_services_result(request)
        .unwrap();
    let mut competing_cx = cx.clone();
    let mut driver = Box::pin(
        RunningProcessOwner::prepare_and_attach_interrupted_exit_resident_window(
            owner,
            request,
            &mut preparation,
            admit,
            window,
            appearance,
            adapters,
            &mut configurator,
            |_, _| panic!("pending preparation reached attachment"),
            cancellation.clone(),
            cx,
        ),
    );
    std::future::poll_fn(|cx| {
        assert!(driver.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let mut competing_preparation = None;
    let mut competing_adapters = None;
    let mut competing_configurator: Option<MainWindowConversationComposerConfigurator> =
        Some(Box::new(resident_fixture::configure));
    assert!(
        RunningProcessOwner::prepare_and_attach_interrupted_exit_resident_window(
            owner,
            request,
            &mut competing_preparation,
            |_| panic!("competing attachment admitted preparation"),
            window,
            appearance,
            &mut competing_adapters,
            &mut competing_configurator,
            |_, _| panic!("competing attachment consumed current source"),
            CommandCancellation::new(),
            &mut competing_cx,
        )
        .await
        .unwrap_err()
        .contains("already being driven")
    );
    assert!(competing_preparation.is_none());
    assert!(competing_adapters.is_none());
    assert!(competing_configurator.is_some());
    assert!(
        RunningProcessOwner::await_interrupted_exit_completion(
            owner,
            request,
            CommandCancellation::new(),
            &mut competing_cx,
        )
        .await
        .unwrap_err()
        .contains("already being driven")
    );
    if cancel {
        cancellation.cancel();
        assert!(driver.await.unwrap_err().contains("cancelled"));
    } else {
        drop(driver);
    }
    assert!(adapters.is_some() && configurator.is_some());
    assert!(
        owner
            .borrow()
            .interrupted_exit_services_result(request)
            .is_err()
    );
    assert!(!RunningProcessOwner::finish_exit(owner, request));
    preparation.expect("abandoned or cancelled waiting lost the admitted key")
}

pub(super) async fn drain(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &RunningExitRequest,
    key: &ResidentPreparationKey,
    must_wait: bool,
    cx: &mut AsyncApp,
) -> Result<MainWindowComposerCandidateSource, (MainWindowComposerRetiredClose, String)> {
    use std::future::Future;
    let mut preparation = None;
    assert_eq!(
        RunningProcessOwner::cancel_and_drain_interrupted_exit_resident(
            owner,
            &mut preparation,
            cx
        )
        .await
        .err()
        .unwrap(),
        "No resident preparation key"
    );
    preparation = Some(key.clone());
    let mut driver = Box::pin(
        RunningProcessOwner::cancel_and_drain_interrupted_exit_resident(
            owner,
            &mut preparation,
            cx,
        ),
    );
    let returned = std::future::poll_fn(|cx| {
        std::task::Poll::Ready(match driver.as_mut().poll(cx) {
            std::task::Poll::Ready(result) => Some(result.unwrap()),
            std::task::Poll::Pending => None,
        })
    })
    .await;
    drop(driver);
    if must_wait {
        assert!(
            returned.is_none(),
            "ready preparation skipped pending cleanup coverage"
        );
    }
    let returned = if let Some(returned) = returned {
        returned
    } else {
        assert!(preparation.is_some());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        owner.borrow_mut().test_set_resident_cleanup_failure(true);
        assert_eq!(
            RunningProcessOwner::cancel_and_drain_interrupted_exit_resident(
                owner,
                &mut preparation,
                cx
            )
            .await
            .err()
            .unwrap(),
            "injected resident cleanup failure"
        );
        assert!(preparation.is_some());
        assert!(
            owner
                .borrow_mut()
                .take_cancelled_resident_preparation(key)
                .is_none()
        );
        owner.borrow_mut().test_set_resident_cleanup_failure(false);
        RunningProcessOwner::cancel_and_drain_interrupted_exit_resident(owner, &mut preparation, cx)
            .await
            .unwrap()
    };
    assert!(preparation.is_none());
    assert!(!RunningProcessOwner::finish_exit(owner, request));
    preparation = Some(key.clone());
    assert_eq!(
        RunningProcessOwner::cancel_and_drain_interrupted_exit_resident(
            owner,
            &mut preparation,
            cx
        )
        .await
        .err()
        .unwrap(),
        "No resident preparation"
    );
    assert!(preparation.is_some());
    returned
}

pub(super) async fn attempt(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &RunningExitRequest,
    key: &ResidentPreparationKey,
    window: WindowHandle<MainWindowShellRoot>,
    appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
    previous_appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
    resident: &Entity<MainWindowConversationComposer>,
    mount: &Entity<MainWindowConversationComposerMount>,
    drafts: &Rc<RefCell<RunningShutdownDrafts>>,
    close: MainWindowConversationComposerCloseTicket,
    adapters: &mut Option<PreparedComposerRecoveryAdapters>,
    mut current: gpui_text_input::RangePrepublicationCurrent,
    scenario: ResidentScenario,
    cx: &mut AsyncApp,
) -> bool {
    let mut preparation = Some(key.clone());
    let layout = resident
        .read_with(cx, |resident, app| {
            resident.gpui_input().read(app).resident_layout_snapshot()
        })
        .unwrap();
    let mut configurator: Option<MainWindowConversationComposerConfigurator> =
        Some(Box::new(move |selection| {
            if scenario == ResidentScenario::DrivenAppearanceRefusal {
                resident_fixture::configure_current(selection, &layout)
            } else {
                resident_fixture::configure(selection)
            }
        }));
    let foreign = request.test_foreign();
    assert!(
        RunningProcessOwner::prepare_and_attach_interrupted_exit_resident_window(
            owner,
            &foreign,
            &mut preparation,
            |_| panic!("existing preparation was readmitted"),
            window,
            appearance,
            adapters,
            &mut configurator,
            |_, _| panic!("foreign request reached attachment"),
            CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap_err()
        .contains("request changed")
    );
    let cancellation = CommandCancellation::new();
    if scenario == ResidentScenario::DrivenCancelledAttachment {
        cancellation.cancel();
    }
    if scenario == ResidentScenario::DrivenStaleAttachment {
        owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(&foreign);
    }
    if scenario == ResidentScenario::DrivenCapacityAttachment {
        current.available_capacity = gpui_text_input::RangeSurfaceCharge { bytes: 0, items: 0 };
    }
    let (input, focus) = window
        .update(cx, |root, window, app| {
            let input = resident.read(app).gpui_input();
            root.notice_safe_focus(app).focus(window);
            (input, window.focused(app))
        })
        .unwrap();
    let result = RunningProcessOwner::prepare_and_attach_interrupted_exit_resident_window(
        owner,
        request,
        &mut preparation,
        |_| panic!("existing preparation was readmitted"),
        window,
        if scenario == ResidentScenario::DrivenAppearanceRefusal {
            previous_appearance
        } else {
            appearance
        },
        adapters,
        &mut configurator,
        |_, _| Ok(current),
        cancellation,
        cx,
    )
    .await;
    let attached = matches!(
        scenario,
        ResidentScenario::DrivenAttachment | ResidentScenario::DrivenAppearanceRefusal
    );
    assert_eq!(preparation.is_none(), attached);
    let result = if scenario == ResidentScenario::DrivenAppearanceRefusal {
        let error = result.unwrap_err();
        assert!(error.contains("appearance"), "{error}");
        assert!(adapters.is_none() && configurator.is_none());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        cx.update(|app| {
            window
                .update(app, |root, native, app| {
                    assert_eq!(
                        native.focused(app),
                        focus,
                        "appearance refusal changed focus"
                    );
                    assert_eq!(root.test_exit_presentation().0, "Exiting…");
                    assert!(resident.read(app).recovery_snapshot().is_none());
                    assert!(!resident.read(app).gpui_input().read(app).is_enabled());
                })
                .unwrap();
            owner
                .borrow_mut()
                .bind_interrupted_exit_appearance(request, window, appearance, app)
                .unwrap();
        })
        .unwrap();
        None
    } else {
        Some(result)
    };
    window
        .update(cx, |root, window, app| {
            let expected_focus = if scenario == ResidentScenario::DrivenAppearanceRefusal {
                Some(root.notice_safe_focus(app))
            } else {
                focus
            };
            assert_eq!(window.focused(app), expected_focus);
            assert_eq!(resident.read(app).gpui_input(), input);
            assert!(!input.read(app).is_enabled());
            assert_eq!(mount.read(app).contribution().as_ref(), Some(resident));
            if attached {
                let fresh_close = owner
                    .borrow()
                    .test_captured_recovery_ticket(resident.entity_id())
                    .unwrap();
                if let Some(result) = result {
                    let (returned_close, record) = result.unwrap();
                    assert_eq!(returned_close, fresh_close);
                    assert_eq!(
                        record.selected_thread(),
                        Some(resident.read(app).selection_identity().claim())
                    );
                }
                assert_ne!(fresh_close, close);
                assert_eq!(
                    owner
                        .borrow()
                        .test_captured_recovery_ticket(resident.entity_id()),
                    Some(fresh_close)
                );
                assert!(resident.read(app).recovery_snapshot().is_none());
                assert!(adapters.is_none() && configurator.is_none());
                assert!(!drafts.borrow().test_recovery_ready());
                use crate::theme_runtime::AppearancePublicationTarget;
                assert_eq!(appearance.read(app).target().snapshot().count, 1);
                assert_eq!(root.test_exit_presentation().0, "Exiting…");
                owner
                    .borrow()
                    .interrupted_exit_services_result(request)
                    .unwrap();
                assert!(
                    owner
                        .borrow()
                        .interrupted_exit_resident_result(key)
                        .is_err()
                );
            } else {
                assert!(result.unwrap().is_err());
                assert!(adapters.is_some() && configurator.is_some());
                assert!(root.test_shell_construction_retired());
                assert_eq!(
                    owner
                        .borrow()
                        .test_captured_recovery_ticket(resident.entity_id()),
                    Some(close)
                );
                assert_eq!(
                    resident
                        .read(app)
                        .recovery_snapshot()
                        .unwrap()
                        .close_ticket(),
                    close
                );
            }
            assert!(!RunningProcessOwner::finish_exit(owner, request));
        })
        .unwrap();
    if attached {
        assert!(
            RunningProcessOwner::prepare_and_attach_interrupted_exit_resident_window(
                owner,
                request,
                &mut Some(key.clone()),
                |_| panic!("duplicate attachment was readmitted"),
                window,
                appearance,
                adapters,
                &mut configurator,
                |_, _| panic!("duplicate attachment reached current inputs"),
                CommandCancellation::new(),
                cx,
            )
            .await
            .is_err()
        );
    }
    attached
}
