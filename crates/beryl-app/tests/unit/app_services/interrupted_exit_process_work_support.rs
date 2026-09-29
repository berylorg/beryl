pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    candidate: InterruptedExitCandidate,
    cx: &mut AsyncApp,
) -> InterruptedExitCandidate {
    let mut candidate = Some(candidate);
    let foreign = request.test_foreign();
    for (cancel, unwind) in [(false, false), (true, false), (false, true)] {
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            assert!(
                RunningProcessOwner::settle_interrupted_exit_process_work(
                    owner,
                    request,
                    CommandCancellation::new(),
                    app,
                    |_, _| panic!("missing settlement"),
                )
                .is_err()
            );
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
        let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
        for _ in 0..if cancel || unwind { 1 } else { 2 } {
            let cancellation = CommandCancellation::new();
            if cancel {
                cancellation.cancel();
            }
            let (sender, receiver) = futures_channel::oneshot::channel();
            cx.update(|app| {
                assert!(
                    RunningProcessOwner::settle_interrupted_exit_process_work(
                        owner,
                        &foreign,
                        CommandCancellation::new(),
                        app,
                        |_, _| panic!("foreign request"),
                    )
                    .is_err()
                );
                owner
                    .borrow()
                    .test_set_resident_graph_retirement(Some(Err("failed retirement".into())));
                assert_eq!(
                    RunningProcessOwner::settle_interrupted_exit_process_work(
                        owner,
                        request,
                        CommandCancellation::new(),
                        app,
                        |_, _| panic!("failed retirement"),
                    )
                    .unwrap_err(),
                    "failed retirement"
                );
                owner
                    .borrow()
                    .test_set_resident_graph_retirement(Some(Ok(())));
                RunningProcessOwner::test_settle_interrupted_exit_process_work(
                    owner,
                    request,
                    cancellation,
                    app,
                    move |owner, _| {
                        assert!(!owner.borrow().test_services_on_worker());
                        assert!(owner.borrow().interrupted_exit_session().is_some());
                        sender.send(()).unwrap();
                    },
                    move || {
                        if unwind {
                            panic!("injected process work unwind");
                        }
                    },
                )
                .unwrap();
                assert!(owner.borrow().test_services_on_worker());
                assert!(owner.borrow().interrupted_exit_session().is_none());
                assert!(
                    RunningProcessOwner::settle_interrupted_exit_process_work(
                        owner,
                        request,
                        CommandCancellation::new(),
                        app,
                        |_, _| panic!("duplicate work"),
                    )
                    .is_err()
                );
                assert!(
                    RunningProcessOwner::revalidate_interrupted_exit_candidate(
                        owner,
                        request,
                        app,
                        |_, _| panic!("concurrent revalidation"),
                    )
                    .is_err()
                );
                assert!(!RunningProcessOwner::finish_exit(owner, request));
                owner
                    .borrow_mut()
                    .test_replace_interrupted_exit_request(&foreign);
            })
            .unwrap();
            receiver.await.unwrap();
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_candidate_result(request)
                    .unwrap_err()
                    .contains("request changed")
            );
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(request);
            for _ in 0..2 {
                let result = owner.borrow().interrupted_exit_candidate_result(request);
                if cancel {
                    assert!(result.unwrap_err().contains("cancelled"));
                    assert!(
                        owner
                            .borrow()
                            .test_interrupted_exit_process_work_cancelled()
                    );
                } else if unwind {
                    assert!(result.unwrap_err().contains("unwound"));
                } else {
                    result.unwrap();
                }
            }
            if cancel || unwind {
                cx.update(|app| {
                    assert!(
                        RunningProcessOwner::settle_interrupted_exit_process_work(
                            owner,
                            request,
                            CommandCancellation::new(),
                            app,
                            |_, _| panic!("failed outcome retained"),
                        )
                        .unwrap_err()
                        .contains("no successful settlement")
                    );
                    assert!(
                        RunningProcessOwner::revalidate_interrupted_exit_candidate(
                            owner,
                            request,
                            app,
                            |_, _| panic!("failed outcome overwritten"),
                        )
                        .is_err()
                    );
                })
                .unwrap();
            }
            assert_eq!(
                original,
                format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
            );
            assert!(!RunningProcessOwner::finish_exit(owner, request));
        }
        candidate = Some(owner.borrow().test_take_interrupted_exit_candidate());
    }
    candidate.unwrap()
}
