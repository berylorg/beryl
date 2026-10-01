pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    stale: beryl_home_store::HomeGeneration,
    faults: &FaultController,
    mut previous_delay: u64,
    cx: &mut AsyncApp,
) -> u64 {
    use beryl_home_store::CommandCancellation;

    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    previous_delay =
        preparation_cancellation::verify(owner, request, retired, previous_delay, cx).await;
    let deadline = owner
        .borrow()
        .interrupted_exit_reopen_deadline(request)
        .unwrap();
    let foreign = request.test_foreign();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (request, generation, cancellation) in [
        (&foreign, retired, CommandCancellation::new()),
        (request, stale, CommandCancellation::new()),
        (request, retired, cancelled),
    ] {
        assert!(
            RunningProcessOwner::prepare_retired_interrupted_exit(
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
    }
    for retirement in [None, Some(Err("failed retirement".into()))] {
        owner
            .borrow()
            .test_set_resident_graph_retirement(retirement);
        assert!(
            RunningProcessOwner::prepare_retired_interrupted_exit(
                owner,
                request,
                retired,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                CommandCancellation::new(),
                cx,
            )
            .await
            .is_err()
        );
    }
    owner
        .borrow()
        .test_set_resident_graph_retirement(Some(Ok(())));
    assert_eq!(
        owner
            .borrow()
            .interrupted_exit_reopen_deadline(request)
            .unwrap(),
        deadline
    );

    for fail in [true, false] {
        if fail {
            faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
        }
        let result = RunningProcessOwner::prepare_retired_interrupted_exit(
            owner,
            request,
            retired,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
            cx,
        )
        .await;
        assert!(!owner.borrow().test_services_on_worker());
        assert!(owner.borrow().test_services().graph().is_none());
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        if fail {
            assert!(result.is_err());
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_reopen_deadline(request)
                    .unwrap()
                    .is_some()
            );
        } else {
            result.unwrap();
            owner
                .borrow()
                .interrupted_exit_services_result(request)
                .unwrap();
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_reopen_deadline(request)
                    .unwrap()
                    .is_none()
            );
        }
        let evidence = owner.borrow().interrupted_exit_services_result(request);
        let retained_deadline = owner
            .borrow()
            .interrupted_exit_reopen_deadline(request)
            .unwrap();
        assert!(
            RunningProcessOwner::prepare_retired_interrupted_exit(
                owner,
                request,
                retired,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                CommandCancellation::new(),
                cx,
            )
            .await
            .unwrap_err()
            .contains("already retained")
        );
        assert_eq!(
            owner.borrow().interrupted_exit_services_result(request),
            evidence
        );
        assert_eq!(
            owner
                .borrow()
                .interrupted_exit_reopen_deadline(request)
                .unwrap(),
            retained_deadline
        );
        if !fail {
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
        }
        let failure = owner
            .borrow_mut()
            .take_interrupted_exit_preparation_failure(request, retired)
            .unwrap();
        match failure {
            crate::app_services::recovery_graph::RecoveryServicePreparationError::App(failure) => {
                if fail {
                    assert!(matches!(failure.error(), AppServiceOpenError::Theme(_)));
                } else {
                    assert!(matches!(failure.error(), AppServiceOpenError::Cancelled));
                }
                assert!(failure.into_retry_parts().is_err());
            }
            error => panic!("unexpected preparation failure: {error:?}"),
        }
        let deadline = owner
            .borrow()
            .interrupted_exit_reopen_deadline(request)
            .unwrap()
            .unwrap();
        previous_delay =
            retry_delay::verify(owner, request, retired, deadline, Some(previous_delay), cx).await;
    }
    previous_delay
}
