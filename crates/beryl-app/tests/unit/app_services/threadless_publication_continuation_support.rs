pub(super) async fn assert_refused(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    cx: &mut AsyncApp,
) {
    assert!(
        RunningProcessOwner::recover_published_interrupted_exit(
            owner,
            request,
            CommandCancellation::new(),
            cx,
        )
        .await
        .is_err()
    );
    assert!(
        RunningProcessOwner::bind_and_complete_interrupted_exit_threadless(
            owner,
            request,
            CommandCancellation::new(),
            cx,
        )
        .await
        .is_err()
    );
}

pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    home: beryl_model::BerylHomeId,
    window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
    retired: beryl_home_store::HomeGeneration,
    generation: beryl_home_store::HomeGeneration,
    cx: &mut AsyncApp,
) {
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let previous = owner.borrow().test_process_appearance();
    let mut drive_cx = cx.clone();
    let mut drive = Box::pin(RunningProcessOwner::continue_interrupted_exit_threadless(
        owner,
        request,
        home,
        retired,
        generation,
        window,
        CommandCancellation::new(),
        &mut drive_cx,
    ));
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            drive
                .as_mut()
                .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
                .is_pending()
        );
        assert_refused(owner, request, cx).await;
        if owner.borrow().test_services_on_worker() {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    drop(drive);
    while owner.borrow().test_services_on_worker() {
        assert!(std::time::Instant::now() < deadline);
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    owner
        .borrow()
        .interrupted_exit_publication_result(request)
        .unwrap();
    let foreign = request.test_foreign();
    assert_refused(owner, &foreign, cx).await;
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(
        RunningProcessOwner::recover_published_interrupted_exit(owner, request, cancelled, cx,)
            .await
            .is_err()
    );
    let mut activation_cx = cx.clone();
    let mut activation = Box::pin(RunningProcessOwner::recover_published_interrupted_exit(
        owner,
        request,
        CommandCancellation::new(),
        &mut activation_cx,
    ));
    assert!(
        activation
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
            .is_pending()
    );
    assert_refused(owner, request, cx).await;
    drop(activation);
    assert_refused(owner, request, cx).await;
    while owner.borrow().test_services_on_worker() {
        assert!(std::time::Instant::now() < deadline);
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    owner
        .borrow()
        .interrupted_exit_theme_activation_result(request)
        .unwrap();
    assert_refused(owner, &foreign, cx).await;
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(
        RunningProcessOwner::bind_and_complete_interrupted_exit_threadless(
            owner, request, cancelled, cx,
        )
        .await
        .is_err()
    );
    assert_eq!(
        original,
        format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
    );
    assert_eq!(previous, owner.borrow().test_process_appearance());
    assert!(owner.borrow().exit_requested());
    RunningProcessOwner::bind_and_complete_interrupted_exit_threadless(
        owner,
        request,
        CommandCancellation::new(),
        cx,
    )
    .await
    .unwrap();
}
