pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    generation: beryl_home_store::HomeGeneration,
    deadline: std::time::Instant,
    previous_delay: Option<u64>,
    cx: &mut AsyncApp,
) -> u64 {
    use beryl_home_store::CommandCancellation;
    use std::{
        future::Future,
        task::Poll,
        time::{Duration, Instant},
    };

    let remaining = deadline.checked_duration_since(Instant::now()).unwrap();
    let seconds = remaining.as_secs() + 1;
    let expected = match previous_delay {
        None => {
            assert!([1, 2, 5].contains(&seconds));
            seconds
        }
        Some(1) => 2,
        Some(2) => 5,
        Some(5) => 10,
        Some(10 | 30) => 30,
        other => panic!("unexpected previous delay: {other:?}"),
    };
    assert_eq!(seconds, expected);
    assert!(remaining > Duration::from_millis(expected * 1000 - 500));
    cx.update(|app| {
        assert_eq!(
            RunningProcessOwner::construct_interrupted_exit_candidate(
                owner,
                request,
                generation,
                CommandCancellation::new(),
                app,
                |_, _| panic!("early preparation retry"),
            )
            .unwrap_err(),
            "Interrupted Exit reopening retry is delayed"
        );
    })
    .unwrap();
    let cancellation = CommandCancellation::new();
    let mut drive_cx = cx.clone();
    let mut drive = Box::pin(RunningProcessOwner::construct_and_settle_interrupted_exit(
        owner,
        request,
        generation,
        cancellation.clone(),
        &mut drive_cx,
    ));
    std::future::poll_fn(|task| {
        assert!(drive.as_mut().poll(task).is_pending());
        Poll::Ready(())
    })
    .await;
    assert!(!owner.borrow().test_services_on_worker());
    if previous_delay.is_none() {
        drop(drive);
    } else {
        cancellation.cancel();
        assert!(drive.await.unwrap_err().contains("cancelled"));
    }
    assert_eq!(
        owner
            .borrow()
            .interrupted_exit_reopen_deadline(request)
            .unwrap(),
        Some(deadline)
    );
    assert!(
        owner
            .borrow_mut()
            .take_interrupted_exit_preparation_failure(&request.test_foreign(), generation)
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
    if previous_delay.is_none() {
        while let Some(delay) = deadline.checked_duration_since(Instant::now()) {
            cx.background_executor().timer(delay).await;
        }
    } else {
        owner
            .borrow_mut()
            .test_expire_interrupted_exit_reopen_deadline(deadline);
    }
    expected
}
