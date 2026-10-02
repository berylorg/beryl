use super::*;
#[path = "ordinary_marker_resident_recovery.rs"]
mod marker_residents;

#[test]
fn native_mounted_unresolved_close_retains_exact_removed_member_and_protected_resident() {
    run_mounted_retained(2, 1, |owner, _, cx| {
        Box::pin(async move {
            let invoking = windows(&owner)[0];
            let resident = composer(invoking, cx).await;
            let selection = cx
                .update(|app| resident.read(app).selection_identity())
                .unwrap();
            let generation = owner.borrow().test_ordinary_command_status().3;
            owner.borrow_mut().test_fail_next_nonfinal_native_close(
                invoking,
                gpui::WindowsNativeWindowDestructionTestFault::MissingSettlement,
            );
            post_close(native(invoking, cx).await);
            wait_until(
                cx,
                || {
                    owner
                        .borrow()
                        .test_process()
                        .windows
                        .shells()
                        .iter()
                        .any(|shell| {
                            matches!(
                                shell.test_nonfinal_native_close_outcome(),
                                Some(
                                    gpui::WindowsNativeWindowDestructionOutcome::Unresolved { .. }
                                )
                            )
                        })
                },
                "unresolved exact native close custody",
            )
            .await;
            assert_eq!(
                owner.borrow().test_ordinary_command_status(),
                (2, true, false, generation)
            );
            assert!(matches!(
                owner.borrow().shutdown_session(),
                Some(crate::running_owner::RunningShutdownSession::RemovedWindow(
                    _
                ))
            ));
            let removed = snapshot(&owner);
            assert_eq!(removed.windows().len(), 1);
            assert!(
                removed
                    .windows()
                    .iter()
                    .all(|record| record.window_id() != selection.window_id())
            );
            let revision = owner
                .borrow()
                .test_services()
                .graph()
                .unwrap()
                .home()
                .home_revision()
                .unwrap();
            cx.update(|app| {
                let composer = resident.read(app);
                assert_eq!(composer.selection_identity(), selection);
                assert!(composer.test_retains_detached_native_close());
                assert!(composer.gpui_input().read(app).is_quiescent());
                assert!(!composer.test_widget_released());
            })
            .unwrap();
            activate_exit(windows(&owner)[1], cx);
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
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
            assert_eq!(
                owner.borrow().test_ordinary_command_status(),
                (2, true, false, generation)
            );
            assert_eq!(snapshot(&owner), removed);
            cx.update(|app| app.quit()).unwrap();
        })
    });
}

#[test]
fn native_mounted_surviving_close_restores_healthy_claim_editor_history_and_background_work() {
    run_native_failure(None);
}

#[test]
fn native_mounted_surviving_close_restoration_storage_failure_recovers_same_resident() {
    run_native_failure(Some(FaultPoint::BeforeCommit));
}

#[test]
fn native_mounted_surviving_close_reported_failed_restoration_is_reconciled_without_repeat() {
    run_native_failure(Some(FaultPoint::AfterPersist));
}

fn run_native_failure(restoration_fault: Option<FaultPoint>) {
    run_mounted_with_faults(
        2,
        1,
        move |owner, faults, cx| {
            Box::pin(async move {
                let invoking = windows(&owner)[0];
                let native = native(invoking, cx).await;
                let resident = composer(invoking, cx).await;
                let entity = resident.entity_id();
                let original = snapshot(&owner);
                let generation = owner.borrow().test_ordinary_command_status().3;
                let old_selection = cx
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
                let healthy_work = restoration_fault.is_none().then(|| {
                    retain_unviewed_work(&owner, beryl_model::SyndicThreadId::from_bytes([246; 16]))
                });
                let permit = owner.borrow().test_services().process.execution_permit();
                owner.borrow_mut().test_fail_next_nonfinal_native_close(
                    invoking,
                    gpui::WindowsNativeWindowDestructionTestFault::Refuse,
                );
                if let Some(point) = restoration_fault {
                    owner
                        .borrow_mut()
                        .test_before_next_native_close_restoration(move |_| {
                            faults.fail_next(point)
                        });
                }
                post_close(native);
                wait_until(
                    cx,
                    || {
                        let status = owner.borrow().test_ordinary_command_status();
                        if status.1 || status.2 || status.3.is_none() {
                            return false;
                        }
                        snapshot(&owner)
                            .windows()
                            .iter()
                            .find(|record| record.window_id() == old_selection.window_id())
                            .is_some_and(|record| {
                                record.selected_thread().unwrap().revision()
                                    != old_selection.claim().revision()
                            })
                    },
                    "settled surviving native close restoration",
                )
                .await;
                assert!(unsafe { IsWindow(Some(native)).as_bool() });
                assert_eq!(windows(&owner).len(), 2);
                assert_eq!(composer(invoking, cx).await.entity_id(), entity);
                let fresh = cx
                    .update(|app| resident.read(app).selection_identity())
                    .unwrap();
                assert_eq!(fresh.claim().thread_id(), old_selection.claim().thread_id());
                assert_eq!(
                    fresh.claim().generation(),
                    old_selection.claim().generation()
                );
                assert_ne!(fresh.claim().revision(), old_selection.claim().revision());
                assert_eq!(fresh.binding().root(), old_selection.binding().root());
                assert_eq!(fresh.binding().history(), old_selection.binding().history());
                assert_eq!(
                    cx.update(|app| resident
                        .read(app)
                        .gpui_input()
                        .read(app)
                        .surface()
                        .unwrap()
                        .selection())
                        .unwrap(),
                    widget_selection
                );
                let restored = snapshot(&owner);
                assert_eq!(
                    restored.windows()[0].placement(),
                    original.windows()[0].placement()
                );
                assert_eq!(restored.windows()[1], original.windows()[1]);
                assert_eq!(restored.windows()[0].selected_thread(), Some(fresh.claim()));
                if restoration_fault.is_none() {
                    assert_eq!(owner.borrow().test_ordinary_command_status().3, generation);
                    assert_eq!(fresh.binding(), old_selection.binding());
                    assert!(
                        observe(&owner, ProjectionCancellationToken::new(), cx)
                            .await
                            .unwrap()
                            .has_work()
                    );
                    permit.commit(|| ()).unwrap();
                } else {
                    assert_ne!(owner.borrow().test_ordinary_command_status().3, generation);
                }
                assert_eq!(copy_all(invoking, &resident, cx).await, draft_text(1));
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                assert_eq!(windows(&owner).len(), 2);
                assert!(!owner.borrow().exit_requested());
                drop(healthy_work);
                post_close(native);
                wait_until(
                    cx,
                    || {
                        owner.borrow().test_ordinary_command_status().0 == 1
                            && !owner.borrow().exit_requested()
                    },
                    "fresh close after native restoration",
                )
                .await;
                assert!(!unsafe { IsWindow(Some(native)).as_bool() });
                activate_exit(windows(&owner)[0], cx);
            })
        },
        true,
        1,
    );
}
