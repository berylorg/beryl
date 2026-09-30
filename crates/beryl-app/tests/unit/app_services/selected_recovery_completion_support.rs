pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    generation: beryl_home_store::HomeGeneration,
    appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
    cx: &mut AsyncApp,
) {
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        RunningProcessOwner::publish_interrupted_exit_services(
            owner,
            request,
            retired,
            generation,
            appearance,
            CommandCancellation::new(),
            app,
            move |_, result, _| {
                let _ = sender.send(result);
            },
        )
    })
    .unwrap()
    .unwrap();
    receiver.await.unwrap().unwrap();
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        RunningProcessOwner::activate_interrupted_exit_theme(
            owner,
            request,
            appearance,
            CommandCancellation::new(),
            app,
            move |_, result, _| {
                let _ = sender.send(result);
            },
        )
    })
    .unwrap()
    .unwrap();
    receiver.await.unwrap().unwrap();
    cx.update(|app| {
        owner
            .borrow_mut()
            .bind_interrupted_exit_process(request, appearance, app)
    })
    .unwrap()
    .unwrap();

    let foreign = request.test_foreign();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (attempt, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::await_interrupted_exit_completion(
                owner,
                attempt,
                cancellation,
                cx,
            )
            .await
            .is_err()
        );
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(owner.borrow().exit_requested());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        let retained = owner.borrow();
        retained
            .interrupted_exit_publication_result(request)
            .unwrap();
        retained
            .interrupted_exit_theme_activation_result(request)
            .unwrap();
        assert_eq!(retained.test_process_appearance(), *appearance);
        assert!(
            retained
                .test_services()
                .process
                .execution_permit()
                .reserve()
                .is_err()
        );
        cx.update(|app| {
            for shell in retained.test_process().windows.shells() {
                let root = shell.window().read(app).unwrap();
                assert_eq!(root.test_exit_presentation().0, "Exiting…");
                assert!(root.test_notices_inert());
                let mount = root.controller().unwrap().composer_mount().unwrap();
                let resident = mount.read(app).contribution().unwrap();
                assert!(!resident.read(app).gpui_input().read(app).is_enabled());
            }
        })
        .unwrap();
    }
    RunningProcessOwner::await_interrupted_exit_completion(
        owner,
        request,
        CommandCancellation::new(),
        cx,
    )
    .await
    .unwrap();
    assert!(
        RunningProcessOwner::await_interrupted_exit_completion(
            owner,
            request,
            CommandCancellation::new(),
            cx,
        )
        .await
        .is_err()
    );
}
