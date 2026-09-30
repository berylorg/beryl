pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    generation: beryl_home_store::HomeGeneration,
    stale_generation: beryl_home_store::HomeGeneration,
    delivery: RetirementDelivery,
    cx: &mut AsyncApp,
) -> beryl_home_store::HomeRecoveryCandidate {
    use beryl_home_store::CommandCancellation;
    use std::{future::Future, task::Poll, time::Duration};

    let foreign = request.test_foreign();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let window = owner.borrow().test_process().windows.shells()[0].window();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (request, generation, cancellation) in [
        (&foreign, generation, CommandCancellation::new()),
        (request, stale_generation, CommandCancellation::new()),
        (request, generation, cancelled),
    ] {
        assert!(
            RunningProcessOwner::retire_and_prepare_interrupted_exit(
                owner,
                request,
                generation,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                cancellation,
                cx,
            )
            .await
            .is_err()
        );
        assert!(!owner.borrow().test_services_on_worker());
        assert!(owner.borrow().test_services().graph().is_some());
        cx.update(|app| {
            assert!(!window.read(app).unwrap().test_shell_construction_retired());
        })
        .unwrap();
    }

    let cancellation = CommandCancellation::new();
    let mut drive_cx = cx.clone();
    let mut drive = Box::pin(RunningProcessOwner::retire_and_prepare_interrupted_exit(
        owner,
        request,
        generation,
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
    assert!(owner.borrow().test_services_on_worker());
    assert!(
        RunningProcessOwner::retire_and_prepare_interrupted_exit(
            owner,
            request,
            generation,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
            cx,
        )
        .await
        .is_err()
    );
    let mut drive = Some(drive);
    if matches!(delivery, RetirementDelivery::Dropped) {
        drop(drive.take());
    }
    while owner.borrow().test_services_on_worker() {
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    owner
        .borrow()
        .interrupted_exit_graph_retirement_result(request)
        .unwrap();
    assert!(owner.borrow().test_services().graph().is_none());
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
    assert!(owner.borrow().exit_requested());
    assert!(!RunningProcessOwner::finish_exit(owner, request));
    cx.update(|app| {
        let root = window.read(app).unwrap();
        assert!(root.test_shell_construction_retired());
        assert_eq!(root.test_exit_presentation().0, "Exiting…");
        assert_eq!(
            owner.borrow().test_process().windows.shells()[0].window(),
            window
        );
    })
    .unwrap();

    match delivery {
        RetirementDelivery::Cancelled => cancellation.cancel(),
        RetirementDelivery::Stale => owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(&foreign),
        _ => {}
    }
    if let Some(drive) = drive {
        let result = drive.await;
        match delivery {
            RetirementDelivery::Ready => result.unwrap(),
            RetirementDelivery::Cancelled => assert!(result.unwrap_err().contains("cancelled")),
            RetirementDelivery::Stale => {
                assert!(result.unwrap_err().contains("request changed"));
                owner
                    .borrow_mut()
                    .test_replace_interrupted_exit_request(request);
            }
            RetirementDelivery::Dropped => unreachable!(),
        }
    }
    assert!(
        RunningProcessOwner::retire_and_prepare_interrupted_exit(
            owner,
            request,
            generation,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
            cx,
        )
        .await
        .is_err()
    );
    if !matches!(delivery, RetirementDelivery::Ready) {
        assert!(!owner.borrow().test_services_on_worker());
        owner
            .borrow()
            .interrupted_exit_graph_retirement_result(request)
            .unwrap();
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
    } else {
        owner
            .borrow()
            .interrupted_exit_services_result(request)
            .unwrap();
        assert!(!owner.borrow().test_services_on_worker());
        assert!(owner.borrow().test_services().graph().is_none());
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(owner.borrow().exit_requested());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        cx.update(|app| {
            let root = window.read(app).unwrap();
            assert_eq!(root.test_exit_presentation().0, "Exiting…");
        })
        .unwrap();
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            RunningProcessOwner::cancel_interrupted_exit_services(
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
        assert!(!owner.borrow().test_services_on_worker());
        let failure = owner
            .borrow_mut()
            .take_interrupted_exit_preparation_failure(request, generation)
            .unwrap();
        assert!(matches!(
            failure,
            crate::app_services::recovery_graph::RecoveryServicePreparationError::App(_)
        ));
    }
    RunningProcessOwner::construct_and_settle_interrupted_exit(
        owner,
        request,
        generation,
        CommandCancellation::new(),
        cx,
    )
    .await
    .unwrap();
    owner
        .borrow()
        .interrupted_exit_candidate_result(request)
        .unwrap();
    let candidate = owner.borrow().test_take_interrupted_exit_candidate();
    assert_ne!(candidate.candidate.generation(), generation);
    drop(candidate.session);
    candidate.candidate
}
