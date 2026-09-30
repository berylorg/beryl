pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    faults: &FaultController,
    delivery: RecoveryPublicationDelivery,
    cx: &mut AsyncApp,
) -> beryl_home_store::HomeRecoveryCandidate {
    use beryl_home_store::CommandCancellation;
    use std::{future::Future, task::Poll, time::Duration};

    let foreign = request.test_foreign();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let window = owner.borrow().test_process().windows.shells()[0].window();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (request, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::construct_and_settle_interrupted_exit(
                owner,
                request,
                retired,
                cancellation,
                cx,
            )
            .await
            .is_err()
        );
        assert!(!owner.borrow().test_services_on_worker());
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
    }
    let cancellation = CommandCancellation::new();
    if matches!(delivery, RecoveryPublicationDelivery::Driven) {
        faults.fail_next(FaultPoint::BeforeReopen);
    }
    let mut drive_cx = cx.clone();
    let mut drive = Box::pin(RunningProcessOwner::construct_and_settle_interrupted_exit(
        owner,
        request,
        retired,
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
        RunningProcessOwner::construct_and_settle_interrupted_exit(
            owner,
            request,
            retired,
            CommandCancellation::new(),
            cx,
        )
        .await
        .is_err()
    );
    while owner.borrow().test_services_on_worker() {
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    if matches!(delivery, RecoveryPublicationDelivery::Driven) {
        assert!(
            owner
                .borrow()
                .interrupted_exit_construction_result(request)
                .is_err()
        );
        assert!(
            owner
                .borrow()
                .interrupted_exit_reopen_deadline(request)
                .unwrap()
                .is_some()
        );
    } else {
        owner
            .borrow()
            .interrupted_exit_construction_result(request)
            .unwrap();
    }
    match delivery {
        RecoveryPublicationDelivery::DrivenCancelled => cancellation.cancel(),
        RecoveryPublicationDelivery::DrivenStale => owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(&foreign),
        _ => {}
    }
    let result = drive.await;
    if matches!(delivery, RecoveryPublicationDelivery::DrivenStale) {
        owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(request);
    }
    assert!(!owner.borrow().test_services_on_worker());
    assert!(owner.borrow().test_services().graph().is_none());
    assert_eq!(
        original,
        format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
    );
    assert!(owner.borrow().exit_requested());
    assert!(!RunningProcessOwner::finish_exit(owner, request));
    cx.update(|app| {
        assert_eq!(
            window.read(app).unwrap().test_exit_presentation().0,
            "Exiting…"
        );
        assert_eq!(
            owner.borrow().test_process().windows.shells()[0].window(),
            window
        );
    })
    .unwrap();
    assert!(
        RunningProcessOwner::construct_and_settle_interrupted_exit(
            owner,
            request,
            retired,
            CommandCancellation::new(),
            cx,
        )
        .await
        .is_err()
    );

    if matches!(delivery, RecoveryPublicationDelivery::Driven) {
        result.unwrap();
        owner
            .borrow()
            .interrupted_exit_candidate_result(request)
            .unwrap();
        let candidate = owner.borrow().test_take_interrupted_exit_candidate();
        assert_ne!(candidate.candidate.generation(), retired);
        assert_eq!(
            candidate.candidate.service_reference().health().state(),
            beryl_home_store::HomeHealthState::Reopening
        );
        drop(candidate.session);
        candidate.candidate
    } else {
        assert!(result.unwrap_err().contains(
            if matches!(delivery, RecoveryPublicationDelivery::DrivenCancelled) {
                "cancelled"
            } else {
                "request changed"
            }
        ));
        owner
            .borrow()
            .interrupted_exit_construction_result(request)
            .unwrap();
        assert!(
            owner
                .borrow()
                .interrupted_exit_candidate_result(request)
                .is_err()
        );
        constructed_settlement::verify(owner, request, retired, cx).await
    }
}
