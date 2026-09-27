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

fn foreign_recovery_candidate() -> (
    tempfile::TempDir,
    crate::running_owner::InterruptedExitCandidate,
) {
    let directory = support::native_home();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    BerylState::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let candidate = home.recover_same_home().unwrap();
    let session = BerylState::reacquire_candidate(&candidate)
        .unwrap()
        .session();
    (
        directory,
        crate::running_owner::InterruptedExitCandidate { candidate, session },
    )
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
                        if consumer && fault.is_some() {
                            if matches!(fault, Some(FaultPoint::AfterCommitBeforePersist)) {
                                let (sender, receiver) = futures_channel::oneshot::channel();
                                cx.update(|app| RunningProcessOwner::reconcile_shutdown_session(
                                    &owner, app, move |_, _| { sender.send(()).unwrap(); }
                                )).unwrap().unwrap();
                                receiver.await.unwrap();
                                let borrowed = owner.borrow();
                                let graph = borrowed.test_services().graph().unwrap();
                                assert!(borrowed.shutdown_session().unwrap().require_ready(graph.home(), &graph.state().session()).is_ok());
                                assert!(borrowed.require_shutdown_session_ready().is_err());
                            }
                            let before = format!("{:?}", owner.borrow().shutdown_session().unwrap());
                            let foreign = request.test_foreign();
                            assert!(owner.borrow_mut().retain_interrupted_exit_session(&foreign).is_err());
                            assert!(owner.borrow().interrupted_exit_session().is_none());
                            assert_eq!(before, format!("{:?}", owner.borrow().shutdown_session().unwrap()));
                            owner.borrow_mut().retain_interrupted_exit_session(&request).unwrap();
                            assert_eq!(before, format!("{:?}", owner.borrow().interrupted_exit_session().unwrap()));
                            assert!(matches!(owner.borrow().shutdown_session(), Some(RunningShutdownSession::RecoveryOwned)));
                            assert!(owner.borrow_mut().retain_interrupted_exit_session(&request).is_err());
                            assert!(owner.borrow().require_shutdown_session_ready().is_err());
                            assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                            let (directory, candidate) = cx.background_executor().spawn(async {
                                foreign_recovery_candidate()
                            }).await;
                            let mut candidate = Some(candidate);
                            let (sender, receiver) = futures_channel::oneshot::channel();
                            cx.update(|app| {
                                assert!(RunningProcessOwner::settle_interrupted_exit_candidate(
                                    &owner, &foreign, &mut candidate, app, |_, _| panic!("foreign admission")
                                ).is_err());
                                assert!(candidate.is_some());
                                assert_eq!(before, format!("{:?}", owner.borrow().interrupted_exit_session().unwrap()));
                                RunningProcessOwner::test_settle_interrupted_exit_candidate(
                                    &owner, &request, &mut candidate, app,
                                    move |_, _| { sender.send(()).unwrap(); },
                                    move || {
                                        if matches!(fault, Some(FaultPoint::AfterPersist)) {
                                            panic!("injected settlement unwind");
                                        }
                                    },
                                ).unwrap();
                                assert!(candidate.is_none());
                                assert!(owner.borrow().interrupted_exit_session().is_none());
                                assert!(owner.borrow().interrupted_exit_candidate_result(&request).is_err());
                                assert!(RunningProcessOwner::settle_interrupted_exit_candidate(
                                    &owner, &request, &mut candidate, app, |_, _| panic!("duplicate settlement")
                                ).is_err());
                                owner.borrow_mut().test_replace_interrupted_exit_request(&foreign);
                            }).unwrap();
                            receiver.await.unwrap();
                            assert!(owner.borrow().interrupted_exit_candidate_result(&request).unwrap_err().contains("request changed"));
                            owner.borrow_mut().test_replace_interrupted_exit_request(&request);
                            let result = owner.borrow().interrupted_exit_candidate_result(&request).unwrap_err();
                            assert!(result.contains(if matches!(fault, Some(FaultPoint::AfterPersist)) { "unwound" } else { "different configured home" }));
                            assert_eq!(before, format!("{:?}", owner.borrow().interrupted_exit_session().unwrap()));
                            assert!(owner.borrow().require_shutdown_session_ready().is_err());
                            assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                            let candidate = owner.borrow().test_take_interrupted_exit_candidate();
                            cx.background_executor().spawn(async move {
                                candidate.candidate.abort().close().unwrap();
                                drop(directory);
                            }).await;
                        } else {
                            assert!(owner.borrow_mut().retain_interrupted_exit_session(&request).is_err());
                            assert!(owner.borrow().interrupted_exit_session().is_none());
                        }
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
                        if !consumer && matches!(fault, Some(FaultPoint::AfterCommitBeforePersist)) {
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
