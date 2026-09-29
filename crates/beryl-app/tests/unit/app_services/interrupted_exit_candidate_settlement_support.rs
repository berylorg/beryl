pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    mut candidate: Option<crate::running_owner::InterruptedExitCandidate>,
    unwind: bool,
    cx: &mut AsyncApp,
) -> crate::running_owner::InterruptedExitCandidate {
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let foreign = request.test_foreign();
    let retirement = owner
        .borrow()
        .interrupted_exit_graph_retirement_result(request);
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        assert!(
            RunningProcessOwner::settle_interrupted_exit_candidate(
                owner,
                &foreign,
                &mut candidate,
                app,
                |_, _| panic!("foreign admission"),
            )
            .unwrap_err()
            .contains("request changed")
        );
        assert!(candidate.is_some());
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        if let Err(error) = &retirement {
            assert_eq!(
                RunningProcessOwner::settle_interrupted_exit_candidate(
                    owner,
                    request,
                    &mut candidate,
                    app,
                    |_, _| panic!("settlement after failed retirement"),
                )
                .unwrap_err(),
                *error,
            );
            assert!(candidate.is_some());
            assert_eq!(
                original,
                format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
            );
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_candidate_result(request)
                    .is_err()
            );
            assert!(!RunningProcessOwner::finish_exit(owner, request));
            return;
        }
        RunningProcessOwner::test_settle_interrupted_exit_candidate(
            owner,
            request,
            &mut candidate,
            app,
            move |_, _| {
                sender.send(()).unwrap();
            },
            move || {
                if unwind {
                    panic!("injected settlement unwind");
                }
            },
        )
        .unwrap();
        assert!(candidate.is_none());
        assert!(owner.borrow().interrupted_exit_session().is_none());
        assert!(
            owner
                .borrow()
                .interrupted_exit_candidate_result(request)
                .is_err()
        );
        assert!(
            RunningProcessOwner::settle_interrupted_exit_candidate(
                owner,
                request,
                &mut candidate,
                app,
                |_, _| panic!("duplicate settlement"),
            )
            .is_err()
        );
        owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(&foreign);
    })
    .unwrap();
    if retirement.is_err() {
        return candidate.unwrap();
    }
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
    let result = owner
        .borrow()
        .interrupted_exit_candidate_result(request)
        .unwrap_err();
    assert!(result.contains(if unwind {
        "unwound"
    } else {
        "different configured home"
    }));
    assert_eq!(
        original,
        format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
    );
    owner.borrow().test_take_interrupted_exit_candidate()
}
