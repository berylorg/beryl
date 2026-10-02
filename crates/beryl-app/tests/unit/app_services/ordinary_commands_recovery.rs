use super::*;

#[path = "ordinary_commands_dirty.rs"]
mod dirty;
#[path = "ordinary_commands_native_recovery.rs"]
mod native_recovery;
#[path = "ordinary_commands_pre_native_recovery.rs"]
mod pre_native_recovery;

#[test]
fn native_mounted_healthy_removal_conflict_preserves_exact_window_and_requires_fresh_close() {
    run_mounted(
        2,
        1,
        |owner, cx| {
            Box::pin(async move {
                let invoking = windows(&owner)[0];
                let native = native(invoking, cx).await;
                let original = snapshot(&owner);
                let generation = owner.borrow().test_ordinary_command_status().3;
                let resident = composer(invoking, cx).await;
                let entity = resident.entity_id();
                let selection = cx
                    .update(|app| resident.read(app).selection_identity())
                    .unwrap();
                let settings = owner
                    .borrow()
                    .test_services()
                    .graph()
                    .unwrap()
                    .state()
                    .settings();
                let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let advanced = called.clone();
                owner
                    .borrow_mut()
                    .test_before_next_ordinary_command_admission(move |home| {
                        let mut command =
                            beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
                        command
                            .add(
                                settings.apply(
                                    settings.revision(home).unwrap(),
                                    beryl_state::ApplySettings::new(vec![
                                        beryl_state::SettingUpdate::new(
                                            beryl_state::SettingKey::DeveloperInstructions,
                                            beryl_state::ExpectedSettingRevision::Absent,
                                            beryl_state::SettingValue::developer_instructions(
                                                "ordinary close conflict",
                                            )
                                            .unwrap(),
                                        ),
                                    ])
                                    .unwrap(),
                                ),
                            )
                            .unwrap();
                        assert!(matches!(
                            home.execute(command),
                            beryl_home_store::CommandOutcome::Committed { .. }
                        ));
                        advanced.store(true, Ordering::SeqCst);
                    });
                post_close(native);
                wait_until(
                    cx,
                    || called.load(Ordering::SeqCst) && !owner.borrow().exit_requested(),
                    "healthy conflict recovery",
                )
                .await;
                assert_eq!(
                    owner.borrow().test_ordinary_command_status(),
                    (2, false, false, generation)
                );
                assert_eq!(
                    owner
                        .borrow()
                        .test_services()
                        .graph()
                        .unwrap()
                        .home()
                        .health()
                        .state(),
                    HomeHealthState::Healthy
                );
                assert!(unsafe { IsWindow(Some(native)).as_bool() });
                assert_eq!(snapshot(&owner), original);
                assert_eq!(
                    owner.borrow().test_cancelled_ordinary_close_outcomes(),
                    Some((Some(false), None))
                );
                assert_eq!(composer(invoking, cx).await.entity_id(), entity);
                assert_eq!(
                    cx.update(|app| resident.read(app).selection_identity())
                        .unwrap(),
                    selection
                );
                assert_eq!(copy_all(invoking, &resident, cx).await, draft_text(1));
                let deadline = Instant::now() + Duration::from_secs(10);
                while cx
                    .update(|app| invoking.read(app).unwrap().notice_projection().is_none())
                    .unwrap()
                {
                    assert!(
                        Instant::now() < deadline,
                        "healthy conflict notice was not delivered"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                cx.update(|app| {
                    let notice = invoking.read(app).unwrap().notice_projection().unwrap();
                    assert_eq!(notice.content.commands().count(), 0);
                })
                .unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                assert_eq!(snapshot(&owner), original);
                post_close(native);
                wait_until(
                    cx,
                    || {
                        owner.borrow().test_ordinary_command_status().0 == 1
                            && !owner.borrow().exit_requested()
                    },
                    "fresh conflict close",
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

#[test]
fn native_mounted_failure_before_draft_preparation_preserves_unremoved_window_and_requires_fresh_close()
 {
    run_mounted(
        2,
        1,
        |owner, cx| {
            Box::pin(async move {
                let invoking = windows(&owner)[0];
                let native = native(invoking, cx).await;
                let original = snapshot(&owner);
                let resident = composer(invoking, cx).await;
                let entity = resident.entity_id();
                let generation = owner.borrow().test_ordinary_command_status().3;
                let old_command = cx
                    .update(|app| {
                        owner
                            .borrow()
                            .window_exit_command(original.windows()[0].window_id(), app)
                            .unwrap()
                    })
                    .unwrap();
                let removal_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let removed = removal_started.clone();
                owner
                    .borrow_mut()
                    .test_before_next_ordinary_session_removal(move |_| {
                        removed.store(true, Ordering::SeqCst)
                    });
                owner
                    .borrow_mut()
                    .test_before_next_ordinary_draft_prepare(|home| {
                        home.inject_retained_maintenance_terminal();
                        assert!(beryl_state::BerylState::reacquire(home).is_err());
                        assert_eq!(home.health().state(), HomeHealthState::Failed);
                    });
                post_close(native);
                wait_until(
                    cx,
                    || {
                        let status = owner.borrow().test_ordinary_command_status();
                        status.3 != generation && !status.1 && !status.2
                    },
                    "unremoved draft-preparation recovery",
                )
                .await;
                assert!(!removal_started.load(Ordering::SeqCst));
                assert!(unsafe { IsWindow(Some(native)).as_bool() });
                assert_eq!(windows(&owner).len(), 2);
                assert_eq!(composer(invoking, cx).await.entity_id(), entity);
                let recovered = snapshot(&owner);
                assert_eq!(
                    recovered
                        .windows()
                        .iter()
                        .map(|record| (
                            record.window_id(),
                            record.placement(),
                            record.selected_thread().map(|claim| claim.thread_id())
                        ))
                        .collect::<Vec<_>>(),
                    original
                        .windows()
                        .iter()
                        .map(|record| (
                            record.window_id(),
                            record.placement(),
                            record.selected_thread().map(|claim| claim.thread_id())
                        ))
                        .collect::<Vec<_>>()
                );
                assert_eq!(copy_all(invoking, &resident, cx).await, draft_text(1));
                assert!(old_command.disabled_reason().is_some());
                old_command.request_close();
                old_command.request_exit();
                assert!(!owner.borrow().exit_requested());
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                assert_eq!(snapshot(&owner), recovered);
                assert!(!owner.borrow().exit_requested());
                post_close(native);
                wait_until(
                    cx,
                    || {
                        owner.borrow().test_ordinary_command_status().0 == 1
                            && !owner.borrow().exit_requested()
                    },
                    "fresh unremoved close",
                )
                .await;
                assert!(removal_started.load(Ordering::SeqCst));
                assert!(!unsafe { IsWindow(Some(native)).as_bool() });
                activate_exit(windows(&owner)[0], cx);
            })
        },
        true,
        1,
    );
}

#[test]
fn native_mounted_failed_removal_before_commit_preserves_window_and_requires_fresh_close() {
    removal(FaultPoint::BeforeCommit, 2, false);
}

#[test]
fn native_mounted_reported_failed_committed_removal_restores_same_window_before_fresh_close() {
    removal(FaultPoint::AfterPersist, 2, false);
}

#[test]
fn native_mounted_uncertain_removal_reconciles_exact_window_before_fresh_close() {
    removal(FaultPoint::AfterCommitBeforePersist, 2, false);
}

#[test]
fn native_mounted_final_reported_failed_removal_preserves_same_window_before_fresh_close() {
    removal(FaultPoint::AfterPersist, 1, false);
}

#[test]
fn native_mounted_final_uncertain_removal_recovers_same_window_before_fresh_close() {
    removal(FaultPoint::AfterCommitBeforePersist, 1, false);
}

#[test]
fn native_mounted_recovered_creation_owner_publishes_new_window_with_renewed_commands() {
    removal(FaultPoint::AfterPersist, 2, true);
}

fn removal(point: FaultPoint, count: u8, create: bool) {
    let healthy = point == FaultPoint::AfterCommitBeforePersist;
    run_mounted_with_faults(
        count,
        1,
        move |owner, faults, cx| {
            Box::pin(async move {
                let invoking = windows(&owner)[0];
                let invoking_native = native(invoking, cx).await;
                let original = snapshot(&owner);
                let original_window = original.windows()[0].clone();
                let other_window = original.windows().get(1).cloned();
                let resident = composer(invoking, cx).await;
                let entity = resident.entity_id();
                let generation = owner.borrow().test_ordinary_command_status().3;
                let old_selection = cx
                    .update(|app| resident.read(app).selection_identity())
                    .unwrap();
                let healthy_work = (healthy && count > 1).then(|| {
                    retain_unviewed_work(&owner, beryl_model::SyndicThreadId::from_bytes([245; 16]))
                });
                let permit = owner.borrow().test_services().process.execution_permit();
                let old_command = cx
                    .update(|app| {
                        owner
                            .borrow()
                            .window_exit_command(original_window.window_id(), app)
                            .unwrap()
                    })
                    .unwrap();
                let creation_owner = if create {
                    Some(
                        cx.update(|app| {
                            invoking
                                .read(app)
                                .unwrap()
                                .test_creation_owner_identity(app)
                                .unwrap()
                        })
                        .unwrap(),
                    )
                } else {
                    None
                };
                let thread = cx
                    .update(|app| resident.read(app).selection_identity().claim().thread_id())
                    .unwrap();
                owner
                    .borrow_mut()
                    .test_before_next_ordinary_session_removal(move |_| faults.fail_next(point));
                post_close(invoking_native);
                wait_until(
                    cx,
                    || {
                        let status = owner.borrow().test_ordinary_command_status();
                        (if healthy {
                            owner.borrow().test_cancelled_ordinary_close_outcomes()
                                == Some((Some(true), Some(true)))
                        } else {
                            status.3 != generation
                        }) && !status.1
                            && !status.2
                    },
                    "automatic ordinary-close recovery",
                )
                .await;
                assert!(unsafe { IsWindow(Some(invoking_native)).as_bool() });
                assert_eq!(windows(&owner).len(), usize::from(count));
                assert_eq!(composer(invoking, cx).await.entity_id(), entity);
                let restored = snapshot(&owner);
                let restored_window = restored
                    .windows()
                    .iter()
                    .find(|record| record.window_id() == original_window.window_id())
                    .unwrap();
                assert_eq!(restored_window.placement(), original_window.placement());
                assert_eq!(
                    restored_window.selected_thread().unwrap().thread_id(),
                    thread
                );
                if let Some(other_window) = other_window {
                    assert_eq!(
                        restored
                            .windows()
                            .iter()
                            .find(|record| record.window_id() == other_window.window_id())
                            .unwrap()
                            .placement(),
                        other_window.placement()
                    );
                }
                assert_eq!(
                    cx.update(|app| resident.read(app).selection_identity().claim())
                        .unwrap(),
                    restored_window.selected_thread().unwrap()
                );
                assert_eq!(copy_all(invoking, &resident, cx).await, draft_text(1));
                if healthy {
                    assert_eq!(owner.borrow().test_ordinary_command_status().3, generation);
                    let fresh = cx
                        .update(|app| resident.read(app).selection_identity())
                        .unwrap();
                    assert_eq!(fresh.binding(), old_selection.binding());
                    assert_eq!(
                        fresh.claim().generation(),
                        old_selection.claim().generation()
                    );
                    assert_ne!(fresh.claim().revision(), old_selection.claim().revision());
                    assert!(old_command.disabled_reason().is_none());
                    if healthy_work.is_some() {
                        assert!(
                            observe(&owner, ProjectionCancellationToken::new(), cx)
                                .await
                                .unwrap()
                                .has_work()
                        );
                    }
                    if count > 1 {
                        permit.commit(|| ()).unwrap();
                    }
                } else {
                    assert!(old_command.disabled_reason().is_some());
                    old_command.request_close();
                    old_command.request_exit();
                    assert!(!owner.borrow().exit_requested());
                }
                owner
                    .borrow()
                    .test_services()
                    .process
                    .execution_permit()
                    .commit(|| ())
                    .unwrap();
                if let Some(creation_owner) = creation_owner {
                    cx.update(|app| {
                        let root = invoking.read(app).unwrap();
                        assert_eq!(root.test_creation_owner_identity(app), Some(creation_owner));
                        assert!(root.new_window_disabled_reason(app).is_none());
                    })
                    .unwrap();
                    let original_windows = windows(&owner);
                    invoking
                        .update(cx, |root, _, cx| {
                            assert!(root.new_window_disabled_reason(cx).is_none());
                            root.test_activate_new_window_command(cx)
                        })
                        .unwrap();
                    wait_for_created_window(&owner, invoking, 3, cx).await;
                    let created = windows(&owner)
                        .into_iter()
                        .find(|window| !original_windows.contains(window))
                        .unwrap();
                    let created_native = native(created, cx).await;
                    assert_eq!(snapshot(&owner).windows().len(), 3);
                    cx.update(|app| {
                        assert!(created.read(app).unwrap().test_exit_command_enabled())
                    })
                    .unwrap();
                    post_close(created_native);
                    wait_for_created_close(&owner, created, 2, cx).await;
                    assert!(!unsafe { IsWindow(Some(created_native)).as_bool() });
                }
                let settled = snapshot(&owner);
                assert_eq!(settled.windows(), restored.windows());
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                assert!(unsafe { IsWindow(Some(invoking_native)).as_bool() });
                assert_eq!(snapshot(&owner), settled);
                drop(healthy_work);
                post_close(invoking_native);
                if count == 1 {
                    return;
                }
                wait_until(
                    cx,
                    || {
                        owner.borrow().test_ordinary_command_status().0 == 1
                            && !owner.borrow().exit_requested()
                    },
                    "fresh recovered close",
                )
                .await;
                assert!(!unsafe { IsWindow(Some(invoking_native)).as_bool() });
                activate_exit(windows(&owner)[0], cx);
            })
        },
        true,
        if count == 1 { 0 } else { 1 },
    );
}
