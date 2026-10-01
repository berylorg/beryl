pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    generation: beryl_home_store::HomeGeneration,
    appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
    delivery: RecoveryPublicationDelivery,
    cx: &mut AsyncApp,
) {
    use beryl_home_store::CommandCancellation;
    use std::{future::Future, task::Poll, time::Duration};
    let foreign = request.test_foreign();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let previous = owner.borrow().test_process_appearance();
    let window = owner.borrow().test_process().windows.shells()[0].window();
    let home = owner
        .borrow()
        .interrupted_exit_appearance(request)
        .unwrap()
        .prepared()
        .home()
        .home_id();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (request, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::attach_and_complete_interrupted_exit_threadless(
                owner,
                request,
                home,
                retired,
                generation,
                window,
                appearance,
                cancellation,
                cx,
            )
            .await
            .is_err()
        );
        assert!(!owner.borrow().test_services_on_worker());
    }

    let cancellation = CommandCancellation::new();
    let mut drive_cx = cx.clone();
    let mut drive = Box::pin(
        RunningProcessOwner::attach_and_complete_interrupted_exit_threadless(
            owner,
            request,
            home,
            retired,
            generation,
            window,
            appearance,
            cancellation.clone(),
            &mut drive_cx,
        ),
    );
    std::future::poll_fn(|task| {
        assert!(drive.as_mut().poll(task).is_pending());
        Poll::Ready(())
    })
    .await;
    assert_reserved(owner, request, retired, generation, appearance, cx).await;
    while owner
        .borrow()
        .interrupted_exit_services_result(request)
        .is_err()
    {
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert_reserved(owner, request, retired, generation, appearance, cx).await;
    assert!(owner.borrow().test_services().graph().is_none());
    assert!(owner.borrow().exit_requested());
    cx.update(|app| {
        assert!(
            owner
                .borrow()
                .validate_interrupted_exit_bindings(request, appearance, app)
                .is_err()
        );
        assert_eq!(
            window.read(app).unwrap().test_exit_presentation().0,
            "Exiting…"
        );
    })
    .unwrap();
    std::future::poll_fn(|task| {
        assert!(drive.as_mut().poll(task).is_pending());
        Poll::Ready(())
    })
    .await;
    assert!(owner.borrow().test_services_on_worker());
    assert_reserved(owner, request, retired, generation, appearance, cx).await;
    while owner.borrow().test_services_on_worker() {
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert_reserved(owner, request, retired, generation, appearance, cx).await;
    owner
        .borrow()
        .interrupted_exit_publication_result(request)
        .unwrap();
    let gate = owner.borrow().test_interrupted_exit_start_gate();
    assert_eq!(owner.borrow().test_process_appearance(), previous);
    assert!(owner.borrow().exit_requested());
    cx.update(|app| {
        assert_eq!(
            window.read(app).unwrap().test_exit_presentation().0,
            "Exiting…"
        );
        assert!(window.read(app).unwrap().test_notices_inert());
    })
    .unwrap();
    match delivery {
        RecoveryPublicationDelivery::DrivenCancelled => cancellation.cancel(),
        RecoveryPublicationDelivery::DrivenStale => owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(&foreign),
        _ => {}
    }
    let result = drive.await;
    if matches!(delivery, RecoveryPublicationDelivery::Driven) {
        result.unwrap();
        assert!(gate.wait());
        assert!(owner.borrow().interrupted_exit_session().is_none());
        assert!(owner.borrow().shutdown_status().is_none());
        assert!(!owner.borrow().exit_requested());
        assert_eq!(owner.borrow().test_process_appearance(), *appearance);
        drop(
            owner
                .borrow()
                .test_services()
                .process
                .execution_permit()
                .reserve()
                .unwrap(),
        );
        cx.update(|app| {
            assert_eq!(window.read(app).unwrap().test_exit_presentation().0, "Exit");
            assert!(!window.read(app).unwrap().test_notices_inert());
        })
        .unwrap();
    } else {
        let error = result.unwrap_err();
        if matches!(delivery, RecoveryPublicationDelivery::DrivenCancelled) {
            assert!(error.contains("cancelled"));
        } else {
            assert!(error.contains("request"));
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(request);
        }
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        owner
            .borrow()
            .interrupted_exit_publication_result(request)
            .unwrap();
        assert!(
            owner
                .borrow()
                .interrupted_exit_theme_activation_result(request)
                .unwrap_err()
                .contains("not settled")
        );
        assert_eq!(owner.borrow().test_process_appearance(), previous);
        assert!(
            owner
                .borrow()
                .test_services()
                .process
                .execution_permit()
                .reserve()
                .is_err()
        );
        assert!(owner.borrow().exit_requested());
        cx.update(|app| {
            owner
                .borrow()
                .validate_interrupted_exit_bindings(request, appearance, app)
                .unwrap();
            assert_eq!(
                window.read(app).unwrap().test_exit_presentation().0,
                "Exiting…"
            );
            assert!(window.read(app).unwrap().test_notices_inert());
        })
        .unwrap();
        drop(owner.borrow().test_take_interrupted_exit_start());
        assert!(!gate.wait());
    }
    assert_eq!(
        owner.borrow().test_process().windows.shells()[0].window(),
        window
    );
    assert!(
        RunningProcessOwner::publish_and_complete_interrupted_exit(
            owner,
            request,
            retired,
            generation,
            appearance,
            CommandCancellation::new(),
            cx,
        )
        .await
        .is_err()
    );
}

async fn assert_reserved(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    generation: beryl_home_store::HomeGeneration,
    appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
    cx: &mut AsyncApp,
) {
    use beryl_home_store::CommandCancellation;
    let on_worker = owner.borrow().test_services_on_worker();
    assert!(
        RunningProcessOwner::publish_and_complete_interrupted_exit(
            owner,
            request,
            retired,
            generation,
            appearance,
            CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap_err()
        .contains("already being driven")
    );
    assert!(
        RunningProcessOwner::await_interrupted_exit_completion(
            owner,
            request,
            CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap_err()
        .contains("already being driven")
    );
    assert!(
        RunningProcessOwner::retry_interrupted_exit_preparation(
            owner,
            request,
            retired,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
            |_| panic!("competing retry"),
            cx,
        )
        .await
        .unwrap_err()
        .contains("already being driven")
    );
    assert_eq!(owner.borrow().test_services_on_worker(), on_worker);
}
