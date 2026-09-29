pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    fault: FaultPoint,
    candidate: &mut Option<crate::running_owner::InterruptedExitCandidate>,
    cx: &mut AsyncApp,
) {
    let generation = owner
        .borrow()
        .test_services()
        .graph()
        .unwrap()
        .home()
        .health()
        .generation()
        .unwrap();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let foreign = request.test_foreign();
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        assert!(
            RunningProcessOwner::retire_interrupted_exit_graph(
                owner,
                &foreign,
                generation,
                app,
                |_, _| panic!("foreign retirement"),
            )
            .is_err()
        );
        assert!(!owner.borrow().test_services_on_worker());
        let window = owner.borrow().test_process().windows.shells()[0].window();
        window
            .update(app, |root, _, cx| {
                root.set_shutdown_interaction_gated(false, cx)
            })
            .unwrap()
            .unwrap();
        assert!(
            RunningProcessOwner::retire_interrupted_exit_graph(
                owner,
                request,
                generation,
                app,
                |_, _| panic!("retirement with an ungated original window"),
            )
            .unwrap_err()
            .contains("exact gated shell")
        );
        assert!(!owner.borrow().test_services_on_worker());
        assert!(owner.borrow().test_services().graph().is_some());
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(
            owner
                .borrow()
                .interrupted_exit_graph_retirement_result(request)
                .unwrap_err()
                .contains("has not returned")
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        window
            .update(app, |root, _, cx| {
                root.set_shutdown_interaction_gated(true, cx)
            })
            .unwrap()
            .unwrap();
        RunningProcessOwner::test_retire_interrupted_exit_graph(
            owner,
            request,
            generation,
            app,
            move |owner, _| {
                assert!(!owner.borrow().test_services_on_worker());
                sender.send(()).unwrap();
            },
            move || {
                if matches!(fault, FaultPoint::AfterPersist) {
                    panic!("injected graph retirement unwind");
                }
            },
        )
        .unwrap();
        assert!(owner.borrow().test_services_on_worker());
        assert!(
            owner
                .borrow()
                .interrupted_exit_graph_retirement_result(request)
                .is_err()
        );
        assert!(
            RunningProcessOwner::retire_interrupted_exit_graph(
                owner,
                request,
                generation,
                app,
                |_, _| panic!("overlapping retirement"),
            )
            .is_err()
        );
        assert!(
            RunningProcessOwner::settle_interrupted_exit_candidate(
                owner,
                request,
                candidate,
                app,
                |_, _| panic!("overlapping candidate"),
            )
            .is_err()
        );
        assert!(candidate.is_some());
        owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(&foreign);
    })
    .unwrap();
    receiver.await.unwrap();
    assert!(
        owner
            .borrow()
            .interrupted_exit_graph_retirement_result(request)
            .unwrap_err()
            .contains("request changed")
    );
    owner
        .borrow_mut()
        .test_replace_interrupted_exit_request(request);
    let result = owner
        .borrow()
        .interrupted_exit_graph_retirement_result(request);
    match fault {
        FaultPoint::BeforeCommit => {
            result.unwrap();
            assert!(owner.borrow().test_services().graph().is_none());
        }
        FaultPoint::AfterPersist => {
            assert!(result.unwrap_err().contains("unwound"));
            assert!(owner.borrow().test_services().graph().is_some());
        }
        FaultPoint::AfterCommitBeforePersist => {
            assert!(
                result
                    .unwrap_err()
                    .contains("exact published failed generation")
            );
            assert!(owner.borrow().test_services().graph().is_some());
        }
        _ => unreachable!(),
    }
    assert_eq!(
        original,
        format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
    );
    assert!(!RunningProcessOwner::finish_exit(owner, request));
    cx.update(|app| {
        assert!(
            RunningProcessOwner::retire_interrupted_exit_graph(
                owner,
                request,
                generation,
                app,
                |_, _| panic!("duplicate retirement"),
            )
            .is_err()
        );
    })
    .unwrap();
}
