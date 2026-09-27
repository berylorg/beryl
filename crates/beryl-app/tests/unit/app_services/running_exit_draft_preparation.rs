use crate::running_owner::{
    ExitDraftPreparationCompletion, ExitProgressError, RunningShutdownDraftAction,
    RunningShutdownDraftProgress,
};

#[test]
fn native_exit_draft_preparation_retains_ready_attempt() {
    run(false, false);
}

#[test]
fn native_exit_draft_preparation_preserves_failure_after_reopening() {
    run(true, false);
}

#[test]
fn native_exit_draft_preparation_preserves_both_failures_and_custody() {
    run(true, true);
}

fn run(preparation_failure: bool, recovery_failure: bool) {
    run_policy(preparation_failure, recovery_failure, false);
}

#[test]
fn native_exit_consumer_completes_recovered_draft_failure_and_reports_original_cause() {
    run_policy(true, false, true);
}

#[test]
fn native_exit_consumer_retains_failed_draft_recovery_and_reports_both_causes() {
    run_policy(true, true, true);
}

fn run_policy(preparation_failure: bool, recovery_failure: bool, consumer: bool) {
    let directory = support::native_home();
    let input = input(directory.path(), |path, _| support::open(path));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new().with_quit_on_last_window_close(false).run(move |app| {
        startup_owner::start(input, move |result, app| {
            let StartupCompletion::Running(running) = result else { panic!("startup failed") };
            let invoking = running.windows.window_ids()[0];
            let window = running.windows.shells()[0].window();
            let original_reason = window.read(app).unwrap().new_window_disabled_reason(app);
            let owner = RunningProcessOwner::start(running, app);
            owner.borrow().window_exit_command(invoking, app).unwrap().request_exit();
            app.spawn(async move |cx| {
                let request = next_request(&owner, cx).await;
                let identity = request.identity();
                let (request, error) = cx.update(|app| RunningProcessOwner::prepare_exit_drafts(
                    &owner, request, app, |_, _, _, _| panic!("unadmitted preparation")))
                    .unwrap().err().unwrap();
                assert!(matches!(error, ExitProgressError::Intent));
                let observation = observe(&owner, ProjectionCancellationToken::new(), cx).await.unwrap();
                cx.update(|app| owner.borrow_mut().try_begin_idle_shutdown(
                    invoking, ShutdownIntent::ApplicationExit, &observation, app)).unwrap().unwrap();
                let (request, error) = cx.update(|app| RunningProcessOwner::prepare_exit_drafts(
                    &owner, request, app, |_, _, _, _| panic!("work not ready")))
                    .unwrap().err().unwrap();
                assert!(matches!(error, ExitProgressError::Intent));
                let (sender, receiver) = futures_channel::oneshot::channel();
                assert!(cx.update(|app| RunningProcessOwner::drive_exit(
                    &owner, request, ProjectionCancellationToken::new(), app,
                    move |_, request, result, _| {
                        assert!(matches!(result.unwrap(), AppServiceShutdownProgress::Ready));
                        sender.send(request).ok().unwrap();
                    })).unwrap().is_ok());
                let mut request = receiver.await.unwrap();
                let attempt = owner.borrow().test_services().graph().unwrap().shutdown;
                if recovery_failure {
                    let (sender, receiver) = futures_channel::oneshot::channel();
                    let (returned, error) = cx.update(|app| {
                        RunningProcessOwner::drive_shutdown_drafts(&owner, RunningShutdownDraftAction::Prepare,
                            app, move |_, result, _| {
                                assert_eq!(result.unwrap(), RunningShutdownDraftProgress::Ready);
                                sender.send(()).ok().unwrap();
                            }).unwrap();
                        RunningProcessOwner::prepare_exit_drafts(&owner, request, app,
                            |_, _, _, _| panic!("busy preparation must not notify"))
                    }).unwrap().err().unwrap();
                    request = returned;
                    assert!(matches!(error, ExitProgressError::DraftPreparation(_)));
                    assert!(Rc::ptr_eq(&identity, &request.identity()));
                    receiver.await.unwrap();
                }
                let expected_failure = cx.update(|app| {
                    if preparation_failure {
                        window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(false, cx))
                            .unwrap().unwrap();
                        Some(RunningProcessOwner::advance_shutdown_drafts(&owner, app).unwrap_err())
                    } else { None }
                }).unwrap();
                let (sender, receiver) = futures_channel::oneshot::channel();
                let weak = Rc::downgrade(&owner);
                let thread = std::thread::current().id();
                assert!(cx.update(|app| {
                    let completed = move |owner: &Rc<RefCell<RunningProcessOwner>>, request, result, app: &mut gpui::App| {
                            assert_eq!(thread, std::thread::current().id());
                            assert!(owner.try_borrow_mut().is_ok());
                            assert!(!owner.borrow().test_services_on_worker());
                            assert_eq!(owner.borrow().exit_requested(), !consumer || recovery_failure);
                            match result {
                                ExitDraftPreparationCompletion::Ready => {
                                    assert!(!preparation_failure);
                                    assert!(matches!(owner.borrow().shutdown_status(),
                                        Some((_, _, RunningShutdownStatus::WorkReady))));
                                    assert!(!RunningProcessOwner::finish_exit(owner, &request));
                                    assert!(RunningProcessOwner::release_shutdown_interaction_gate(owner, app).is_err());
                                }
                                ExitDraftPreparationCompletion::Failed { preparation, recovery } => {
                                    assert_eq!(Some(preparation), expected_failure);
                                    if recovery_failure {
                                        assert!(matches!(recovery, Err(ExitProgressError::DraftRelease(_))));
                                        assert_eq!(owner.borrow().test_services().graph().unwrap().shutdown, attempt);
                                        assert!(!RunningProcessOwner::finish_exit(owner, &request));
                                    } else {
                                        assert!(matches!(recovery.unwrap(), AppServiceShutdownProgress::Failed {
                                            reason: crate::cas_projection::ShutdownFailure::Cancelled, reopened: true
                                        }));
                                        assert!(owner.borrow().shutdown_status().is_none());
                                        assert_eq!(window.read(app).unwrap().new_window_disabled_reason(app), original_reason);
                                        assert_eq!(RunningProcessOwner::finish_exit(owner, &request), !consumer);
                                    }
                                }
                            }
                            sender.send((owner.clone(), request)).ok().unwrap();
                        };
                    if consumer {
                        RunningProcessOwner::test_complete_exit_work(&owner, request,
                            app, move |owner, request, outcome, app| {
                                assert_eq!(outcome.command_completed, !recovery_failure);
                                let root = window.read(app).unwrap();
                                let notice = root.notice_projection().unwrap();
                                assert_eq!(notice.content.title().as_str(), "Couldn't exit Beryl");
                                assert_eq!(notice.content.commands().count(), 0);
                                assert_eq!(notice.content.detail().as_str(), outcome.result.as_ref().unwrap_err().to_string());
                                let crate::running_owner::ExitAttemptError::DraftPreparation { preparation, recovery } = outcome.result.unwrap_err()
                                    else { panic!("original failure was replaced") };
                                completed(owner, request, ExitDraftPreparationCompletion::Failed { preparation, recovery }, app);
                            });
                        Ok(())
                    } else {
                        let scheduled = RunningProcessOwner::prepare_exit_drafts(&owner, request, app, completed);
                        assert!(RunningProcessOwner::drive_shutdown_drafts(&owner, RunningShutdownDraftAction::Prepare,
                            app, |_, _, _| panic!("duplicate preparation")).is_err());
                        scheduled
                    }
                }).unwrap().is_ok());
                drop(owner);
                assert!(weak.upgrade().is_some());
                let (owner, returned) = receiver.await.unwrap();
                request = returned;
                assert!(Rc::ptr_eq(&identity, &request.identity()));
                if !preparation_failure || recovery_failure {
                    let (sender, receiver) = futures_channel::oneshot::channel();
                    assert!(cx.update(|app| {
                        window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(true, cx))
                            .unwrap().unwrap();
                        RunningProcessOwner::recover_exit_drafts(&owner, request, app,
                            move |owner, request, result, _| {
                                assert!(matches!(result.unwrap(), AppServiceShutdownProgress::Failed { reopened: true, .. }));
                                assert!(RunningProcessOwner::finish_exit(owner, &request));
                                sender.send(request).ok().unwrap();
                            })
                    }).unwrap().is_ok());
                    request = receiver.await.unwrap();
                }
                let (_, error) = cx.update(|app| RunningProcessOwner::prepare_exit_drafts(
                    &owner, request, app, |_, _, _, _| panic!("stale preparation")))
                    .unwrap().err().unwrap();
                assert!(matches!(error, ExitProgressError::Request(_)));
                let running = Rc::try_unwrap(owner).ok().unwrap().into_inner().test_into_process();
                support::dispose_running(running, cx).await;
                assert!(weak.upgrade().is_none());
                observed.set(true);
                cx.update(|app| app.quit()).unwrap();
            }).detach();
        }, app);
        support::watchdog(app);
    });
    assert!(finished.get());
    assert_reopens(&directory);
}
