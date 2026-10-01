pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    faults: &FaultController,
    mut previous_delay: u64,
    cx: &mut AsyncApp,
) -> u64 {
    use crate::running_owner::RecoveryPreparationFailure;
    use beryl_home_store::CommandCancellation;
    use std::{
        cell::Cell,
        future::Future,
        task::Poll,
        time::{Duration, Instant},
    };

    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    for (mode, candidate) in [
        ("success", false),
        ("cancel", false),
        ("drop", false),
        ("stale", false),
        ("success", true),
        ("cancel", true),
        ("drop", true),
        ("stale", true),
    ] {
        faults.fail_next(if candidate {
            FaultPoint::BeforeReadConfirmation
        } else {
            FaultPoint::BeforeThemeWatchSpawn
        });
        let delivered = Cell::new(0);
        let cancellation = CommandCancellation::new();
        let mut drive_cx = cx.clone();
        let mut drive = Box::pin(RunningProcessOwner::retry_interrupted_exit_preparation(
            owner,
            request,
            retired,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            cancellation.clone(),
            |failure| {
                assert!(!owner.borrow().test_services_on_worker());
                assert!(owner.borrow().test_services().graph().is_none());
                assert!(!RunningProcessOwner::finish_exit(owner, request));
                match failure {
                    RecoveryPreparationFailure::Candidate(failure) => {
                        assert!(candidate && delivered.get() == 0);
                        assert!(!failure.to_string().is_empty());
                        assert!(
                            owner
                                .borrow_mut()
                                .take_interrupted_exit_candidate_failure(request)
                                .is_err()
                        );
                    }
                    RecoveryPreparationFailure::Services(failure) => {
                        assert!(!candidate || delivered.get() == 1);
                        let crate::app_services::recovery_graph::RecoveryServicePreparationError::App(failure) = failure else {
                                panic!("unexpected preparation failure");
                            };
                        assert!(matches!(failure.error(), AppServiceOpenError::Theme(_)));
                        assert!(failure.into_retry_parts().is_err());
                    }
                    _ => panic!("unexpected resume failure"),
                }
                assert!(
                    owner
                        .borrow_mut()
                        .take_interrupted_exit_preparation_failure(request, retired)
                        .is_err()
                );
                delivered.set(delivered.get() + 1);
                if mode == "success" && delivered.get() == 1 {
                    faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
                }
                if mode == "stale" {
                    owner
                        .borrow_mut()
                        .test_replace_interrupted_exit_request(&request.test_foreign());
                }
            },
            &mut drive_cx,
        ));
        let timeout = Instant::now() + Duration::from_secs(5);
        let expected = if mode == "success" { 2 } else { 1 };
        for count in 1..=expected {
            loop {
                let result =
                    std::future::poll_fn(|task| Poll::Ready(drive.as_mut().poll(task))).await;
                if mode == "stale" && delivered.get() == 1 {
                    assert!(
                        matches!(result, Poll::Ready(Err(ref error)) if error.contains("request changed"))
                    );
                    break;
                }
                assert!(result.is_pending(), "{mode}: {result:?}");
                if delivered.get() == count {
                    break;
                }
                assert!(
                    Instant::now() < timeout,
                    "{mode}: preparation retry did not deliver"
                );
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
            }
            if mode == "stale" {
                owner
                    .borrow_mut()
                    .test_replace_interrupted_exit_request(request);
            }
            assert!(!owner.borrow().test_services_on_worker());
            let deadline = owner
                .borrow()
                .interrupted_exit_reopen_deadline(request)
                .unwrap()
                .unwrap();
            let expected_delay = match previous_delay {
                1 => 2,
                2 => 5,
                5 => 10,
                10 | 30 => 30,
                other => panic!("unexpected delay {other}"),
            };
            let remaining = deadline.checked_duration_since(Instant::now()).unwrap();
            assert!(remaining > Duration::from_millis(expected_delay * 1000 - 500));
            assert!(remaining <= Duration::from_secs(expected_delay));
            previous_delay = expected_delay;
            if mode != "stale" {
                let competing = RunningProcessOwner::retry_interrupted_exit_preparation(
                    owner,
                    request,
                    retired,
                    configuration(),
                    SyndicTimestamp::from_unix_millis(2),
                    CommandCancellation::new(),
                    |_| panic!("competing driver received failure"),
                    cx,
                )
                .await
                .unwrap_err();
                assert!(competing.contains("already being driven"));
                assert!(!owner.borrow().test_services_on_worker());
                assert_eq!(
                    owner
                        .borrow()
                        .interrupted_exit_reopen_deadline(request)
                        .unwrap(),
                    Some(deadline)
                );
            }
            if mode == "success" {
                owner
                    .borrow_mut()
                    .test_expire_interrupted_exit_reopen_deadline(deadline);
            }
        }
        match mode {
            "success" => {
                drive.await.unwrap();
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
                    .unwrap()
                })
                .unwrap();
                receiver.await.unwrap();
                owner
                    .borrow_mut()
                    .take_interrupted_exit_preparation_failure(request, retired)
                    .unwrap();
                let deadline = owner
                    .borrow()
                    .interrupted_exit_reopen_deadline(request)
                    .unwrap()
                    .unwrap();
                previous_delay = retry_delay::verify(
                    owner,
                    request,
                    retired,
                    deadline,
                    Some(previous_delay),
                    cx,
                )
                .await;
            }
            "cancel" => {
                let deadline = owner
                    .borrow()
                    .interrupted_exit_reopen_deadline(request)
                    .unwrap();
                cancellation.cancel();
                assert!(drive.await.unwrap_err().contains("cancelled"));
                assert_eq!(
                    owner
                        .borrow()
                        .interrupted_exit_reopen_deadline(request)
                        .unwrap(),
                    deadline
                );
                owner
                    .borrow_mut()
                    .test_expire_interrupted_exit_reopen_deadline(deadline.unwrap());
            }
            "drop" | "stale" => {
                drop(drive);
                let deadline = owner
                    .borrow()
                    .interrupted_exit_reopen_deadline(request)
                    .unwrap()
                    .unwrap();
                owner
                    .borrow_mut()
                    .test_expire_interrupted_exit_reopen_deadline(deadline);
            }
            _ => unreachable!(),
        }
        assert_eq!(delivered.get(), expected);
        assert!(!owner.borrow().test_services_on_worker());
        assert!(owner.borrow().test_services().graph().is_none());
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
    }
    previous_delay
}
