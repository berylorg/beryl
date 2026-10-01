pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    generation: beryl_home_store::HomeGeneration,
    expected: &str,
    abandon: bool,
    cx: &mut AsyncApp,
) {
    use std::{
        future::Future,
        task::Poll,
        time::{Duration, Instant},
    };

    assert!(
        RunningProcessOwner::dispose_and_take_interrupted_exit_candidate_failure(
            owner,
            &request.test_foreign(),
            generation,
            cx,
        )
        .await
        .is_err()
    );
    assert!(!owner.borrow().test_services_on_worker());
    let mut drive_cx = cx.clone();
    let mut drive = Box::pin(
        RunningProcessOwner::dispose_and_take_interrupted_exit_candidate_failure(
            owner,
            request,
            generation,
            &mut drive_cx,
        ),
    );
    std::future::poll_fn(|task| {
        assert!(drive.as_mut().poll(task).is_pending());
        Poll::Ready(())
    })
    .await;
    assert!(owner.borrow().test_services_on_worker());
    assert!(
        owner
            .borrow_mut()
            .take_interrupted_exit_candidate_failure(request)
            .is_err()
    );
    assert!(
        owner
            .borrow()
            .interrupted_exit_reopen_deadline(request)
            .unwrap()
            .is_none()
    );
    assert!(
        RunningProcessOwner::dispose_and_take_interrupted_exit_candidate_failure(
            owner, request, generation, cx,
        )
        .await
        .is_err()
    );
    let failure = if abandon {
        drop(drive);
        let timeout = Instant::now() + Duration::from_secs(10);
        while owner.borrow().test_services_on_worker() {
            assert!(
                Instant::now() < timeout,
                "candidate disposal did not return"
            );
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        assert_eq!(
            owner
                .borrow()
                .interrupted_exit_candidate_result(request)
                .unwrap_err(),
            expected
        );
        assert!(
            owner
                .borrow_mut()
                .take_interrupted_exit_candidate_failure(&request.test_foreign())
                .is_err()
        );
        owner
            .borrow_mut()
            .take_interrupted_exit_candidate_failure(request)
            .unwrap()
    } else {
        drive.await.unwrap()
    };
    assert_eq!(failure.to_string(), expected);
    assert!(!owner.borrow().test_services_on_worker());
    let deadline = owner
        .borrow()
        .interrupted_exit_reopen_deadline(request)
        .unwrap()
        .unwrap();
    assert!(deadline <= Instant::now() + Duration::from_secs(1));
    assert!(
        owner
            .borrow_mut()
            .take_interrupted_exit_candidate_failure(request)
            .is_err()
    );
    assert!(
        RunningProcessOwner::dispose_and_take_interrupted_exit_candidate_failure(
            owner, request, generation, cx,
        )
        .await
        .is_err()
    );
    assert_eq!(
        owner
            .borrow()
            .interrupted_exit_reopen_deadline(request)
            .unwrap(),
        Some(deadline)
    );
    assert!(!RunningProcessOwner::finish_exit(owner, request));
}
