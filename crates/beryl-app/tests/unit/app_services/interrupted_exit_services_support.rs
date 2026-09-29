pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    candidate: InterruptedExitCandidate,
    generation: beryl_home_store::HomeGeneration,
    window: beryl_model::WindowId,
    faults: &FaultController,
    cx: &mut AsyncApp,
) -> InterruptedExitCandidate {
    use crate::app_services::recovery_graph::RecoveryServicePreparationError;
    use beryl_home_store::CommandCancellation;
    let mut candidate = Some(candidate);
    let foreign = request.test_foreign();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    for mode in ["success", "theme_failure", "cancel"] {
        let fresh = candidate.as_ref().unwrap();
        let home = fresh.candidate.home_id();
        let fresh_generation = fresh.candidate.generation();
        let requirement = configuration()
            .projection
            .turn_start_admission_requirement();
        let adapters = |request| {
            owner.borrow().interrupted_exit_composer_adapters(
                request,
                home,
                fresh_generation,
                requirement,
            )
        };
        cx.update(|app| {
            assert!(
                RunningProcessOwner::prepare_interrupted_exit_threadless_window(
                    owner,
                    request,
                    home,
                    generation,
                    window,
                    app,
                    |_, _, _| panic!("missing graph callback"),
                )
                .is_err()
            );
        })
        .unwrap();
        assert!(adapters(request).is_err());
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
            assert!(adapters(request).is_err());
            assert!(
                owner
                    .borrow_mut()
                    .take_interrupted_exit_preparation_failure(request, generation)
                    .is_err()
            );
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
        assert!(adapters(request).is_err());
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
        if mode == "success" {
            assert!(
                owner
                    .borrow_mut()
                    .take_interrupted_exit_preparation_failure(request, generation)
                    .is_err()
            );
        }
        assert!(adapters(&foreign).is_err());
        let retained_marker = if mode == "success" {
            for retirement in [None, Some(Err("failed retirement".into()))] {
                owner
                    .borrow()
                    .test_set_resident_graph_retirement(retirement);
                assert!(adapters(request).is_err());
            }
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Ok(())));
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_composer_adapters(request, home, generation, requirement,)
                    .is_err()
            );
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_composer_adapters(
                        request,
                        beryl_model::BerylHomeId::from_bytes([99; 16]),
                        fresh_generation,
                        requirement,
                    )
                    .is_err()
            );
            for (source_home, source_generation, source_window, stale, succeeds) in [
                (
                    beryl_model::BerylHomeId::from_bytes([99; 16]),
                    generation,
                    window,
                    false,
                    false,
                ),
                (home, fresh_generation, window, false, false),
                (
                    home,
                    generation,
                    beryl_model::WindowId::from_bytes([99; 16]),
                    false,
                    false,
                ),
                (home, generation, window, true, false),
                (home, generation, window, false, true),
            ] {
                let (sender, receiver) = futures_channel::oneshot::channel();
                cx.update(|app| {
                    let refused = |request, app: &mut gpui::App| {
                        RunningProcessOwner::prepare_interrupted_exit_threadless_window(
                            owner,
                            request,
                            home,
                            generation,
                            window,
                            app,
                            |_, _, _| panic!("refused window authentication callback"),
                        )
                    };
                    assert!(refused(&foreign, app).is_err());
                    for retirement in [None, Some(Err("failed retirement".into()))] {
                        owner
                            .borrow()
                            .test_set_resident_graph_retirement(retirement);
                        assert!(refused(request, app).is_err());
                    }
                    owner
                        .borrow()
                        .test_set_resident_graph_retirement(Some(Ok(())));
                    RunningProcessOwner::prepare_interrupted_exit_threadless_window(
                        owner,
                        request,
                        source_home,
                        source_generation,
                        source_window,
                        app,
                        move |owner, result, _| {
                            assert!(owner.borrow().interrupted_exit_session().is_some());
                            assert!(owner.borrow().test_services().graph().is_none());
                            assert!(sender.send(result).is_ok());
                        },
                    )
                    .unwrap();
                    assert!(refused(request, app).is_err());
                    assert!(adapters(request).is_err());
                    assert!(!RunningProcessOwner::finish_exit(owner, request));
                    if stale {
                        owner
                            .borrow_mut()
                            .test_replace_interrupted_exit_request(&foreign);
                    }
                })
                .unwrap();
                let result = receiver.await.unwrap();
                assert_eq!(result.is_ok(), succeeds);
                if let Ok(authenticated) = result {
                    assert_eq!(authenticated.home_id(), home);
                    assert_eq!(authenticated.generation(), fresh_generation);
                    assert_eq!(authenticated.window().window_id(), window);
                }
                if stale {
                    owner
                        .borrow_mut()
                        .test_replace_interrupted_exit_request(request);
                }
                owner
                    .borrow()
                    .interrupted_exit_services_result(request)
                    .unwrap();
            }
            let prepared = adapters(request).unwrap();
            assert!(prepared.matches(home, fresh_generation));
            let (_, marker, _, _) = prepared.into_parts();
            drop(adapters(request).unwrap());
            assert!(!marker.test_generation_retired());
            Some(marker)
        } else {
            assert!(adapters(request).is_err());
            None
        };
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
            assert!(adapters(request).is_err());
            assert!(retained_marker.as_ref().unwrap().test_generation_retired());
            assert_eq!(
                original,
                format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
            );
            assert!(owner.borrow().test_services().graph().is_none());
            assert!(!RunningProcessOwner::finish_exit(owner, request));
        }
        cx.update(|app| {
            assert!(
                RunningProcessOwner::prepare_interrupted_exit_threadless_window(
                    owner,
                    request,
                    home,
                    generation,
                    window,
                    app,
                    |_, _, _| panic!("failed graph callback"),
                )
                .is_err()
            );
        })
        .unwrap();
        let evidence = owner
            .borrow()
            .interrupted_exit_services_result(request)
            .unwrap_err();
        assert!(
            owner
                .borrow_mut()
                .take_interrupted_exit_preparation_failure(&foreign, generation)
                .is_err()
        );
        for retirement in [None, Some(Err("failed retirement".into()))] {
            owner
                .borrow()
                .test_set_resident_graph_retirement(retirement);
            assert!(
                owner
                    .borrow_mut()
                    .take_interrupted_exit_preparation_failure(request, generation)
                    .is_err()
            );
        }
        owner
            .borrow()
            .test_set_resident_graph_retirement(Some(Ok(())));
        let failure = owner
            .borrow_mut()
            .take_interrupted_exit_preparation_failure(request, generation)
            .unwrap();
        assert!(
            owner
                .borrow_mut()
                .take_interrupted_exit_preparation_failure(request, generation)
                .is_err()
        );
        assert_eq!(evidence, format!("{failure:?}"));
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        match failure {
            RecoveryServicePreparationError::App(failure) => {
                if mode != "theme_failure" {
                    assert!(matches!(failure.error(), AppServiceOpenError::Cancelled));
                } else {
                    assert!(matches!(failure.error(), AppServiceOpenError::Theme(_)));
                }
                assert!(failure.into_retry_parts().is_err());
            }
            error => panic!("unexpected preparation failure: {error:?}"),
        }
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            RunningProcessOwner::construct_interrupted_exit_candidate(
                owner,
                request,
                generation,
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
            RunningProcessOwner::settle_constructed_exit_candidate(
                owner,
                request,
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
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        candidate = Some(owner.borrow().test_take_interrupted_exit_candidate());
    }
    candidate.unwrap()
}
