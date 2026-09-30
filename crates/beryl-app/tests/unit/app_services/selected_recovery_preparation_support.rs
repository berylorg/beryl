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
    drop(residents);
    cx.background_executor()
        .spawn(async move {
            assert!(running.services.graph().is_none());
            running
                .services
                .test_retired_service_home()
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
