pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
    delivery: RecoveryPublicationDelivery,
    cx: &mut AsyncApp,
) -> bool {
    use beryl_home_store::CommandCancellation;
    let foreign = request.test_foreign();
    let prepared = owner
        .borrow()
        .test_services()
        .graph()
        .unwrap()
        .current_appearance()
        .unwrap();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let cancellation = CommandCancellation::new();
    let before = cancellation.clone();
    let after = cancellation.clone();
    let gui_thread = std::thread::current().id();
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        let refuse = |request, appearance, cancellation, app: &mut gpui::App| {
            assert!(
                RunningProcessOwner::activate_interrupted_exit_theme(
                    owner,
                    request,
                    appearance,
                    cancellation,
                    app,
                    |_, _, _| panic!("refused activation callback"),
                )
                .is_err()
            );
        };
        refuse(&foreign, appearance, CommandCancellation::new(), app);
        let previous = owner.borrow().test_process_appearance();
        refuse(request, &previous, CommandCancellation::new(), app);
        let cancelled = CommandCancellation::new();
        cancelled.cancel();
        refuse(request, appearance, cancelled, app);
        let drafts = owner
            .borrow_mut()
            .test_replace_recovery_drafts(None)
            .unwrap();
        refuse(request, appearance, CommandCancellation::new(), app);
        owner
            .borrow_mut()
            .test_replace_recovery_drafts(Some(drafts.clone()));
        let busy = drafts.borrow_mut();
        refuse(request, appearance, CommandCancellation::new(), app);
        drop(busy);
        assert!(
            owner
                .borrow()
                .interrupted_exit_theme_activation_result(request)
                .is_err()
        );
        assert!(
            owner
                .borrow()
                .release_interrupted_exit_mounts(request, appearance, app)
                .unwrap_err()
                .contains("not settled")
        );
        RunningProcessOwner::test_activate_interrupted_exit_theme(
            owner,
            request,
            appearance,
            cancellation,
            app,
            move |owner, result, _| {
                assert!(!owner.borrow().test_services_on_worker());
                assert!(owner.borrow().interrupted_exit_session().is_some());
                sender.send(result).unwrap();
            },
            move |services| {
                assert_ne!(gui_thread, std::thread::current().id());
                match delivery {
                    RecoveryPublicationDelivery::ThemeActivationFailure => {
                        // Exercise the one-shot loader's ordinary error through worker delivery.
                        services.graph_mut().unwrap().release_theme().unwrap();
                    }
                    RecoveryPublicationDelivery::ThemeActivationUnwind => {
                        panic!("injected theme activation unwind")
                    }
                    RecoveryPublicationDelivery::ThemeActivationCancelled => before.cancel(),
                    _ => {}
                }
            },
            move || {
                if matches!(
                    delivery,
                    RecoveryPublicationDelivery::Cancelled | RecoveryPublicationDelivery::Stale
                ) {
                    after.cancel();
                }
            },
        )
        .unwrap();
        assert!(owner.borrow().test_services_on_worker());
        assert!(owner.borrow().interrupted_exit_session().is_some());
        assert!(
            owner
                .borrow()
                .release_interrupted_exit_mounts(request, appearance, app)
                .is_err()
        );
        refuse(request, appearance, CommandCancellation::new(), app);
        assert!(
            owner
                .borrow()
                .validate_interrupted_exit_bindings(request, appearance, app)
                .is_err()
        );
        assert!(
            owner
                .borrow()
                .interrupted_exit_theme_activation_result(request)
                .is_err()
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        if matches!(delivery, RecoveryPublicationDelivery::Stale) {
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(&foreign);
        }
    })
    .unwrap();
    let delivered = receiver.await.unwrap();
    let failure = match delivery {
        RecoveryPublicationDelivery::ThemeActivationFailure => Some("failed"),
        RecoveryPublicationDelivery::ThemeActivationUnwind => Some("unwound"),
        RecoveryPublicationDelivery::ThemeActivationCancelled => Some("cancelled"),
        _ => None,
    };
    match delivery {
        RecoveryPublicationDelivery::Stale => {
            assert!(delivered.unwrap_err().contains("request changed"));
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_theme_activation_result(request)
                    .is_err()
            );
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(request);
        }
        RecoveryPublicationDelivery::Cancelled => {
            assert!(delivered.unwrap_err().contains("cancelled"))
        }
        _ => match failure {
            Some(reason) => assert!(delivered.unwrap_err().contains(reason)),
            None => delivered.unwrap(),
        },
    }
    let actual = owner
        .borrow()
        .interrupted_exit_theme_activation_result(request);
    if let Some(reason) = failure {
        assert!(actual.unwrap_err().contains(reason));
        let previous = owner.borrow().test_process_appearance();
        assert!(
            RunningProcessOwner::bind_and_complete_interrupted_exit(
                owner,
                request,
                appearance,
                CommandCancellation::new(),
                cx,
            )
            .await
            .unwrap_err()
            .contains(reason)
        );
        assert_eq!(owner.borrow().test_process_appearance(), previous);
        assert!(owner.borrow().exit_requested());
    } else {
        actual.unwrap();
        let running = owner.borrow();
        let graph = running.test_services().graph().unwrap();
        assert!(graph.theme.is_none());
        assert!(Arc::ptr_eq(&prepared, &graph.current_appearance().unwrap()));
    }
    assert_eq!(
        original,
        format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
    );
    cx.update(|app| {
        if let Some(reason) = failure {
            assert!(
                owner
                    .borrow()
                    .release_interrupted_exit_mounts(request, appearance, app)
                    .unwrap_err()
                    .contains(reason)
            );
            owner
                .borrow()
                .validate_interrupted_exit_bindings(request, appearance, app)
                .unwrap();
        }
        assert!(
            RunningProcessOwner::activate_interrupted_exit_theme(
                owner,
                request,
                appearance,
                CommandCancellation::new(),
                app,
                |_, _, _| panic!("activation cannot repeat"),
            )
            .is_err()
        );
        assert!(
            owner
                .borrow()
                .interrupted_exit_theme_activation_result(&foreign)
                .is_err()
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
    })
    .unwrap();
    assert!(
        RunningProcessOwner::activate_and_complete_interrupted_exit(
            owner,
            request,
            appearance,
            CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap_err()
        .contains("already attempted")
    );
    assert!(!owner.borrow().test_services_on_worker());
    failure.is_none()
}
