use crate::exit_session::ExitSessionExecution;
use crate::running_owner::{
    ExitProgressError, RunningShutdownDraftProgress, RunningShutdownSession,
};

#[test]
fn native_exit_session_publication_delivers_exact_ready_request() {
    run(None, false);
}

#[test]
fn native_exit_session_publication_retains_noncommit() {
    run(Some(FaultPoint::BeforeCommit), false);
}

#[test]
fn native_exit_session_publication_retains_postcommit_failure() {
    run(Some(FaultPoint::AfterPersist), false);
}

#[test]
fn native_exit_session_publication_retains_indeterminate_custody() {
    run(Some(FaultPoint::AfterCommitBeforePersist), false);
}

#[test]
fn native_exit_attempt_session_publication_retains_ready_request() {
    run(None, true);
}

#[test]
fn native_exit_attempt_session_publication_reports_noncommit() {
    run(Some(FaultPoint::BeforeCommit), true);
}

#[test]
fn native_exit_attempt_session_publication_reports_postcommit_failure() {
    run(Some(FaultPoint::AfterPersist), true);
}

#[test]
fn native_exit_attempt_session_publication_reports_indeterminate() {
    run(Some(FaultPoint::AfterCommitBeforePersist), true);
}

fn run(fault: Option<FaultPoint>, consumer: bool) {
    let directory = support::native_home();
    let faults = FaultController::new();
    let opening_faults = faults.clone();
    let input = input(directory.path(), move |path, _| {
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
            opening_faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let syndic = SyndicStorage::register(&mut candidate).unwrap();
        let candidate = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap();
        StartupHomeOpen::Ready {
            candidate,
            state,
            syndic,
        }
    });
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                input,
                move |result, app| {
                    let StartupCompletion::Running(running) = result else {
                        panic!("startup failed")
                    };
                    let invoking = running.windows.window_ids()[0];
                    let window = running.windows.shells()[0].window();
                    let owner = RunningProcessOwner::start(running, app);
                    owner
                        .borrow()
                        .window_exit_command(invoking, app)
                        .unwrap()
                        .request_exit();
                    app.spawn(async move |cx| {
                        let request = next_request(&owner, cx).await;
                        let identity = request.identity();
                        let (request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::publish_exit_session(
                                    &owner,
                                    request,
                                    app,
                                    |_, _, _, _| panic!("unadmitted publication"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::Intent));
                        let observation = observe(&owner, ProjectionCancellationToken::new(), cx)
                            .await
                            .unwrap();
                        cx.update(|app| {
                            owner.borrow_mut().try_begin_idle_shutdown(
                                invoking,
                                ShutdownIntent::ApplicationExit,
                                &observation,
                                app,
                            )
                        })
                        .unwrap()
                        .unwrap();
                        let (sender, receiver) = futures_channel::oneshot::channel();
                        assert!(
                            cx.update(|app| RunningProcessOwner::drive_exit(
                                &owner,
                                request,
                                ProjectionCancellationToken::new(),
                                app,
                                move |_, request, result, _| {
                                    assert!(matches!(
                                        result.unwrap(),
                                        AppServiceShutdownProgress::Ready
                                    ));
                                    sender.send(request).ok().unwrap();
                                }
                            ))
                            .unwrap()
                            .is_ok()
                        );
                        let request = receiver.await.unwrap();
                        let (request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::publish_exit_session(
                                    &owner,
                                    request,
                                    app,
                                    |_, _, _, _| panic!("drafts not ready"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::SessionPublication(_)));
                        assert!(owner.borrow().shutdown_session().is_none());
                        cx.update(|app| {
                            assert_eq!(
                                RunningProcessOwner::advance_shutdown_drafts(&owner, app).unwrap(),
                                RunningShutdownDraftProgress::Ready
                            )
                        })
                        .unwrap();
                        let (request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::publish_exit_session(
                                    &owner,
                                    request,
                                    app,
                                    |_, _, _, _| panic!("placements not ready"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::SessionPublication(_)));
                        assert!(owner.borrow().shutdown_session().is_none());
                        if !consumer {
                        let (sender, receiver) = futures_channel::oneshot::channel();
                        cx.update(|app| {
                            RunningProcessOwner::capture_shutdown_placements(
                                &owner,
                                app,
                                move |_, result, _| {
                                    sender.send(result).ok().unwrap();
                                },
                            )
                        })
                        .unwrap()
                        .unwrap();
                        receiver.await.unwrap().unwrap();
                        }
                        let original_attempt =
                            owner.borrow().test_services().graph().unwrap().shutdown;
                        if let Some(fault) = fault {
                            faults.fail_next(fault);
                        }
                        let (sender, receiver) = futures_channel::oneshot::channel();
                        let weak = Rc::downgrade(&owner);
                        let thread = std::thread::current().id();
                        assert!(
                            cx.update(|app| {
                                let completed = move |owner: &Rc<RefCell<RunningProcessOwner>>, request, result: Result<(), String>, app: &mut gpui::App| {
                                    assert_eq!(thread, std::thread::current().id());
                                    assert!(owner.try_borrow_mut().is_ok());
                                    assert!(!owner.borrow().test_services_on_worker());
                                    assert_eq!(
                                        owner.borrow().test_services().graph().unwrap().shutdown,
                                        original_attempt
                                    );
                                    assert_eq!(result.is_ok(), fault.is_none());
                                    assert!(owner.borrow().exit_requested());
                                    assert!(!RunningProcessOwner::finish_exit(owner, &request));
                                    assert!(
                                        RunningProcessOwner::release_shutdown_drafts(owner, app)
                                            .is_err()
                                    );
                                    assert!(
                                        RunningProcessOwner::release_shutdown_interaction_gate(
                                            owner, app
                                        )
                                        .is_err()
                                    );
                                    match (fault, owner.borrow().shutdown_session().unwrap()) {
                                        (
                                            None,
                                            RunningShutdownSession::Settled(Ok(
                                                ExitSessionExecution::Committed {
                                                    later_failure: None,
                                                    local_finalization: None,
                                                    ..
                                                },
                                            )),
                                        ) => {}
                                        (
                                            Some(FaultPoint::BeforeCommit),
                                            RunningShutdownSession::Settled(Ok(
                                                ExitSessionExecution::NotCommitted { .. },
                                            )),
                                        ) => {}
                                        (
                                            Some(FaultPoint::AfterPersist),
                                            RunningShutdownSession::Settled(Ok(
                                                ExitSessionExecution::Committed {
                                                    later_failure: Some(_),
                                                    local_finalization: Some(_),
                                                    ..
                                                },
                                            )),
                                        ) => {}
                                        (
                                            Some(FaultPoint::AfterCommitBeforePersist),
                                            RunningShutdownSession::Settled(Ok(
                                                ExitSessionExecution::Indeterminate(_),
                                            )),
                                        ) => {}
                                        other => panic!("unexpected publication: {other:?}"),
                                    }
                                    sender.send((owner.clone(), request)).ok().unwrap();
                                };
                                if consumer {
                                    RunningProcessOwner::test_complete_exit_work(&owner, request, app,
                                        move |owner, request, outcome, app| {
                                            assert!(!outcome.command_completed);
                                            let result = match outcome.result {
                                                Ok(crate::running_owner::ExitAttemptCompletion::SessionReady) => Ok(()),
                                                Err(crate::running_owner::ExitAttemptError::SessionPublication(error)) => {
                                                    let notice = window.read(app).unwrap().notice_projection().unwrap();
                                                    assert_eq!(notice.content.title().as_str(), "Couldn't exit Beryl");
                                                    assert_eq!(notice.content.commands().count(), 0);
                                                    assert_eq!(notice.report_count, 1);
                                                    assert_eq!(notice.content.detail().as_str(), format!("Exit session publication failed: {error}"));
                                                    Err(error)
                                                },
                                                other => panic!("unexpected attempt outcome: {other:?}"),
                                            };
                                            if result.is_ok() {
                                                assert!(window.read(app).unwrap().notice_projection().is_none());
                                            }
                                            completed(owner, request, result, app);
                                        });
                                    Ok(())
                                } else {
                                    RunningProcessOwner::publish_exit_session(&owner, request, app, completed)
                                }
                            })
                            .unwrap()
                            .is_ok()
                        );
                        drop(owner);
                        assert!(weak.upgrade().is_some());
                        let (owner, request) = receiver.await.unwrap();
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        let (request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::publish_exit_session(
                                    &owner,
                                    request,
                                    app,
                                    |_, _, _, _| panic!("duplicate publication"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::SessionPublication(_)));
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                        let cancelled = ProjectionCancellationToken::new();
                        cancelled.cancel();
                        assert!(
                            cx.update(|app| RunningProcessOwner::advance_shutdown(
                                &owner,
                                cancelled,
                                app,
                                |_, _| panic!("publication fences recovery")
                            ))
                            .unwrap()
                            .is_err()
                        );
                        // Settle only the fixture's installed store operation before disposing its native windows.
                        if matches!(fault, Some(FaultPoint::AfterCommitBeforePersist)) {
                            let home = owner
                                .borrow()
                                .test_services()
                                .graph()
                                .unwrap()
                                .home()
                                .service_reference();
                            let pending = home.pending_reconciliations();
                            assert_eq!(pending.len(), 1);
                            cx.background_executor()
                                .spawn(async move {
                                    assert!(matches!(
                                        home.retry_reconciliation(&pending[0]).unwrap(),
                                        beryl_home_store::ReconciliationResolution::ExactNew { .. }
                                    ));
                                })
                                .await;
                        }
                        let running = Rc::try_unwrap(owner)
                            .ok()
                            .unwrap()
                            .into_inner()
                            .test_into_process();
                        if matches!(
                            fault,
                            Some(FaultPoint::BeforeCommit | FaultPoint::AfterPersist)
                        ) {
                            dispose_failed_fixture(running, cx).await;
                        } else {
                            support::dispose_running(running, cx).await;
                        }
                        assert!(weak.upgrade().is_none());
                        observed.set(true);
                        cx.update(|app| app.quit()).unwrap();
                    })
                    .detach();
                },
                app,
            );
            support::watchdog(app);
        });
    assert!(finished.get());
    assert_reopens(&directory);
}

async fn dispose_failed_fixture(mut running: startup_owner::StartedProcess, cx: &mut AsyncApp) {
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        running.windows.test_dispose(
            move |result, _| {
                assert!(result.retained.is_none());
                sender.send(()).unwrap();
            },
            app,
        )
    })
    .unwrap();
    receiver.await.unwrap();
    running
        .appearance
        .update(cx, |appearance, _| appearance.retire())
        .unwrap();
    cx.background_executor()
        .spawn(async move {
            assert_eq!(
                running.services.graph().unwrap().home().health().state(),
                HomeHealthState::Failed
            );
            running.services.retire_failed_startup().unwrap();
            assert!(running.services.graph().is_none());
            assert!(running.services.retained_close().is_none());
        })
        .await;
}
