use crate::main_window::*;
use crate::running_owner::InterruptedExitRecoveryOutcome;
use crate::theme_runtime::AppearancePublicationTarget;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Scenario {
    Complete,
    CancelBeforeStart,
    RetryReopen,
    RetryResume,
    CommittedResumeFailure,
    DuplicateReport,
    StaleRequest,
    DropDuringRetry,
    PendingCompletion,
    PendingDraftRelease,
    CancelDuringDraftRelease,
    CancelBeforePublicationValidation,
    CancelDuringResidentPreparation,
    CancelAfterPublication,
}

#[test]
fn native_reported_exit_automatically_retries_reopening() {
    run(Scenario::RetryReopen, 2, FaultPoint::BeforeCommit);
}

#[test]
fn native_reported_exit_automatically_retries_noncommitted_resume() {
    run(Scenario::RetryResume, 2, FaultPoint::AfterPersist);
}

#[test]
fn native_reported_exit_preserves_committed_resume_failure() {
    run(
        Scenario::CommittedResumeFailure,
        2,
        FaultPoint::AfterPersist,
    );
}

#[test]
fn native_reported_exit_duplicate_reports_keep_one_automatic_task() {
    run(Scenario::DuplicateReport, 2, FaultPoint::BeforeCommit);
}

#[test]
fn native_reported_exit_cancellation_preserves_fences() {
    run(Scenario::CancelBeforeStart, 0, FaultPoint::BeforeCommit);
}

#[test]
fn native_reported_exit_stale_request_cannot_start_recovery() {
    run(Scenario::StaleRequest, 0, FaultPoint::BeforeCommit);
}

#[test]
fn native_reported_exit_owner_disposal_cancels_retry_task() {
    run(Scenario::DropDuringRetry, 0, FaultPoint::BeforeCommit);
}

#[test]
fn native_reported_exit_automatically_waits_for_coherent_completion() {
    run(Scenario::PendingCompletion, 2, FaultPoint::BeforeCommit);
}

#[test]
fn native_reported_exit_automatically_waits_for_draft_settlement() {
    run(Scenario::PendingDraftRelease, 2, FaultPoint::BeforeCommit);
}

#[test]
fn native_reported_exit_cancellation_disposes_adopted_unpublished_resources() {
    run(
        Scenario::CancelDuringDraftRelease,
        2,
        FaultPoint::BeforeCommit,
    );
}

#[test]
fn native_reported_exit_cancellation_before_publication_validation_disposes_candidate() {
    run(
        Scenario::CancelBeforePublicationValidation,
        2,
        FaultPoint::BeforeCommit,
    );
}

#[test]
fn native_reported_exit_cancellation_joins_resident_preparation_before_candidate_abort() {
    run(
        Scenario::CancelDuringResidentPreparation,
        2,
        FaultPoint::BeforeCommit,
    );
}

#[test]
fn native_reported_exit_late_cancellation_preserves_actual_publication_and_fences() {
    run(
        Scenario::CancelAfterPublication,
        2,
        FaultPoint::AfterPersist,
    );
}

#[test]
fn native_reported_exit_selected_automatically_reconciles_original_commit() {
    run_with_windows(
        Some(FaultPoint::AfterCommitBeforePersist),
        false,
        false,
        RecoveryPublicationDelivery::Automatic(Scenario::Complete),
        None,
        2,
    );
}

#[test]
fn native_reported_exit_threadless_automatically_reconciles_original_commit() {
    run_with_windows(
        Some(FaultPoint::AfterCommitBeforePersist),
        false,
        false,
        RecoveryPublicationDelivery::Automatic(Scenario::Complete),
        None,
        0,
    );
}

fn run(scenario: Scenario, windows: u8, fault: FaultPoint) {
    run_with_windows(
        Some(fault),
        true,
        false,
        RecoveryPublicationDelivery::Automatic(scenario),
        None,
        windows,
    );
}

pub(super) fn after_report(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    scenario: Scenario,
    faults: &FaultController,
    app: &mut gpui::App,
) {
    match scenario {
        Scenario::CancelBeforeStart => owner.borrow().cancel_automatic_recovery(),
        Scenario::CancelBeforePublicationValidation => owner
            .borrow_mut()
            .test_cancel_recovery_before_publication_validation(),
        Scenario::CancelDuringResidentPreparation => owner
            .borrow_mut()
            .test_cancel_recovery_after_resident_admission(),
        Scenario::CancelAfterPublication => {
            owner.borrow_mut().test_cancel_recovery_after_publication()
        }
        Scenario::RetryReopen | Scenario::DropDuringRetry => {
            faults.fail_next(FaultPoint::BeforeReopen)
        }
        Scenario::RetryResume => faults.fail_next(FaultPoint::BeforeCommit),
        Scenario::CommittedResumeFailure => faults.fail_next(FaultPoint::AfterPersist),
        Scenario::StaleRequest => {
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(&request.test_foreign());
        }
        Scenario::DuplicateReport => {
            let foreign = request.test_foreign();
            RunningProcessOwner::test_report_exit_delivery_failure(
                owner,
                &foreign,
                crate::running_owner::ExitAttemptError::SessionPublication("stale report".into()),
                false,
                app,
            );
            RunningProcessOwner::test_report_exit_delivery_failure(
                owner,
                request,
                crate::running_owner::ExitAttemptError::SessionPublication(
                    "duplicate report".into(),
                ),
                false,
                app,
            );
        }
        Scenario::PendingCompletion => {
            let window = owner.borrow().test_process().windows.shells()[0].window();
            let mount = window
                .read(app)
                .unwrap()
                .controller()
                .unwrap()
                .composer_mount()
                .unwrap();
            mount.update(app, |mount, _| {
                assert!(!mount.test_set_recovered_mount_deferred(true));
            });
        }
        Scenario::PendingDraftRelease | Scenario::CancelDuringDraftRelease => {
            let retained = owner.borrow();
            let windows = retained.test_process().windows.shells();
            let window = if scenario == Scenario::CancelDuringDraftRelease {
                windows.last().unwrap().window()
            } else {
                windows[0].window()
            };
            drop(retained);
            let mount = window
                .read(app)
                .unwrap()
                .controller()
                .unwrap()
                .composer_mount()
                .unwrap();
            mount.update(app, |mount, _| {
                assert!(!mount.test_set_recovery_draft_release_deferred(true));
            });
        }
        Scenario::Complete => {}
    }
}

pub(super) struct Snapshot {
    home: beryl_model::BerylHomeId,
    generation: beryl_home_store::HomeGeneration,
    windows: Vec<gpui::WindowHandle<MainWindowShellRoot>>,
    residents: Vec<(
        gpui::Entity<MainWindowConversationComposer>,
        gpui::Entity<gpui_text_input::RangeTextInput>,
        MainWindowComposerSelectionIdentity,
    )>,
}

pub(super) fn capture(owner: &Rc<RefCell<RunningProcessOwner>>, app: &gpui::App) -> Snapshot {
    let retained = owner.borrow();
    let graph = retained.test_services().graph().unwrap();
    let windows = retained
        .test_process()
        .windows
        .shells()
        .iter()
        .map(|shell| shell.window())
        .collect::<Vec<_>>();
    let residents = windows
        .iter()
        .filter_map(|window| {
            let root = window.read(app).unwrap();
            let mount = root.controller().unwrap().composer_mount()?;
            let resident = mount.read(app).contribution().unwrap();
            let input = resident.read(app).gpui_input();
            let selection = resident.read(app).selection_identity();
            Some((resident, input, selection))
        })
        .collect();
    Snapshot {
        home: graph.home().home_id(),
        generation: graph.home().health().generation().unwrap(),
        windows,
        residents,
    }
}

pub(super) async fn verify_and_dispose(
    owner: Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    snapshot: Snapshot,
    scenario: Scenario,
    cx: &mut AsyncApp,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut pending_completion = false;
    loop {
        if scenario == Scenario::DropDuringRetry
            && !owner.borrow().test_services_on_worker()
            && owner.borrow().test_services().graph().is_none()
            && owner
                .borrow()
                .interrupted_exit_reopen_deadline(request)
                .ok()
                .flatten()
                .is_some()
        {
            assert!(matches!(
                &*owner.borrow().automatic_recovery_outcome().unwrap(),
                InterruptedExitRecoveryOutcome::Running
            ));
            assert_eq!(
                Rc::strong_count(&owner),
                1,
                "retry task must hold only weak owner custody"
            );
            let weak = Rc::downgrade(&owner);
            let running = Rc::try_unwrap(owner)
                .ok()
                .unwrap()
                .into_inner()
                .test_into_process();
            assert!(weak.upgrade().is_none());
            dispose_failed_fixture(running, true, false, None, cx).await;
            return;
        }
        let completed = {
            let retained = owner.borrow();
            let outcome = retained
                .automatic_recovery_outcome()
                .expect("reported failure starts automatic recovery");
            match &*outcome {
                InterruptedExitRecoveryOutcome::Running => false,
                InterruptedExitRecoveryOutcome::Completed => true,
                InterruptedExitRecoveryOutcome::Cancelled
                    if matches!(
                        scenario,
                        Scenario::CancelBeforeStart
                            | Scenario::CancelDuringDraftRelease
                            | Scenario::CancelBeforePublicationValidation
                            | Scenario::CancelDuringResidentPreparation
                    ) =>
                {
                    true
                }
                InterruptedExitRecoveryOutcome::Unavailable(error)
                    if scenario == Scenario::StaleRequest =>
                {
                    assert!(error.contains("request changed"));
                    true
                }
                InterruptedExitRecoveryOutcome::Unavailable(error)
                    if scenario == Scenario::CancelAfterPublication =>
                {
                    assert!(error.contains("followed publication"));
                    true
                }
                InterruptedExitRecoveryOutcome::Cancelled => {
                    panic!("automatic recovery was unexpectedly cancelled")
                }
                InterruptedExitRecoveryOutcome::Unavailable(error) => {
                    panic!("automatic recovery is unavailable: {error}")
                }
            }
        };
        if completed {
            break;
        }
        if matches!(
            scenario,
            Scenario::PendingDraftRelease | Scenario::CancelDuringDraftRelease
        ) && !pending_completion
        {
            cx.update(|app| {
                let window = if scenario == Scenario::CancelDuringDraftRelease {
                    snapshot.windows.last().unwrap()
                } else {
                    &snapshot.windows[0]
                };
                let mount = window
                    .read(app)
                    .unwrap()
                    .controller()
                    .unwrap()
                    .composer_mount()
                    .unwrap();
                let consumed = mount.update(app, |mount, _| {
                    mount.test_set_recovery_draft_release_deferred(true)
                });
                if !consumed {
                    mount.update(app, |mount, _| {
                        assert!(mount.test_set_recovery_draft_release_deferred(false));
                    });
                    assert!(owner.borrow().exit_requested());
                    assert!(
                        owner
                            .borrow()
                            .interrupted_exit_publication_result(request)
                            .is_err()
                    );
                    pending_completion = true;
                    if scenario == Scenario::CancelDuringDraftRelease {
                        owner.borrow().cancel_automatic_recovery();
                    }
                }
            })
            .unwrap();
        }
        if scenario == Scenario::PendingCompletion {
            cx.update(|app| {
                let mount = snapshot.windows[0]
                    .read(app)
                    .unwrap()
                    .controller()
                    .unwrap()
                    .composer_mount()
                    .unwrap();
                let consumed = mount.update(app, |mount, _| {
                    mount.test_set_recovered_mount_deferred(true)
                });
                if !consumed {
                    mount.update(app, |mount, _| {
                        assert!(mount.test_set_recovered_mount_deferred(false));
                    });
                    if !owner.borrow().test_services_on_worker()
                        && owner.borrow().test_services().graph().is_some()
                    {
                        assert!(owner.borrow().exit_requested());
                        assert!(
                            owner
                                .borrow()
                                .test_services()
                                .process
                                .execution_permit()
                                .reserve()
                                .is_err()
                        );
                        pending_completion = true;
                    }
                }
            })
            .unwrap();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "automatic recovery did not complete"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    if scenario == Scenario::CancelAfterPublication {
        assert!(owner.borrow().exit_requested());
        owner
            .borrow()
            .interrupted_exit_publication_result(request)
            .unwrap();
        assert!(owner.borrow().test_services().graph().is_some());
        assert!(
            owner
                .borrow()
                .test_services()
                .process
                .execution_permit()
                .reserve()
                .is_err()
        );
        let retained = owner.borrow();
        let session = retained.interrupted_exit_session().unwrap();
        let RunningShutdownSession::Resuming(resume) = &*session else {
            panic!("known original commit retains resume evidence");
        };
        assert_eq!(resume.outcome().unwrap().known_commit(), Some(true));
        drop(session);
        drop(retained);
        cx.update(|app| {
            for (resident, input, selection) in &snapshot.residents {
                assert_eq!(resident.read(app).gpui_input(), *input);
                assert_eq!(
                    resident.read(app).selection_identity().claim(),
                    selection.claim()
                );
                assert!(!input.read(app).is_enabled());
            }
            assert_eq!(app.windows().len(), snapshot.windows.len());
        })
        .unwrap();
        RunningProcessOwner::recover_prepared_interrupted_exit(
            &owner,
            request,
            CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap();
        assert!(matches!(
            &*owner.borrow().automatic_recovery_outcome().unwrap(),
            InterruptedExitRecoveryOutcome::Unavailable(_)
        ));
    }
    if matches!(
        scenario,
        Scenario::CancelDuringDraftRelease
            | Scenario::CancelBeforePublicationValidation
            | Scenario::CancelDuringResidentPreparation
    ) {
        if scenario == Scenario::CancelDuringDraftRelease {
            assert!(pending_completion);
        }
        assert!(owner.borrow().exit_requested());
        assert!(!owner.borrow().test_services_on_worker());
        assert!(owner.borrow().test_services().graph().is_none());
        assert!(
            owner
                .borrow()
                .interrupted_exit_publication_result(request)
                .is_err()
        );
        cx.update(|app| {
            assert_eq!(app.windows().len(), snapshot.windows.len());
            for (window, (resident, input, selection)) in
                snapshot.windows.iter().zip(&snapshot.residents)
            {
                let root = window.read(app).unwrap();
                let mount = root.controller().unwrap().composer_mount().unwrap();
                assert!(mount.update(app, |mount, _| mount.test_unpublished_recovery_detached()));
                assert_eq!(resident.read(app).gpui_input(), *input);
                assert_eq!(
                    resident.read(app).selection_identity().claim(),
                    selection.claim()
                );
                assert!(!input.read(app).is_enabled());
            }
        })
        .unwrap();
        drop(snapshot.residents);
        let mut running = Rc::try_unwrap(owner)
            .ok()
            .unwrap()
            .into_inner()
            .test_into_process();
        cx.update(|app| {
            for shell in running.windows.shells() {
                shell
                    .window()
                    .update(app, |_, window, _| window.remove_window())
                    .unwrap();
            }
            running
                .appearance
                .update(app, |appearance, _| appearance.retire());
            drop(running.windows);
        })
        .unwrap();
        cx.background_executor()
            .spawn(async move {
                running
                    .services
                    .test_retired_service_home()
                    .unwrap()
                    .close()
                    .unwrap();
            })
            .await;
        return;
    }
    if matches!(
        scenario,
        Scenario::CancelBeforeStart | Scenario::StaleRequest
    ) {
        assert!(owner.borrow().exit_requested());
        assert!(owner.borrow().test_services().graph().is_some());
        assert!(!RunningProcessOwner::finish_exit(&owner, request));
        if scenario == Scenario::StaleRequest {
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(request);
            owner
                .borrow_mut()
                .retain_interrupted_exit_session(request)
                .unwrap();
        }
        loop {
            if cx
                .update(|app| {
                    owner
                        .borrow_mut()
                        .retire_interrupted_exit_residents(request, app)
                        .unwrap()
                })
                .unwrap()
            {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            RunningProcessOwner::retire_interrupted_exit_graph(
                &owner,
                request,
                snapshot.generation,
                app,
                move |_, _| {
                    sender.send(()).unwrap();
                },
            )
        })
        .unwrap()
        .unwrap();
        receiver.await.unwrap();
        let running = Rc::try_unwrap(owner)
            .ok()
            .unwrap()
            .into_inner()
            .test_into_process();
        dispose_failed_fixture(running, true, false, None, cx).await;
        return;
    }
    if matches!(
        scenario,
        Scenario::PendingCompletion | Scenario::PendingDraftRelease
    ) {
        assert!(pending_completion);
    }
    if scenario == Scenario::RetryResume {
        assert!(
            owner
                .borrow()
                .automatic_recovery_failure()
                .unwrap()
                .is_some()
        );
    }
    assert!(!owner.borrow().exit_requested());
    assert!(!RunningProcessOwner::finish_exit(&owner, request));
    assert!(owner.borrow().interrupted_exit_session().is_none());
    assert!(owner.borrow().shutdown_status().is_none());
    let appearance = owner.borrow().test_process_appearance();
    {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        assert_eq!(graph.home().home_id(), snapshot.home);
        assert_ne!(
            graph.home().health().generation().unwrap(),
            snapshot.generation
        );
        assert_eq!(
            retained
                .test_process()
                .windows
                .shells()
                .iter()
                .map(|shell| shell.window())
                .collect::<Vec<_>>(),
            snapshot.windows
        );
        drop(
            retained
                .test_services()
                .process
                .execution_permit()
                .reserve()
                .unwrap(),
        );
    }
    cx.update(|app| {
        assert_eq!(app.windows().len(), snapshot.windows.len());
        for window in &snapshot.windows {
            let root = window.read(app).unwrap();
            assert_eq!(root.test_exit_presentation().0, "Exit");
            assert!(!root.test_notices_inert());
        }
        for (resident, input, selection) in &snapshot.residents {
            assert_eq!(resident.read(app).gpui_input(), *input);
            assert_eq!(
                resident.read(app).selection_identity().claim(),
                selection.claim()
            );
            assert_ne!(
                resident.read(app).selection_identity().binding(),
                selection.binding()
            );
            assert!(input.read(app).is_enabled());
        }
        let fresh = appearance
            .read(app)
            .target()
            .snapshot()
            .current
            .prepared()
            .home()
            .home_generation();
        assert_ne!(fresh, snapshot.generation);
    })
    .unwrap();
    if snapshot.residents.is_empty() {
        let running = Rc::try_unwrap(owner)
            .ok()
            .unwrap()
            .into_inner()
            .test_into_process();
        dispose_failed_fixture(running, true, false, Some(appearance), cx).await;
    } else {
        drop(snapshot.residents);
        selected_publication::dispose_recovered(owner, snapshot.windows, appearance, cx).await;
    }
}
