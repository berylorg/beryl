pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    home: beryl_model::BerylHomeId,
    generation: beryl_home_store::HomeGeneration,
    appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
    attach_ready: bool,
    cx: &mut AsyncApp,
) {
    use beryl_home_store::CommandCancellation;
    use std::{future::Future, task::Poll, time::Duration};
    let window = owner.borrow().test_process().windows.shells()[0].window();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let foreign = request.test_foreign();
    window
        .update(cx, |root, native, app| {
            let mut draft = root.begin_shutdown_draft(native, app).unwrap();
            assert!(root.retire_shutdown_draft(&mut draft, app).unwrap());
            owner
                .borrow_mut()
                .test_replace_recovery_drafts(Some(Rc::new(RefCell::new(
                    crate::running_owner::RunningShutdownDrafts::test_recovery_drafts(
                        window, draft,
                    ),
                ))));
        })
        .unwrap();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (supplied, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::attach_interrupted_exit_threadless_window(
                owner,
                supplied,
                home,
                generation,
                window,
                appearance,
                cancellation,
                cx,
            )
            .await
            .is_err()
        );
        owner
            .borrow()
            .interrupted_exit_services_result(request)
            .unwrap();
    }
    for mode in ["dropped", "cancelled", "stale", "ready"] {
        if mode == "ready" && !attach_ready {
            break;
        }
        let cancellation = CommandCancellation::new();
        let mut drive_cx = cx.clone();
        let mut drive = Box::pin(
            RunningProcessOwner::attach_interrupted_exit_threadless_window(
                owner,
                request,
                home,
                generation,
                window,
                appearance,
                cancellation.clone(),
                &mut drive_cx,
            ),
        );
        std::future::poll_fn(|cx| {
            assert!(drive.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        assert!(
            owner
                .borrow()
                .interrupted_exit_services_result(request)
                .is_err()
        );
        let mut drive = Some(drive);
        if mode == "dropped" {
            drop(drive.take());
        }
        while owner
            .borrow()
            .interrupted_exit_services_result(request)
            .is_err()
        {
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        if mode == "cancelled" {
            cancellation.cancel();
        }
        if mode == "stale" {
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(&foreign);
        }
        if let Some(drive) = drive {
            let result = drive.await;
            match mode {
                "ready" => result.unwrap(),
                "stale" => assert!(result.unwrap_err().contains("request changed")),
                "cancelled" => assert!(result.unwrap_err().contains("cancelled")),
                _ => unreachable!(),
            }
        }
        owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(request);
        owner
            .borrow()
            .interrupted_exit_services_result(request)
            .unwrap();
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(owner.borrow().test_services().graph().is_none());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        cx.update(|app| {
            let valid = owner
                .borrow()
                .validate_interrupted_exit_bindings(request, appearance, app);
            assert_eq!(valid.is_ok(), mode == "ready");
            window
                .update(app, |root, _, _| {
                    assert!(root.controller().unwrap().is_threadless());
                    assert_eq!(root.test_exit_presentation().0, "Exiting…");
                })
                .unwrap();
        })
        .unwrap();
    }
}
