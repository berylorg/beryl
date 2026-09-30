pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    generation: beryl_home_store::HomeGeneration,
    pass: usize,
    cx: &mut AsyncApp,
) {
    use beryl_home_store::CommandCancellation;
    use std::{
        future::Future,
        task::Poll,
        time::{Duration, Instant},
    };

    let deadline = owner
        .borrow()
        .interrupted_exit_reopen_deadline(request)
        .unwrap()
        .unwrap();
    let remaining = deadline.checked_duration_since(Instant::now()).unwrap();
    assert!(remaining <= Duration::from_secs(if pass == 0 { 1 } else { 2 }));
    assert!(remaining > Duration::from_millis(if pass == 0 { 500 } else { 1500 }));
    cx.update(|app| {
        assert_eq!(
            RunningProcessOwner::construct_interrupted_exit_candidate(
                owner,
                request,
                generation,
                CommandCancellation::new(),
                app,
                |_, _| panic!("early retry"),
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
    if pass == 0 {
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
    let foreign = request.test_foreign();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (request, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::construct_and_settle_interrupted_exit(
                owner,
                request,
                generation,
                cancellation,
                cx,
            )
            .await
            .is_err()
        );
        assert!(!owner.borrow().test_services_on_worker());
    }
    assert_eq!(
        owner
            .borrow()
            .interrupted_exit_reopen_deadline(request)
            .unwrap(),
        Some(deadline)
    );
    while let Some(delay) = deadline.checked_duration_since(Instant::now()) {
        cx.background_executor().timer(delay).await;
    }
}
