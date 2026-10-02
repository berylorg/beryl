use super::*;

#[test]
fn native_mounted_pre_native_reconciliation_rejects_stale_proof_and_requires_fresh_close() {
    run_pre_native(2, None);
}

#[test]
fn native_mounted_pre_native_restoration_failure_hands_original_and_inverse_to_failed_home() {
    run_pre_native(2, Some(FaultPoint::BeforeCommit));
}

#[test]
fn native_mounted_pre_native_reported_failed_inverse_recovers_without_repeating_removal() {
    run_pre_native(2, Some(FaultPoint::AfterPersist));
}

#[test]
fn native_mounted_final_pre_native_cancellation_preserves_exiting_barrier_until_reopened() {
    run_pre_native(1, None);
}

fn run_pre_native(count: u8, restoration_fault: Option<FaultPoint>) {
    run_mounted_with_faults(
        count,
        1,
        move |owner, faults, cx| {
            Box::pin(async move {
                let invoking = windows(&owner)[0];
                let hwnd = native(invoking, cx).await;
                let resident = composer(invoking, cx).await;
                let original = snapshot(&owner);
                let generation = owner.borrow().test_ordinary_command_status().3;
                let selection = cx
                    .update(|app| resident.read(app).selection_identity())
                    .unwrap();
                let widget_selection = cx
                    .update(|app| {
                        resident
                            .read(app)
                            .gpui_input()
                            .read(app)
                            .surface()
                            .unwrap()
                            .selection()
                    })
                    .unwrap();
                let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let worker_entered = entered.clone();
                let worker_release = release.clone();
                let inverse_faults = faults.clone();
                owner
                    .borrow_mut()
                    .test_before_next_ordinary_command_admission(move |_| {
                        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
                        worker_entered.store(true, Ordering::SeqCst);
                        let deadline = Instant::now() + Duration::from_secs(10);
                        while !worker_release.load(Ordering::SeqCst) {
                            assert!(
                                Instant::now() < deadline,
                                "pre-native proof inspection did not complete"
                            );
                            std::thread::sleep(Duration::from_millis(1));
                        }
                    });
                if let Some(point) = restoration_fault {
                    owner
                        .borrow_mut()
                        .test_before_next_native_close_restoration(move |_| {
                            inverse_faults.fail_next(point)
                        });
                }
                post_close(hwnd);
                wait_until(
                    cx,
                    || entered.load(Ordering::SeqCst),
                    "exact pre-native publication admission",
                )
                .await;
                let identity = cx
                    .update(|app| {
                        let retained = owner.borrow();
                        let shell = retained
                            .test_process()
                            .windows
                            .shells()
                            .iter()
                            .find(|shell| shell.window() == invoking)
                            .unwrap();
                        let identity = shell.test_pre_native_close_identity().unwrap();
                        assert!(shell.test_nonfinal_native_close_outcome().is_none());
                        assert!(
                            retained
                                .test_require_pre_native_close(&identity, app)
                                .is_ok()
                        );
                        assert!(
                            retained
                                .test_require_pre_native_close(&Rc::new(()), app)
                                .is_err()
                        );
                        if count == 1 {
                            assert_eq!(
                                invoking.read(app).unwrap().test_exit_presentation(),
                                (
                                    "Exiting…",
                                    "Application Exit is waiting for active work and durable state."
                                )
                            );
                        }
                        identity
                    })
                    .unwrap();
                release.store(true, Ordering::SeqCst);
                wait_until(
                    cx,
                    || {
                        let retained = owner.borrow();
                        let status = retained.test_ordinary_command_status();
                        !status.1
                            && !status.2
                            && if restoration_fault.is_none() {
                                retained.test_cancelled_ordinary_close_outcomes()
                                    == Some((Some(true), Some(true)))
                            } else {
                                status.3.is_some() && status.3 != generation
                            }
                    },
                    "pre-native cancellation and coherent reopening",
                )
                .await;
                assert!(unsafe { IsWindow(Some(hwnd)).as_bool() });
                assert_eq!(
                    composer(invoking, cx).await.entity_id(),
                    resident.entity_id()
                );
                cx.update(|app| {
                    let retained = owner.borrow();
                    assert!(
                        retained
                            .test_require_pre_native_close(&identity, app)
                            .is_err()
                    );
                    let shell = retained
                        .test_process()
                        .windows
                        .shells()
                        .iter()
                        .find(|shell| shell.window() == invoking)
                        .unwrap();
                    assert!(shell.test_pre_native_close_identity().is_none());
                    assert!(shell.test_nonfinal_native_close_outcome().is_none());
                    assert_eq!(
                        resident
                            .read(app)
                            .gpui_input()
                            .read(app)
                            .surface()
                            .unwrap()
                            .selection(),
                        widget_selection
                    );
                    if restoration_fault.is_none() {
                        let fresh = resident.read(app).selection_identity();
                        assert_eq!(fresh.binding(), selection.binding());
                        assert_eq!(fresh.claim().generation(), selection.claim().generation());
                        assert_ne!(fresh.claim().revision(), selection.claim().revision());
                        assert_eq!(retained.test_ordinary_command_status().3, generation);
                    }
                })
                .unwrap();
                let restored = snapshot(&owner);
                assert_eq!(
                    restored.windows()[0].placement(),
                    original.windows()[0].placement()
                );
                if count > 1 {
                    assert_eq!(
                        restored.windows()[1].placement(),
                        original.windows()[1].placement()
                    );
                }
                assert_eq!(copy_all(invoking, &resident, cx).await, draft_text(1));
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                assert_eq!(snapshot(&owner), restored);
                post_close(hwnd);
                if count == 1 {
                    return;
                }
                wait_until(
                    cx,
                    || {
                        owner.borrow().test_ordinary_command_status().0 == 1
                            && !owner.borrow().exit_requested()
                    },
                    "fresh close after pre-native cancellation",
                )
                .await;
                assert!(!unsafe { IsWindow(Some(hwnd)).as_bool() });
                activate_exit(windows(&owner)[0], cx);
            })
        },
        true,
        if count == 1 { 0 } else { 1 },
    );
}
