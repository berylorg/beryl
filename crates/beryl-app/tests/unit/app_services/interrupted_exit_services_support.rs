pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    candidate: InterruptedExitCandidate,
    generation: beryl_home_store::HomeGeneration,
    faults: &FaultController,
    cx: &mut AsyncApp,
) -> InterruptedExitCandidate {
    use crate::app_services::recovery_graph::RecoveryServicePreparationError;
    use beryl_home_store::CommandCancellation;
    let mut candidate = Some(candidate);
    let foreign = request.test_foreign();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    for mode in ["success", "theme_failure", "cancel"] {
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            RunningProcessOwner::settle_interrupted_exit_candidate(
                owner,
                request,
                &mut candidate,
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
            .interrupted_exit_candidate_result(request)
            .unwrap();
        let cancellation = CommandCancellation::new();
        if mode == "theme_failure" {
            faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
        }
        let canceller = if mode == "cancel" {
            let block = faults.block_next(FaultPoint::BeforeThemeWatchSpawn);
            let cancellation = cancellation.clone();
            Some(std::thread::spawn(move || {
                let reached = block.wait_until_reached(std::time::Duration::from_secs(10));
                cancellation.cancel();
                block.release();
                assert!(reached);
            }))
        } else {
            None
        };
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            let start = |request, cancellation, app: &mut gpui::App| {
                RunningProcessOwner::prepare_interrupted_exit_services(
                    owner,
                    request,
                    generation,
                    configuration(),
                    SyndicTimestamp::from_unix_millis(2),
                    cancellation,
                    app,
                    |_, _| panic!("refused preparation callback"),
                )
            };
            assert!(start(&foreign, CommandCancellation::new(), app).is_err());
            for retirement in [None, Some(Err("failed retirement".into()))] {
                owner
                    .borrow()
                    .test_set_resident_graph_retirement(retirement);
                assert!(start(request, CommandCancellation::new(), app).is_err());
            }
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Ok(())));
            let cancelled = CommandCancellation::new();
            cancelled.cancel();
            assert!(start(request, cancelled, app).is_err());
            assert!(owner.borrow().interrupted_exit_session().is_some());
            RunningProcessOwner::prepare_interrupted_exit_services(
                owner,
                request,
                generation,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                cancellation,
                app,
                move |owner, _| {
                    assert!(!owner.borrow().test_services_on_worker());
                    assert!(owner.borrow().interrupted_exit_session().is_some());
                    sender.send(()).unwrap();
                },
            )
            .unwrap();
            assert!(owner.borrow().test_services_on_worker());
            assert!(owner.borrow().interrupted_exit_session().is_none());
            assert!(start(request, CommandCancellation::new(), app).is_err());
            assert!(!RunningProcessOwner::finish_exit(owner, request));
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(&foreign);
        })
        .unwrap();
        receiver.await.unwrap();
        if let Some(canceller) = canceller {
            canceller.join().unwrap();
        }
        assert!(
            owner
                .borrow()
                .interrupted_exit_services_result(request)
                .unwrap_err()
                .contains("request changed")
        );
        owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(request);
        assert_eq!(
            owner
                .borrow()
                .interrupted_exit_services_result(request)
                .is_ok(),
            mode == "success"
        );
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(owner.borrow().test_services().graph().is_none());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            let cancel = |request, app: &mut gpui::App| {
                RunningProcessOwner::cancel_interrupted_exit_services(
                    owner,
                    request,
                    app,
                    |_, _| panic!("refused cancellation callback"),
                )
            };
            assert!(cancel(&foreign, app).is_err());
            if mode != "success" {
                assert!(cancel(request, app).is_err());
                return;
            }
            for retirement in [None, Some(Err("failed retirement".into()))] {
                owner
                    .borrow()
                    .test_set_resident_graph_retirement(retirement);
                assert!(cancel(request, app).is_err());
            }
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Ok(())));
            RunningProcessOwner::cancel_interrupted_exit_services(
                owner,
                request,
                app,
                move |owner, _| {
                    assert!(!owner.borrow().test_services_on_worker());
                    assert!(owner.borrow().interrupted_exit_session().is_some());
                    sender.send(()).unwrap();
                },
            )
            .unwrap();
            assert!(owner.borrow().test_services_on_worker());
            assert!(owner.borrow().interrupted_exit_session().is_none());
            assert!(cancel(request, app).is_err());
            assert!(!RunningProcessOwner::finish_exit(owner, request));
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(&foreign);
        })
        .unwrap();
        if mode == "success" {
            receiver.await.unwrap();
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_services_result(request)
                    .unwrap_err()
                    .contains("request changed")
            );
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(request);
            assert_eq!(
                original,
                format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
            );
            assert!(owner.borrow().test_services().graph().is_none());
            assert!(!RunningProcessOwner::finish_exit(owner, request));
        }
        let result = owner.borrow().test_take_interrupted_exit_services();
        candidate = Some(
            cx.background_executor()
                .spawn(async move {
                    let failure = match result {
                        Ok(_) => panic!("cancelled services remained prepared"),
                        Err(RecoveryServicePreparationError::App(failure)) => {
                            if mode != "theme_failure" {
                                assert!(matches!(failure.error(), AppServiceOpenError::Cancelled));
                            } else {
                                assert!(matches!(failure.error(), AppServiceOpenError::Theme(_)));
                            }
                            failure
                        }
                        Err(error) => panic!("unexpected preparation failure: {error:?}"),
                    };
                    let (home, _, _) = failure.into_retry_parts().unwrap();
                    let candidate = home.recover_same_home().unwrap();
                    let state = BerylState::reacquire_candidate(&candidate).unwrap();
                    InterruptedExitCandidate {
                        candidate,
                        session: state.session(),
                    }
                })
                .await,
        );
    }
    candidate.unwrap()
}
