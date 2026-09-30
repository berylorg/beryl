pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    generation: beryl_home_store::HomeGeneration,
    cancellation: beryl_home_store::CommandCancellation,
    mode: &str,
    cx: &mut AsyncApp,
) {
    use beryl_home_store::CommandCancellation;
    use std::{future::Future, task::Poll, time::Duration};

    let foreign = request.test_foreign();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (request, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::prepare_interrupted_exit_service_graph(
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
        assert!(owner.borrow().interrupted_exit_session().is_some());
    }
    for retirement in [None, Some(Err("failed retirement".into()))] {
        owner
            .borrow()
            .test_set_resident_graph_retirement(retirement);
        assert!(
            RunningProcessOwner::prepare_interrupted_exit_service_graph(
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
    }
    owner
        .borrow()
        .test_set_resident_graph_retirement(Some(Ok(())));
    let mut drive_cx = cx.clone();
    let mut drive = Box::pin(RunningProcessOwner::prepare_interrupted_exit_service_graph(
        owner,
        request,
        generation,
        configuration(),
        SyndicTimestamp::from_unix_millis(2),
        cancellation,
        &mut drive_cx,
    ));
    std::future::poll_fn(|cx| {
        assert!(drive.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    assert!(owner.borrow().test_services_on_worker());
    assert!(owner.borrow().interrupted_exit_session().is_none());
    assert!(
        owner
            .borrow()
            .interrupted_exit_services_result(request)
            .is_err()
    );
    assert!(
        owner
            .borrow_mut()
            .take_interrupted_exit_preparation_failure(request, generation)
            .is_err()
    );
    assert!(
        RunningProcessOwner::prepare_interrupted_exit_service_graph(
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
    assert!(!RunningProcessOwner::finish_exit(owner, request));

    let mut drive = Some(drive);
    if mode == "dropped" {
        drop(drive.take());
    }
    if mode == "success" {
        owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(&foreign);
    }
    while owner.borrow().test_services_on_worker() {
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert!(owner.borrow().interrupted_exit_session().is_some());
    assert!(owner.borrow().test_services().graph().is_none());
    assert!(!RunningProcessOwner::finish_exit(owner, request));
    if let Some(drive) = drive {
        let result = drive.await;
        if mode == "publish" {
            result.unwrap();
        } else if mode == "success" {
            assert!(result.unwrap_err().contains("request changed"));
        } else {
            assert!(result.is_err());
        }
    }
    owner
        .borrow_mut()
        .test_replace_interrupted_exit_request(&foreign);
}
