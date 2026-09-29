pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    generation: beryl_home_store::HomeGeneration,
    stale_generation: beryl_home_store::HomeGeneration,
    faults: &FaultController,
    cx: &mut AsyncApp,
) -> beryl_home_store::HomeRecoveryCandidate {
    let foreign = request.test_foreign();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    cx.update(|app| {
        assert!(
            RunningProcessOwner::construct_interrupted_exit_candidate(
                owner,
                &foreign,
                generation,
                beryl_home_store::CommandCancellation::new(),
                app,
                |_, _| panic!("foreign construction")
            )
            .is_err()
        );
        owner.borrow().test_set_resident_graph_retirement(None);
        assert!(
            RunningProcessOwner::construct_interrupted_exit_candidate(
                owner,
                request,
                generation,
                beryl_home_store::CommandCancellation::new(),
                app,
                |_, _| panic!("unproven retirement")
            )
            .is_err()
        );
        owner
            .borrow()
            .test_set_resident_graph_retirement(Some(Err("failed retirement".into())));
        assert_eq!(
            RunningProcessOwner::construct_interrupted_exit_candidate(
                owner,
                request,
                generation,
                beryl_home_store::CommandCancellation::new(),
                app,
                |_, _| panic!("failed retirement")
            )
            .unwrap_err(),
            "failed retirement"
        );
        owner
            .borrow()
            .test_set_resident_graph_retirement(Some(Ok(())));
        let cancellation = beryl_home_store::CommandCancellation::new();
        cancellation.cancel();
        assert_eq!(
            RunningProcessOwner::construct_interrupted_exit_candidate(
                owner,
                request,
                generation,
                cancellation,
                app,
                |_, _| panic!("pre-cancelled construction")
            )
            .unwrap_err(),
            "recovery candidate construction was cancelled"
        );
        assert!(!owner.borrow().test_services_on_worker());
    })
    .unwrap();
    for (pass, fault) in [
        Some(FaultPoint::BeforeReopen),
        Some(FaultPoint::AfterReopen),
        None,
        None,
        None,
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(fault) = fault {
            faults.fail_next(fault);
        }
        let (sender, receiver) = futures_channel::oneshot::channel();
        let cancellation = beryl_home_store::CommandCancellation::new();
        cx.update(|app| {
            RunningProcessOwner::construct_interrupted_exit_candidate(
                owner,
                request,
                generation,
                cancellation.clone(),
                app,
                move |owner, _| {
                    assert!(!owner.borrow().test_services_on_worker());
                    sender.send(()).unwrap();
                },
            )
            .unwrap();
            assert!(owner.borrow().test_services_on_worker());
            if pass == 0 || pass == 2 {
                cancellation.cancel();
            }
            assert!(
                RunningProcessOwner::abort_constructed_exit_candidate(
                    owner,
                    request,
                    generation,
                    app,
                    |_, _| panic!("abort during construction")
                )
                .is_err()
            );
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_construction_result(request)
                    .is_err()
            );
            assert!(
                RunningProcessOwner::construct_interrupted_exit_candidate(
                    owner,
                    request,
                    generation,
                    beryl_home_store::CommandCancellation::new(),
                    app,
                    |_, _| panic!("overlapping construction")
                )
                .is_err()
            );
            let mut absent = None;
            assert!(
                RunningProcessOwner::settle_interrupted_exit_candidate(
                    owner,
                    request,
                    &mut absent,
                    app,
                    |_, _| panic!("overlapping settlement")
                )
                .is_err()
            );
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(&foreign);
        })
        .unwrap();
        receiver.await.unwrap();
        assert!(
            owner
                .borrow()
                .interrupted_exit_construction_result(request)
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
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        assert!(owner.borrow().test_services().graph().is_none());
        let result = owner.borrow().interrupted_exit_construction_result(request);
        if fault.is_some() {
            assert!(result.is_err());
            let prefix = match fault.unwrap() {
                FaultPoint::BeforeReopen => {
                    "same-home recovery rejected the physical database layout:"
                }
                FaultPoint::AfterReopen => "reopened home persistence barrier failed:",
                _ => unreachable!(),
            };
            assert!(result.as_ref().unwrap_err().starts_with(prefix));
            assert_eq!(
                result,
                owner.borrow().interrupted_exit_construction_result(request)
            );
        } else if pass == 2 {
            assert_eq!(
                result.unwrap_err(),
                "recovery candidate construction was cancelled"
            );
        } else {
            result.unwrap();
        }
        if pass == 3 {
            let (sender, receiver) = futures_channel::oneshot::channel();
            cx.update(|app| {
                for (target, expected) in [(&foreign, generation), (request, stale_generation)] {
                    assert!(
                        RunningProcessOwner::abort_constructed_exit_candidate(
                            owner,
                            target,
                            expected,
                            app,
                            |_, _| panic!("stale abort")
                        )
                        .is_err()
                    );
                    owner
                        .borrow()
                        .interrupted_exit_construction_result(request)
                        .unwrap();
                    assert!(!owner.borrow().test_services_on_worker());
                }
                RunningProcessOwner::abort_constructed_exit_candidate(
                    owner,
                    request,
                    generation,
                    app,
                    move |owner, _| {
                        assert!(!owner.borrow().test_services_on_worker());
                        sender.send(()).unwrap();
                    },
                )
                .unwrap();
                assert!(owner.borrow().test_services_on_worker());
                assert!(
                    RunningProcessOwner::abort_constructed_exit_candidate(
                        owner,
                        request,
                        generation,
                        app,
                        |_, _| panic!("duplicate abort")
                    )
                    .is_err()
                );
                assert!(
                    RunningProcessOwner::construct_interrupted_exit_candidate(
                        owner,
                        request,
                        generation,
                        beryl_home_store::CommandCancellation::new(),
                        app,
                        |_, _| panic!("construct during abort")
                    )
                    .is_err()
                );
                owner
                    .borrow_mut()
                    .test_replace_interrupted_exit_request(&foreign);
            })
            .unwrap();
            receiver.await.unwrap();
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_construction_result(request)
                    .unwrap_err()
                    .contains("request changed")
            );
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(request);
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_construction_result(request)
                    .is_err()
            );
            assert_eq!(
                original,
                format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
            );
            assert!(!RunningProcessOwner::finish_exit(owner, request));
            assert!(owner.borrow().test_services().graph().is_none());
            cx.update(|app| {
                assert!(
                    RunningProcessOwner::abort_constructed_exit_candidate(
                        owner,
                        request,
                        generation,
                        app,
                        |_, _| panic!("abort without candidate")
                    )
                    .is_err()
                );
            })
            .unwrap();
        }
    }
    cx.update(|app| {
        assert!(
            RunningProcessOwner::construct_interrupted_exit_candidate(
                owner,
                request,
                generation,
                beryl_home_store::CommandCancellation::new(),
                app,
                |_, _| panic!("duplicate construction")
            )
            .is_err()
        );
    })
    .unwrap();
    let candidate = owner.borrow().test_take_constructed_exit_candidate();
    assert_ne!(candidate.generation(), generation);
    candidate
}
