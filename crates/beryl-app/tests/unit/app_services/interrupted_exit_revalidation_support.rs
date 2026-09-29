pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    candidate: InterruptedExitCandidate,
    cx: &mut AsyncApp,
) -> InterruptedExitCandidate {
    let mut candidate = Some(candidate);
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        assert!(
            RunningProcessOwner::revalidate_interrupted_exit_candidate(
                owner,
                request,
                app,
                |_, _| panic!("no settlement"),
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
    let foreign = request.test_foreign();
    for unwind in [false, true] {
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            assert!(
                RunningProcessOwner::revalidate_interrupted_exit_candidate(
                    owner,
                    &foreign,
                    app,
                    |_, _| panic!("foreign revalidation"),
                )
                .unwrap_err()
                .contains("request changed")
            );
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Err("failed retirement".into())));
            assert_eq!(
                RunningProcessOwner::revalidate_interrupted_exit_candidate(
                    owner,
                    request,
                    app,
                    |_, _| panic!("failed retirement"),
                )
                .unwrap_err(),
                "failed retirement"
            );
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Ok(())));
            RunningProcessOwner::test_revalidate_interrupted_exit_candidate(
                owner,
                request,
                app,
                move |_, _| {
                    sender.send(()).unwrap();
                },
                move |_| {
                    if unwind {
                        panic!("injected revalidation unwind");
                    }
                },
            )
            .unwrap();
            assert!(owner.borrow().interrupted_exit_session().is_none());
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_candidate_result(request)
                    .is_err()
            );
            assert!(
                RunningProcessOwner::revalidate_interrupted_exit_candidate(
                    owner,
                    request,
                    app,
                    |_, _| panic!("duplicate revalidation"),
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
        let result = owner.borrow().interrupted_exit_candidate_result(request);
        if unwind {
            assert!(result.unwrap_err().contains("unwound"));
            cx.update(|app| {
                assert!(
                    RunningProcessOwner::revalidate_interrupted_exit_candidate(
                        owner,
                        request,
                        app,
                        |_, _| panic!("failed settlement"),
                    )
                    .unwrap_err()
                    .contains("no successful settlement")
                );
            })
            .unwrap();
        } else {
            result.unwrap();
        }
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
    }
    owner.borrow().test_take_interrupted_exit_candidate()
}
