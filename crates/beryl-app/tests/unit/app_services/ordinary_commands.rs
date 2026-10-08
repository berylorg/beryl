use super::*;
use crate::main_window::{MainWindowConversationComposer, MainWindowShellRoot};
use beryl_model::WindowId;
use beryl_state::MinimalSessionBootstrap;
use gpui::{AppContext, EntityInputHandler};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::{
            Controls::TDM_CLICK_BUTTON,
            WindowsAndMessaging::{FindWindowW, IDCANCEL, IDOK, IsWindow, PostMessageW, WM_CLOSE},
        },
    },
    core::PCWSTR,
};

#[path = "ordinary_commands_support.rs"]
mod mounted_support;
#[allow(dead_code)]
#[path = "running_resident_recovery_support.rs"]
mod resident_fixture;
use mounted_support::*;
#[path = "ordinary_commands_recovery.rs"]
mod recovery;

#[test]
fn native_mounted_toolbar_exit_captures_exact_placements_with_reversed_shell_order() {
    let expected = Rc::new(RefCell::new(Vec::new()));
    let captured = expected.clone();
    let restored = run_mounted(
        2,
        1,
        move |owner, cx| {
            Box::pin(async move {
                let original = snapshot(&owner);
                for window in windows(&owner) {
                    let (id, placement) = capture(window, cx).await;
                    let claim = original
                        .windows()
                        .iter()
                        .find(|record| record.window_id() == id)
                        .unwrap()
                        .selected_thread();
                    captured.borrow_mut().push((id, placement, claim));
                }
                cx.update(|app| {
                    owner
                        .borrow_mut()
                        .test_order_shells_by_descending_window_id(app);
                    let retained = owner.borrow();
                    let process = retained.test_process();
                    let actual = process
                        .windows
                        .shells()
                        .iter()
                        .map(|shell| shell.retained_window_id(app).unwrap())
                        .collect::<Vec<_>>();
                    assert_eq!(
                        actual,
                        process
                            .windows
                            .window_ids()
                            .iter()
                            .rev()
                            .copied()
                            .collect::<Vec<_>>()
                    );
                    assert_ne!(actual, process.windows.window_ids());
                })
                .unwrap();
                activate_exit(windows(&owner)[0], cx);
            })
        },
        true,
        2,
    );
    for (id, placement, claim) in expected.borrow().iter() {
        let record = restored
            .windows()
            .iter()
            .find(|record| record.window_id() == *id)
            .unwrap();
        assert_eq!(record.placement(), placement);
        assert_eq!(record.selected_thread(), *claim);
    }
}

#[test]
fn native_mounted_final_close_records_empty_selected_restore_set() {
    run_mounted(
        1,
        0,
        |owner, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let native = native(window, cx).await;
                post_close(native);
                assert!(unsafe { IsWindow(Some(native)).as_bool() });
            })
        },
        true,
        0,
    );
}

#[test]
fn native_mounted_final_close_records_empty_threadless_restore_set() {
    run_mounted(
        0,
        0,
        |owner, cx| {
            Box::pin(async move {
                post_close(native(windows(&owner)[0], cx).await);
            })
        },
        true,
        0,
    );
}

#[test]
fn native_mounted_toolbar_exit_completes_threadless_process() {
    run_mounted(
        0,
        0,
        |owner, cx| {
            Box::pin(async move {
                activate_exit(windows(&owner)[0], cx);
            })
        },
        true,
        1,
    );
}

#[test]
fn native_mounted_toolbar_exit_preserves_complete_selected_layout() {
    run_mounted(
        2,
        0,
        |owner, cx| {
            Box::pin(async move {
                let before = snapshot(&owner);
                assert_eq!(before.windows().len(), 2);
                let invoking = windows(&owner)[1];
                activate_exit(invoking, cx);
                assert_eq!(owner.borrow().test_ordinary_command_status().0, 2);
            })
        },
        true,
        2,
    );
}

#[test]
fn native_mounted_nonfinal_close_preserves_unviewed_background_work() {
    run_mounted(
        2,
        1,
        |owner, cx| {
            Box::pin(async move {
                let original = snapshot(&owner);
                let surviving = original.windows()[1].clone();
                let invoking = windows(&owner)[0];
                let closing_id = original.windows()[0].window_id();
                let work = retain_unviewed_work(
                    &owner,
                    beryl_model::SyndicThreadId::from_bytes([247; 16]),
                );
                let permit = owner.borrow().test_services().process.execution_permit();
                let native = native(invoking, cx).await;
                let removal_waiting = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let waiting = removal_waiting.clone();
                let (release, retained) = std::sync::mpsc::channel();
                owner
                    .borrow_mut()
                    .test_before_next_ordinary_session_removal(move |_| {
                        waiting.store(true, Ordering::SeqCst);
                        retained.recv_timeout(Duration::from_secs(10)).unwrap();
                    });
                post_close(native);
                wait_until(
                    cx,
                    || removal_waiting.load(Ordering::SeqCst),
                    "nonfinal removal admission",
                )
                .await;
                cx.update(|app| {
                    let root = invoking.read(app).unwrap();
                    assert!(!root.test_exit_command_enabled());
                    assert_eq!(
                        root.test_exit_presentation(),
                        (
                            "Exit",
                            "This window is waiting for its draft and durable close state."
                        )
                    );
                })
                .unwrap();
                release.send(()).unwrap();
                wait_until(
                    cx,
                    || {
                        owner.borrow().test_ordinary_command_status().0 == 1
                            && !owner.borrow().exit_requested()
                    },
                    "nonfinal native close",
                )
                .await;
                assert!(!unsafe { IsWindow(Some(native)).as_bool() });
                assert_eq!(snapshot(&owner).windows(), &[surviving]);
                assert!(
                    !snapshot(&owner)
                        .windows()
                        .iter()
                        .any(|record| record.window_id() == closing_id)
                );
                assert!(
                    observe(&owner, ProjectionCancellationToken::new(), cx)
                        .await
                        .unwrap()
                        .has_work()
                );
                permit.commit(|| ()).unwrap();
                assert!(owner.borrow().shutdown_status().is_none());
                drop(work);
                activate_exit(windows(&owner)[0], cx);
            })
        },
        true,
        1,
    );
}

#[test]
fn native_mounted_close_confirmation_coalesces_and_cancel_requires_fresh_activation() {
    cancellation(false);
}

#[test]
fn native_mounted_toolbar_confirmation_coalesces_and_cancel_requires_fresh_activation() {
    cancellation(true);
}

#[test]
fn native_mounted_confirmed_final_close_waits_for_work_and_records_empty_restore_set() {
    confirmed(false, 1);
}

#[test]
fn native_mounted_confirmed_toolbar_exit_waits_for_unviewed_work_and_preserves_complete_layout() {
    confirmed(true, 2);
}

fn confirmed(toolbar: bool, count: u8) {
    run_mounted(
        count,
        0,
        move |owner, cx| {
            Box::pin(async move {
                let invoking = windows(&owner)[0];
                let native = native(invoking, cx).await;
                let original = snapshot(&owner);
                let permit = owner.borrow().test_services().process.execution_permit();
                let work = retain_unviewed_work(
                    &owner,
                    beryl_model::SyndicThreadId::from_bytes([250; 16]),
                );
                wait_for_unviewed_catalog_source(
                    &owner,
                    beryl_model::SyndicThreadId::from_bytes([250; 16]),
                    cx,
                )
                .await;
                if toolbar {
                    activate_exit(invoking, cx);
                } else {
                    post_close(native);
                }
                let confirmation = dialog(cx).await;
                assert_eq!(snapshot(&owner), original);
                assert!(owner.borrow().shutdown_status().is_none());
                choose(confirmation, IDOK.0);
                wait_until(
                    cx,
                    || owner.borrow().shutdown_status().is_some(),
                    "confirmed mounted barrier admission",
                )
                .await;
                assert!(permit.commit(|| ()).is_err());
                assert_eq!(windows(&owner).len(), usize::from(count));
                for window in windows(&owner) {
                    cx.update(|app| {
                        let root = window.read(app).unwrap();
                        assert!(!root.test_exit_command_enabled());
                        assert_eq!(
                            root.test_exit_presentation(),
                            (
                                "Exiting…",
                                "Application Exit is waiting for active work and durable state."
                            )
                        );
                    })
                    .unwrap();
                    activate_exit(window, cx);
                }
                assert!(unsafe { IsWindow(Some(native)).as_bool() });
                post_close(native);
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                assert_eq!(windows(&owner).len(), usize::from(count));
                drop(work);
            })
        },
        true,
        if toolbar { usize::from(count) } else { 0 },
    );
}

fn cancellation(toolbar: bool) {
    run_mounted(
        1,
        1,
        move |owner, cx| {
            Box::pin(async move {
                let invoking = windows(&owner)[0];
                let original = snapshot(&owner);
                let resident = composer(invoking, cx).await;
                let identity = resident.entity_id();
                let selected = cx
                    .update(|app| resident.read(app).selection_identity())
                    .unwrap();
                let permit = owner.borrow().test_services().process.execution_permit();
                let work = retain_unviewed_work(
                    &owner,
                    beryl_model::SyndicThreadId::from_bytes([248; 16]),
                );
                wait_for_unviewed_catalog_source(
                    &owner,
                    beryl_model::SyndicThreadId::from_bytes([248; 16]),
                    cx,
                )
                .await;
                let native = native(invoking, cx).await;
                for _ in 0..2 {
                    if toolbar {
                        activate_exit(invoking, cx);
                    } else {
                        post_close(native);
                    }
                }
                let confirmation = dialog(cx).await;
                assert!(unsafe { IsWindow(Some(native)).as_bool() });
                assert!(owner.borrow().shutdown_status().is_none());
                permit.commit(|| ()).unwrap();
                assert_eq!(snapshot(&owner), original);
                if toolbar {
                    activate_exit(invoking, cx);
                } else {
                    post_close(native);
                }
                cx.background_executor()
                    .timer(Duration::from_millis(60))
                    .await;
                assert_eq!(dialog(cx).await, confirmation);
                choose(confirmation, IDCANCEL.0);
                wait_until(
                    cx,
                    || !owner.borrow().exit_requested(),
                    "cancelled command consumer",
                )
                .await;
                assert_eq!(snapshot(&owner), original);
                assert_eq!(composer(invoking, cx).await.entity_id(), identity);
                assert_eq!(
                    cx.update(|app| resident.read(app).selection_identity())
                        .unwrap(),
                    selected
                );
                assert_eq!(copy_all(invoking, &resident, cx).await, draft_text(1));
                assert!(
                    observe(&owner, ProjectionCancellationToken::new(), cx)
                        .await
                        .unwrap()
                        .has_work()
                );
                permit.commit(|| ()).unwrap();
                drop(work);
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                assert!(unsafe { IsWindow(Some(native)).as_bool() });
                assert!(!owner.borrow().exit_requested());
                if toolbar {
                    activate_exit(invoking, cx);
                } else {
                    post_close(native);
                }
            })
        },
        true,
        if toolbar { 1 } else { 0 },
    );
}

#[test]
fn native_mounted_feature_gates_reject_delivery_until_fresh_activation() {
    run_mounted(
        1,
        0,
        |owner, cx| {
            Box::pin(async move {
                let invoking = windows(&owner)[0];
                let native = native(invoking, cx).await;
                let original = snapshot(&owner);
                for (gate, reason) in [
                    (
                        startup_owner::RunningExitGate::SettingsReconciliation,
                        "Application Exit is waiting for Settings reconciliation.",
                    ),
                    (
                        startup_owner::RunningExitGate::HomeUnavailable,
                        "The Beryl home store is unavailable. See the Beryl-home failure notice for automatic recovery.",
                    ),
                ] {
                    owner.borrow().set_exit_gate(gate, true);
                    cx.update(|app| {
                        let root = invoking.read(app).unwrap();
                        assert!(!root.test_exit_command_enabled());
                        assert_eq!(root.test_exit_presentation().1, reason);
                    })
                    .unwrap();
                    activate_exit(invoking, cx);
                    post_close(native);
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    assert!(!owner.borrow().exit_requested());
                    assert_eq!(snapshot(&owner), original);
                    assert!(unsafe { IsWindow(Some(native)).as_bool() });
                    owner.borrow().set_exit_gate(gate, false);
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    assert!(!owner.borrow().exit_requested());
                    assert_eq!(snapshot(&owner), original);
                }
                activate_exit(invoking, cx);
            })
        },
        true,
        1,
    );
}

#[test]
fn native_mounted_window_created_after_startup_routes_native_close() {
    run_mounted(
        1,
        0,
        |owner, cx| {
            Box::pin(async move {
                let original = snapshot(&owner);
                let source = windows(&owner)[0];
                source
                    .update(cx, |root, _, cx| {
                        assert!(
                            root.new_window_disabled_reason(cx).is_none(),
                            "{:?}",
                            root.new_window_disabled_reason(cx)
                        );
                        root.test_activate_new_window_command(cx)
                    })
                    .unwrap();
                wait_for_created_window(&owner, source, 2, cx).await;
                let created = windows(&owner)
                    .into_iter()
                    .find(|window| *window != source)
                    .unwrap();
                let created_native = native(created, cx).await;
                assert_eq!(snapshot(&owner).windows().len(), 2);
                let created_id = created
                    .read_with(cx, |root, _| root.controller().unwrap().window_id())
                    .unwrap();
                assert!(
                    snapshot(&owner)
                        .windows()
                        .iter()
                        .any(|record| record.window_id() == created_id)
                );
                post_close(created_native);
                wait_for_created_close(&owner, created, 1, cx).await;
                assert!(!unsafe { IsWindow(Some(created_native)).as_bool() });
                assert_eq!(snapshot(&owner).windows(), original.windows());
                activate_exit(source, cx);
            })
        },
        true,
        1,
    );
}

#[test]
fn native_mounted_exit_from_window_created_after_startup_preserves_complete_layout() {
    let expected = Rc::new(RefCell::new(None));
    let record = expected.clone();
    let restored = run_mounted(
        1,
        0,
        move |owner, cx| {
            Box::pin(async move {
                let source = windows(&owner)[0];
                source
                    .update(cx, |root, _, cx| {
                        assert!(
                            root.new_window_disabled_reason(cx).is_none(),
                            "{:?}",
                            root.new_window_disabled_reason(cx)
                        );
                        root.test_activate_new_window_command(cx)
                    })
                    .unwrap();
                wait_for_created_window(&owner, source, 2, cx).await;
                let created = windows(&owner)
                    .into_iter()
                    .find(|window| *window != source)
                    .unwrap();
                assert_eq!(snapshot(&owner).windows().len(), 2);
                let native = native(created, cx).await;
                let (id, placement) = capture(created, cx).await;
                let claim = snapshot(&owner)
                    .windows()
                    .iter()
                    .find(|window| window.window_id() == id)
                    .unwrap()
                    .selected_thread()
                    .unwrap();
                *record.borrow_mut() = Some((id, placement, claim, native));
                cx.update(|app| assert!(created.read(app).unwrap().test_exit_command_enabled()))
                    .unwrap();
                activate_exit(created, cx);
            })
        },
        true,
        2,
    );
    let (id, placement, claim, native) = expected.borrow_mut().take().unwrap();
    let record = restored
        .windows()
        .iter()
        .find(|record| record.window_id() == id)
        .unwrap();
    assert_eq!(record.placement(), &placement);
    assert_eq!(record.selected_thread(), Some(claim));
    assert!(!unsafe { IsWindow(Some(native)).as_bool() });
}
