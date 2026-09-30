pub(super) async fn verify_and_dispose(
    owner: Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    cx: &mut AsyncApp,
) {
    let window = owner.borrow().test_process().windows.shells()[0].window();
    let (home, retired) = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        (
            graph.home().home_id(),
            graph.home().health().generation().unwrap(),
        )
    };
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let (mount, composer, input, selection, close) = window
        .update(cx, |root, _, app| {
            assert!(!root.controller().unwrap().is_threadless());
            let mount = root.controller().unwrap().composer_mount().unwrap();
            let composer = mount.read(app).contribution().unwrap();
            let resident = composer.read(app);
            let input = resident.gpui_input();
            assert_eq!(root.test_exit_presentation().0, "Exiting…");
            let selection = resident.selection_identity().claim();
            let close = owner
                .borrow()
                .test_captured_recovery_ticket(composer.entity_id())
                .unwrap();
            (mount, composer.clone(), input, selection, close)
        })
        .unwrap();

    RunningProcessOwner::retire_and_prepare_interrupted_exit(
        &owner,
        request,
        retired,
        configuration(),
        SyndicTimestamp::from_unix_millis(2),
        CommandCancellation::new(),
        cx,
    )
    .await
    .unwrap();

    {
        let retained = owner.borrow();
        retained
            .interrupted_exit_graph_retirement_result(request)
            .unwrap();
        retained.interrupted_exit_services_result(request).unwrap();
        assert!(!retained.test_services_on_worker());
        assert!(retained.test_services().graph().is_none());
        let appearance = retained.interrupted_exit_appearance(request).unwrap();
        let candidate = appearance.prepared().home();
        assert_eq!(candidate.home_id(), home);
        assert_ne!(candidate.home_generation(), retired);
        assert_eq!(
            original,
            format!("{:?}", retained.interrupted_exit_session().unwrap())
        );
        assert!(retained.exit_requested());
        assert_eq!(retained.test_process().windows.shells()[0].window(), window);
        assert_eq!(
            retained.test_captured_recovery_ticket(composer.entity_id()),
            Some(close)
        );
    }
    assert!(!RunningProcessOwner::finish_exit(&owner, request));
    window
        .update(cx, |root, _, app| {
            assert!(root.test_shell_construction_retired());
            assert_eq!(root.test_exit_presentation().0, "Exiting…");
            assert_eq!(
                root.controller().unwrap().composer_mount().as_ref(),
                Some(&mount)
            );
            assert_eq!(mount.read(app).contribution().as_ref(), Some(&composer));
            let resident = composer.read(app);
            assert_eq!(resident.gpui_input(), input);
            assert_eq!(resident.selection_identity().claim(), selection);
            assert_eq!(resident.recovery_snapshot().unwrap().close_ticket(), close);
            assert!(!input.read(app).is_enabled());
        })
        .unwrap();

    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        RunningProcessOwner::cancel_interrupted_exit_services(&owner, request, app, move |_, _| {
            sender.send(()).unwrap();
        })
    })
    .unwrap()
    .unwrap();
    receiver.await.unwrap();
    assert!(matches!(
        owner
            .borrow_mut()
            .take_interrupted_exit_preparation_failure(request, retired)
            .unwrap(),
        crate::app_services::recovery_graph::RecoveryServicePreparationError::App(_)
    ));
    assert!(!owner.borrow().test_services_on_worker());
    assert!(!RunningProcessOwner::finish_exit(&owner, request));
    let mut running = Rc::try_unwrap(owner)
        .ok()
        .unwrap()
        .into_inner()
        .test_into_process();
    cx.update(|app| {
        window
            .update(app, |_, window, _| window.remove_window())
            .unwrap();
        drop(running.windows);
        running
            .appearance
            .update(app, |appearance, _| appearance.retire());
        assert!(app.windows().is_empty());
    })
    .unwrap();
    drop((mount, composer, input));
    cx.background_executor()
        .spawn(async move {
            assert!(running.services.graph().is_none());
            running
                .services
                .test_retired_service_home()
                .unwrap()
                .close()
                .unwrap();
        })
        .await;
}
