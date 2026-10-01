mod retry_delay {
    use super::*;
    include!("recovery_reopen_delay_support.rs");
}

mod resume_attempt {
    use super::*;
    include!("recovery_resume_attempt_support.rs");
}

mod candidate_disposal {
    use super::*;
    include!("recovery_candidate_disposal_support.rs");
}

pub(super) async fn verify_and_dispose(
    owner: Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    publish: bool,
    cx: &mut AsyncApp,
) {
    let windows: Vec<_> = owner
        .borrow()
        .test_process()
        .windows
        .shells()
        .iter()
        .map(|shell| shell.window())
        .collect();
    let (home, retired) = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        (
            graph.home().home_id(),
            graph.home().health().generation().unwrap(),
        )
    };
    let (original, execution, resume_expected) = {
        let retained = owner.borrow();
        let session = retained.interrupted_exit_session().unwrap();
        let RunningShutdownSession::Settled(Ok(outcome)) = &*session else {
            panic!("selected recovery must retain the original execution");
        };
        if let ExitSessionExecution::Indeterminate(pending) = outcome {
            assert!(pending.candidate_resolution().is_none());
        }
        (
            publication_evidence(&session),
            std::mem::discriminant(outcome),
            !matches!(
                &*session,
                RunningShutdownSession::Settled(Ok(ExitSessionExecution::NotCommitted { .. }))
            ),
        )
    };
    let mut residents = Vec::new();
    for window in &windows {
        let snapshot = window
            .update(cx, |root, _, app| {
                assert!(!root.controller().unwrap().is_threadless());
                let mount = root.controller().unwrap().composer_mount().unwrap();
                let composer = mount.read(app).contribution().unwrap();
                let resident = composer.read(app);
                let input = resident.gpui_input();
                assert_eq!(root.test_exit_presentation().0, "Exiting…");
                let selection = resident.selection_identity().claim();
                let close = owner
                    .borrow()
                    .test_captured_recovery_ticket(composer.entity_id())
                    .unwrap();
                (mount, composer.clone(), input, selection, close)
            })
            .unwrap();
        residents.push(snapshot);
    }

    verify_exclusive_preparation(&owner, request, retired, &residents[0].0, cx).await;
    RunningProcessOwner::retire_and_prepare_interrupted_exit(
        &owner,
        request,
        retired,
        configuration(),
        SyndicTimestamp::from_unix_millis(2),
        CommandCancellation::new(),
        cx,
    )
    .await
    .unwrap();

    {
        let retained = owner.borrow();
        retained
            .interrupted_exit_graph_retirement_result(request)
            .unwrap();
        retained.interrupted_exit_services_result(request).unwrap();
        assert!(!retained.test_services_on_worker());
        assert!(retained.test_services().graph().is_none());
        let appearance = retained.interrupted_exit_appearance(request).unwrap();
        let candidate = appearance.prepared().home();
        assert_eq!(candidate.home_id(), home);
        assert_ne!(candidate.home_generation(), retired);
        let session = retained.interrupted_exit_session().unwrap();
        assert_eq!(original, publication_evidence(&session));
        if resume_expected {
            let RunningShutdownSession::Resuming(resume) = &*session else {
                panic!("committed Exit must retain a distinct Running resume");
            };
            assert!(resume.result_revision().is_some());
            let Some(crate::exit_session::ResumeSessionOutcome::Committed {
                receipt,
                later_failure: None,
                local_finalization: None,
            }) = resume.outcome()
            else {
                panic!("Running resume must retain its successful commit");
            };
            assert_eq!(receipt.generation(), candidate.home_generation());
            let crate::exit_session::InterruptedExit::Executed(outcome) = resume.exit() else {
                panic!("candidate settlement must preserve the original execution");
            };
            assert_eq!(execution, std::mem::discriminant(outcome));
            match outcome {
                ExitSessionExecution::Committed {
                    receipt,
                    later_failure: Some(_),
                    ..
                } => {
                    assert_eq!(receipt.generation(), retired);
                }
                ExitSessionExecution::Indeterminate(pending) => {
                    assert!(matches!(
                        pending.candidate_resolution(),
                        Some(Ok(
                            beryl_home_store::ReconciliationResolution::ExactNew { .. }
                        ))
                    ));
                }
                _ => panic!("expected original commit or candidate-reconciled commit"),
            }
        } else {
            assert!(matches!(
                &*session,
                RunningShutdownSession::Settled(Ok(ExitSessionExecution::NotCommitted { .. }))
            ));
        }
        assert!(retained.exit_requested());
        assert_eq!(
            retained.test_process().windows.shells().len(),
            windows.len()
        );
        for (index, (_, composer, _, _, close)) in residents.iter().enumerate() {
            assert_eq!(
                retained.test_process().windows.shells()[index].window(),
                windows[index]
            );
            assert_eq!(
                retained.test_captured_recovery_ticket(composer.entity_id()),
                Some(*close)
            );
        }
    }
    assert!(!RunningProcessOwner::finish_exit(&owner, request));
    for (window, (mount, composer, input, selection, close)) in windows.iter().zip(&residents) {
        window
            .update(cx, |root, _, app| {
                assert!(root.test_shell_construction_retired());
                assert_eq!(root.test_exit_presentation().0, "Exiting…");
                assert_eq!(
                    root.controller().unwrap().composer_mount().as_ref(),
                    Some(mount)
                );
                assert_eq!(mount.read(app).contribution().as_ref(), Some(composer));
                let resident = composer.read(app);
                assert_eq!(&resident.gpui_input(), input);
                assert_eq!(&resident.selection_identity().claim(), selection);
                assert_eq!(&resident.recovery_snapshot().unwrap().close_ticket(), close);
                assert!(!input.read(app).is_enabled());
            })
            .unwrap();
    }

    if publish {
        drop(residents);
        selected_publication::verify_and_dispose(owner, request, retired, cx).await;
        return;
    }

    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        RunningProcessOwner::cancel_interrupted_exit_services(&owner, request, app, move |_, _| {
            sender.send(()).unwrap();
        })
    })
    .unwrap()
    .unwrap();
    receiver.await.unwrap();
    assert!(matches!(
        owner
            .borrow_mut()
            .take_interrupted_exit_preparation_failure(request, retired)
            .unwrap(),
        crate::app_services::recovery_graph::RecoveryServicePreparationError::App(_)
    ));
    assert!(!owner.borrow().test_services_on_worker());
    assert!(!RunningProcessOwner::finish_exit(&owner, request));
    drop(residents);
    dispose_retired(owner, windows, None, cx).await;
}

pub(super) async fn verify_resume_failure(
    owner: Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    fault: beryl_home_store::test_faults::FaultPoint,
    driven: bool,
    cx: &mut AsyncApp,
) {
    let windows: Vec<_> = owner
        .borrow()
        .test_process()
        .windows
        .shells()
        .iter()
        .map(|shell| shell.window())
        .collect();
    assert_eq!(windows.len(), 2);
    let retired = owner
        .borrow()
        .test_services()
        .graph()
        .unwrap()
        .home()
        .health()
        .generation()
        .unwrap();
    let original = publication_evidence(&owner.borrow().interrupted_exit_session().unwrap());
    let error = candidate_disposal::prepare_failure(
        &owner,
        request,
        retired,
        match (driven, fault) {
            (true, beryl_home_store::test_faults::FaultPoint::BeforeCommit) => {
                candidate_disposal::DisposalWait::AbandonPending
            }
            (true, beryl_home_store::test_faults::FaultPoint::AfterPersist) => {
                candidate_disposal::DisposalWait::AbandonDelivered
            }
            _ => candidate_disposal::DisposalWait::Complete,
        },
        cx,
    )
    .await;
    {
        let retained = owner.borrow();
        retained
            .interrupted_exit_graph_retirement_result(request)
            .unwrap();
        assert_eq!(
            retained
                .interrupted_exit_candidate_result(request)
                .unwrap_err(),
            error
        );
        assert!(retained.interrupted_exit_services_result(request).is_err());
        assert!(retained.interrupted_exit_appearance(request).is_err());
        assert!(!retained.test_services_on_worker());
        assert!(retained.test_services().graph().is_none());
        assert!(retained.exit_requested());
        let session = retained.interrupted_exit_session().unwrap();
        assert_eq!(publication_evidence(&session), original);
        let RunningShutdownSession::Resuming(resume) = &*session else {
            panic!("resume outcome lost")
        };
        assert!(matches!(
            resume.exit(),
            crate::exit_session::InterruptedExit::Executed(ExitSessionExecution::Committed {
                later_failure: Some(_),
                ..
            })
        ));
        use crate::exit_session::ResumeSessionOutcome;
        use beryl_home_store::test_faults::FaultPoint;
        match fault {
            FaultPoint::BeforeCommit => assert!(matches!(
                resume.outcome(),
                Some(ResumeSessionOutcome::NotCommitted { .. })
            )),
            FaultPoint::AfterCommitBeforePersist => assert!(matches!(
                resume.outcome(),
                Some(ResumeSessionOutcome::Indeterminate {
                    reconciliation: Some(Err(_)),
                    ..
                })
            )),
            FaultPoint::AfterPersist => assert!(matches!(
                resume.outcome(),
                Some(ResumeSessionOutcome::Committed {
                    later_failure: Some(_),
                    ..
                })
            )),
            _ => panic!("unsupported resume failure"),
        }
        assert!(resume.result_revision().is_some());
        assert_eq!(
            retained.test_process().windows.shells().len(),
            windows.len()
        );
        for (index, window) in windows.iter().enumerate() {
            assert_eq!(
                retained.test_process().windows.shells()[index].window(),
                *window
            );
        }
    }
    assert!(!RunningProcessOwner::finish_exit(&owner, request));
    for window in &windows {
        window
            .update(cx, |root, _, app| {
                assert!(root.test_shell_construction_retired());
                assert_eq!(root.test_exit_presentation().0, "Exiting…");
                let mount = root.controller().unwrap().composer_mount().unwrap();
                let composer = mount.read(app).contribution().unwrap();
                let resident = composer.read(app);
                assert!(!resident.gpui_input().read(app).is_enabled());
                assert_eq!(
                    Some(resident.recovery_snapshot().unwrap().close_ticket()),
                    owner
                        .borrow()
                        .test_captured_recovery_ticket(composer.entity_id())
                );
            })
            .unwrap();
    }
    if fault != beryl_home_store::test_faults::FaultPoint::BeforeCommit {
        cx.update(|app| {
            assert!(
                RunningProcessOwner::retry_interrupted_exit_resume(
                    &owner,
                    request,
                    app,
                    |_, _| panic!("unproven noncommit retry")
                )
                .is_err()
            );
        })
        .unwrap();
    }
    {
        let resume_revision = {
            let retained = owner.borrow();
            let session = retained.interrupted_exit_session().unwrap();
            let RunningShutdownSession::Resuming(resume) = &*session else {
                panic!("resume outcome lost before next candidate")
            };
            resume.result_revision().unwrap()
        };
        candidate_disposal::verify_retained_failure(&owner, request, retired, &error, cx).await;
        retry_delay::verify(&owner, request, retired, 0, cx).await;
        if driven {
            resume_attempt::verify(&owner, request, retired, fault, resume_revision, cx).await;
            assert_eq!(
                original,
                publication_evidence(&owner.borrow().interrupted_exit_session().unwrap())
            );
            assert!(!RunningProcessOwner::finish_exit(&owner, request));
            for window in &windows {
                window
                    .update(cx, |root, _, app| {
                        assert_eq!(root.test_exit_presentation().0, "Exiting…");
                        let mount = root.controller().unwrap().composer_mount().unwrap();
                        let composer = mount.read(app).contribution().unwrap();
                        assert!(!composer.read(app).gpui_input().read(app).is_enabled());
                    })
                    .unwrap();
            }
            let candidate = owner.borrow().test_take_interrupted_exit_candidate();
            drop(candidate.session);
            let home = cx
                .background_executor()
                .spawn(async move { candidate.candidate.abort() })
                .await;
            dispose_retired(owner, windows, Some(home), cx).await;
            return;
        }
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            RunningProcessOwner::construct_interrupted_exit_candidate(
                &owner,
                request,
                retired,
                CommandCancellation::new(),
                app,
                move |_, _| {
                    sender.send(()).unwrap();
                },
            )
            .unwrap();
        })
        .unwrap();
        receiver.await.unwrap();
        owner
            .borrow()
            .interrupted_exit_construction_result(request)
            .unwrap();
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            assert!(
                RunningProcessOwner::settle_constructed_exit_candidate(
                    &owner,
                    &request.test_foreign(),
                    app,
                    |_, _| panic!("stale settlement completion")
                )
                .is_err()
            );
            RunningProcessOwner::settle_constructed_exit_candidate(
                &owner,
                request,
                app,
                move |_, _| {
                    sender.send(()).unwrap();
                },
            )
            .unwrap();
            assert!(
                RunningProcessOwner::settle_constructed_exit_candidate(
                    &owner,
                    request,
                    app,
                    |_, _| panic!("duplicate settlement completion")
                )
                .is_err()
            );
        })
        .unwrap();
        receiver.await.unwrap();
        {
            use crate::exit_session::ResumeSessionOutcome;
            use beryl_home_store::test_faults::FaultPoint;
            let retained = owner.borrow();
            let result = retained.interrupted_exit_candidate_result(request);
            let session = retained.interrupted_exit_session().unwrap();
            let RunningShutdownSession::Resuming(resume) = &*session else {
                panic!("resume outcome lost after next candidate")
            };
            assert_eq!(resume.result_revision(), Some(resume_revision));
            match fault {
                FaultPoint::BeforeCommit => {
                    assert_eq!(result.unwrap_err(), "Session resume did not commit");
                    assert!(matches!(
                        resume.outcome(),
                        Some(ResumeSessionOutcome::NotCommitted { .. })
                    ));
                }
                FaultPoint::AfterPersist => {
                    result.unwrap();
                    assert!(matches!(
                        resume.outcome(),
                        Some(ResumeSessionOutcome::Committed {
                            later_failure: Some(_),
                            ..
                        })
                    ));
                }
                FaultPoint::AfterCommitBeforePersist => {
                    assert!(result.is_err());
                    assert!(matches!(
                        resume.outcome(),
                        Some(ResumeSessionOutcome::Indeterminate {
                            reconciliation: Some(Err(_)),
                            ..
                        })
                    ));
                }
                _ => unreachable!(),
            }
        }
        if fault == beryl_home_store::test_faults::FaultPoint::BeforeCommit {
            verify_resume_retry(&owner, request, cx).await;
        }
        assert_eq!(
            original,
            publication_evidence(&owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(!RunningProcessOwner::finish_exit(&owner, request));
        assert_eq!(
            owner.borrow().test_process().windows.shells().len(),
            windows.len()
        );
        for window in &windows {
            window
                .update(cx, |root, _, app| {
                    assert_eq!(root.test_exit_presentation().0, "Exiting…");
                    let mount = root.controller().unwrap().composer_mount().unwrap();
                    let composer = mount.read(app).contribution().unwrap();
                    let resident = composer.read(app);
                    assert!(!resident.gpui_input().read(app).is_enabled());
                    assert_eq!(
                        Some(resident.recovery_snapshot().unwrap().close_ticket()),
                        owner
                            .borrow()
                            .test_captured_recovery_ticket(composer.entity_id())
                    );
                })
                .unwrap();
        }
        if fault != beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist {
            let candidate = owner.borrow().test_take_interrupted_exit_candidate();
            drop(candidate.session);
            let home = cx
                .background_executor()
                .spawn(async move { candidate.candidate.abort() })
                .await;
            dispose_retired(owner, windows, Some(home), cx).await;
            return;
        }
    }
    use crate::exit_session::ResumeSessionOutcome;
    assert!(
        owner
            .borrow_mut()
            .take_previous_interrupted_exit_resume_reconciliation(request)
            .is_err()
    );
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        assert!(
            RunningProcessOwner::retry_interrupted_exit_resume_reconciliation(
                &owner,
                &request.test_foreign(),
                app,
                |_, _| panic!("stale reconciliation retry")
            )
            .is_err()
        );
        RunningProcessOwner::retry_interrupted_exit_resume_reconciliation(
            &owner,
            request,
            app,
            move |owner, _| {
                assert!(owner.borrow().interrupted_exit_session().is_some());
                sender.send(()).unwrap();
            },
        )
        .unwrap();
        assert!(
            RunningProcessOwner::retry_interrupted_exit_resume_reconciliation(
                &owner,
                request,
                app,
                |_, _| panic!("duplicate reconciliation retry")
            )
            .is_err()
        );
        assert!(
            owner
                .borrow_mut()
                .take_previous_interrupted_exit_resume_reconciliation(request)
                .is_err()
        );
    })
    .unwrap();
    receiver.await.unwrap();
    owner
        .borrow()
        .interrupted_exit_candidate_result(request)
        .unwrap();
    {
        let retained = owner.borrow();
        let session = retained.interrupted_exit_session().unwrap();
        assert_eq!(publication_evidence(&session), original);
        let RunningShutdownSession::Resuming(resume) = &*session else {
            panic!("resume lost")
        };
        assert!(matches!(
            resume.outcome(),
            Some(ResumeSessionOutcome::Indeterminate {
                reconciliation: Some(Ok(
                    beryl_home_store::ReconciliationResolution::ExactNew { .. }
                )),
                ..
            })
        ));
    }
    assert!(
        owner
            .borrow_mut()
            .take_previous_interrupted_exit_resume_reconciliation(&request.test_foreign())
            .is_err()
    );
    owner
        .borrow_mut()
        .take_previous_interrupted_exit_resume_reconciliation(request)
        .unwrap();
    assert!(
        owner
            .borrow_mut()
            .take_previous_interrupted_exit_resume_reconciliation(request)
            .is_err()
    );
    cx.update(|app| {
        assert!(
            RunningProcessOwner::retry_interrupted_exit_resume_reconciliation(
                &owner,
                request,
                app,
                |_, _| panic!("settled reconciliation retry")
            )
            .is_err()
        );
    })
    .unwrap();
    assert!(!RunningProcessOwner::finish_exit(&owner, request));
    assert_eq!(
        owner.borrow().test_process().windows.shells().len(),
        windows.len()
    );
    for window in &windows {
        window
            .update(cx, |root, _, app| {
                assert_eq!(root.test_exit_presentation().0, "Exiting…");
                let mount = root.controller().unwrap().composer_mount().unwrap();
                let composer = mount.read(app).contribution().unwrap();
                assert!(!composer.read(app).gpui_input().read(app).is_enabled());
            })
            .unwrap();
    }
    let candidate = owner.borrow().test_take_interrupted_exit_candidate();
    drop(candidate.session);
    let home = cx
        .background_executor()
        .spawn(async move { candidate.candidate.abort() })
        .await;
    dispose_retired(owner, windows, Some(home), cx).await;
}

async fn verify_resume_retry(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    cx: &mut AsyncApp,
) {
    use crate::exit_session::ResumeSessionOutcome;
    assert!(
        owner
            .borrow_mut()
            .take_previous_interrupted_exit_resume(request)
            .is_err()
    );
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        assert!(
            RunningProcessOwner::retry_interrupted_exit_resume(
                owner,
                &request.test_foreign(),
                app,
                |_, _| panic!("stale retry")
            )
            .is_err()
        );
        RunningProcessOwner::retry_interrupted_exit_resume(owner, request, app, move |owner, _| {
            assert!(owner.borrow().interrupted_exit_session().is_some());
            sender.send(()).unwrap();
        })
        .unwrap();
        assert!(
            RunningProcessOwner::retry_interrupted_exit_resume(owner, request, app, |_, _| panic!(
                "duplicate retry"
            ))
            .is_err()
        );
        assert!(
            owner
                .borrow_mut()
                .take_previous_interrupted_exit_resume(request)
                .is_err()
        );
    })
    .unwrap();
    receiver.await.unwrap();
    owner
        .borrow()
        .interrupted_exit_candidate_result(request)
        .unwrap();
    {
        let retained = owner.borrow();
        let session = retained.interrupted_exit_session().unwrap();
        let RunningShutdownSession::Resuming(resume) = &*session else {
            panic!("resume lost")
        };
        assert!(matches!(
            resume.outcome(),
            Some(ResumeSessionOutcome::Committed {
                later_failure: None,
                ..
            })
        ));
    }
    cx.update(|app| {
        assert_eq!(
            RunningProcessOwner::retry_interrupted_exit_resume(owner, request, app, |_, _| {
                panic!("occupied previous outcome")
            })
            .unwrap_err(),
            "Interrupted Exit resume retry custody is unavailable"
        );
    })
    .unwrap();
    assert!(
        owner
            .borrow_mut()
            .take_previous_interrupted_exit_resume(&request.test_foreign())
            .is_err()
    );
    assert!(matches!(
        owner
            .borrow_mut()
            .take_previous_interrupted_exit_resume(request)
            .unwrap(),
        ResumeSessionOutcome::NotCommitted { .. }
    ));
    assert!(
        owner
            .borrow_mut()
            .take_previous_interrupted_exit_resume(request)
            .is_err()
    );
    cx.update(|app| {
        assert!(
            RunningProcessOwner::retry_interrupted_exit_resume(owner, request, app, |_, _| panic!(
                "committed retry"
            ))
            .is_err()
        );
    })
    .unwrap();
}

async fn dispose_retired(
    owner: Rc<RefCell<RunningProcessOwner>>,
    windows: Vec<gpui::WindowHandle<crate::main_window::MainWindowShellRoot>>,
    home: Option<beryl_home_store::HomeStore>,
    cx: &mut AsyncApp,
) {
    let mut running = Rc::try_unwrap(owner)
        .ok()
        .unwrap()
        .into_inner()
        .test_into_process();
    cx.update(|app| {
        for window in windows {
            window
                .update(app, |_, window, _| window.remove_window())
                .unwrap();
        }
        drop(running.windows);
        running
            .appearance
            .update(app, |appearance, _| appearance.retire());
        assert!(app.windows().is_empty());
    })
    .unwrap();
    cx.background_executor()
        .spawn(async move {
            assert!(running.services.graph().is_none());
            home.or_else(|| running.services.test_retired_service_home())
                .unwrap()
                .close()
                .unwrap();
        })
        .await;
}

async fn verify_exclusive_preparation(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    mount: &gpui::Entity<crate::main_window::MainWindowConversationComposerMount>,
    cx: &mut AsyncApp,
) {
    use std::{future::Future, task::Poll};

    let service = cx
        .update(|app| mount.read(app).test_retain_recovery_service())
        .unwrap();
    let original = publication_evidence(&owner.borrow().interrupted_exit_session().unwrap());
    for cancel in [true, false] {
        let cancellation = CommandCancellation::new();
        let mut drive_cx = cx.clone();
        let mut drive = Box::pin(RunningProcessOwner::retire_and_prepare_interrupted_exit(
            owner,
            request,
            retired,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            cancellation.clone(),
            &mut drive_cx,
        ));
        std::future::poll_fn(|task| {
            assert!(drive.as_mut().poll(task).is_pending());
            Poll::Ready(())
        })
        .await;
        assert!(!owner.borrow().test_services_on_worker());
        assert!(owner.borrow().test_services().graph().is_some());
        let foreign = request.test_foreign();
        for (attempt, expected) in [
            (&foreign, "request changed"),
            (request, "already being driven"),
        ] {
            let error = RunningProcessOwner::retire_and_prepare_interrupted_exit(
                owner,
                attempt,
                retired,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                CommandCancellation::new(),
                cx,
            )
            .await
            .unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
        assert_eq!(
            original,
            publication_evidence(&owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(owner.borrow().exit_requested());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        if cancel {
            cancellation.cancel();
            assert!(drive.await.unwrap_err().contains("cancelled"));
        } else {
            drop(drive);
        }
    }
    drop(service);
}

fn publication_evidence(session: &RunningShutdownSession) -> String {
    let publication = match session {
        RunningShutdownSession::Settled(Ok(outcome)) => outcome.publication(),
        RunningShutdownSession::Reconciled(outcome) => outcome.publication(),
        RunningShutdownSession::Resuming(resume) => resume.exit().publication(),
        _ => panic!("expected retained original Exit evidence"),
    };
    format!("{publication:?}")
}
