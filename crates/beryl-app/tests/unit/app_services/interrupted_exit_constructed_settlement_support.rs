pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    generation: beryl_home_store::HomeGeneration,
    cx: &mut AsyncApp,
) -> beryl_home_store::HomeRecoveryCandidate {
    let foreign = request.test_foreign();
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        assert!(
            RunningProcessOwner::settle_constructed_exit_candidate(
                owner,
                &foreign,
                app,
                |_, _| panic!("foreign settlement")
            )
            .is_err()
        );
        for retirement in [None, Some(Err("failed retirement".into()))] {
            owner
                .borrow()
                .test_set_resident_graph_retirement(retirement);
            assert!(
                RunningProcessOwner::settle_constructed_exit_candidate(
                    owner,
                    request,
                    app,
                    |_, _| panic!("unproven retirement")
                )
                .is_err()
            );
        }
        owner
            .borrow()
            .test_set_resident_graph_retirement(Some(Ok(())));
        owner
            .borrow()
            .interrupted_exit_construction_result(request)
            .unwrap();
        assert!(owner.borrow().interrupted_exit_session().is_some());
        RunningProcessOwner::settle_constructed_exit_candidate(
            owner,
            request,
            app,
            move |owner, _| {
                assert!(owner.borrow().interrupted_exit_session().is_some());
                assert!(!owner.borrow().test_services_on_worker());
                sender.send(()).unwrap();
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
            RunningProcessOwner::settle_constructed_exit_candidate(
                owner,
                request,
                app,
                |_, _| panic!("duplicate settlement")
            )
            .is_err()
        );
        assert!(
            RunningProcessOwner::abort_constructed_exit_candidate(
                owner,
                request,
                generation,
                app,
                |_, _| panic!("abort during settlement")
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
                |_, _| panic!("construct during settlement")
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
            .interrupted_exit_candidate_result(request)
            .unwrap_err()
            .contains("request changed")
    );
    owner
        .borrow_mut()
        .test_replace_interrupted_exit_request(request);
    owner
        .borrow()
        .interrupted_exit_candidate_result(request)
        .unwrap();
    assert!(!RunningProcessOwner::finish_exit(owner, request));
    assert!(owner.borrow().test_services().graph().is_none());
    cx.update(|app| {
        assert!(
            RunningProcessOwner::settle_constructed_exit_candidate(
                owner,
                request,
                app,
                |_, _| panic!("repeat completed settlement")
            )
            .is_err()
        );
    })
    .unwrap();
    let candidate = owner.borrow().test_take_interrupted_exit_candidate();
    assert_eq!(
        candidate.candidate.service_reference().health().state(),
        beryl_home_store::HomeHealthState::Reopening
    );
    drop(candidate.session);
    candidate.candidate
}
