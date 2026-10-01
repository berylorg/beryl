#[derive(Clone, Copy)]
pub(super) enum DisposalWait {
    Complete,
    AbandonPending,
    AbandonDelivered,
}

pub(super) async fn prepare_failure(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    generation: beryl_home_store::HomeGeneration,
    wait: DisposalWait,
    cx: &mut AsyncApp,
) -> String {
    use std::{
        future::Future,
        task::Poll,
        time::{Duration, Instant},
    };

    let mut drive_cx = cx.clone();
    let mut drive = Box::pin(RunningProcessOwner::retire_and_prepare_interrupted_exit(
        owner,
        request,
        generation,
        configuration(),
        SyndicTimestamp::from_unix_millis(2),
        CommandCancellation::new(),
        &mut drive_cx,
    ));
    if matches!(wait, DisposalWait::Complete) {
        return drive.await.unwrap_err();
    }
    let timeout = Instant::now() + Duration::from_secs(10);
    loop {
        if matches!(wait, DisposalWait::AbandonPending)
            && owner.borrow().test_services_on_worker()
            && owner
                .borrow()
                .interrupted_exit_session()
                .is_some_and(|session| matches!(&*session, RunningShutdownSession::Resuming(_)))
        {
            break;
        }
        // Stop polling after the disposal worker returns but before the waiter consumes delivery.
        if owner
            .borrow()
            .interrupted_exit_reopen_deadline(request)
            .is_ok_and(|deadline| deadline.is_some())
        {
            break;
        }
        assert!(
            Instant::now() < timeout,
            "composed candidate disposal did not return"
        );
        std::future::poll_fn(|task| {
            assert!(drive.as_mut().poll(task).is_pending());
            Poll::Ready(())
        })
        .await;
        if matches!(wait, DisposalWait::AbandonPending)
            && owner.borrow().test_services_on_worker()
            && owner
                .borrow()
                .interrupted_exit_session()
                .is_some_and(|session| matches!(&*session, RunningShutdownSession::Resuming(_)))
        {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    drop(drive);
    while owner.borrow().test_services_on_worker() {
        assert!(
            Instant::now() < timeout,
            "abandoned candidate disposal did not return"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert!(!owner.borrow().test_services_on_worker());
    owner
        .borrow()
        .interrupted_exit_candidate_result(request)
        .unwrap_err()
}

pub(super) async fn verify_retained_failure(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    generation: beryl_home_store::HomeGeneration,
    expected: &str,
    cx: &mut AsyncApp,
) {
    assert!(!owner.borrow().test_services_on_worker());
    let deadline = owner
        .borrow()
        .interrupted_exit_reopen_deadline(request)
        .unwrap()
        .unwrap();
    assert!(deadline <= std::time::Instant::now() + std::time::Duration::from_secs(1));
    let foreign = request.test_foreign();
    for attempted in [&foreign, request] {
        assert!(
            RunningProcessOwner::dispose_and_take_interrupted_exit_candidate_failure(
                owner, attempted, generation, cx,
            )
            .await
            .is_err()
        );
    }
    assert!(
        owner
            .borrow_mut()
            .take_interrupted_exit_candidate_failure(&request.test_foreign())
            .is_err()
    );
    assert_eq!(
        owner
            .borrow()
            .interrupted_exit_candidate_result(request)
            .unwrap_err(),
        expected
    );
    let failure = owner
        .borrow_mut()
        .take_interrupted_exit_candidate_failure(request)
        .unwrap();
    assert_eq!(failure.to_string(), expected);
    assert!(
        owner
            .borrow_mut()
            .take_interrupted_exit_candidate_failure(request)
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
