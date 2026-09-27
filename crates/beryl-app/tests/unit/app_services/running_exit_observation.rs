pub(super) async fn delivered(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    cx: &mut AsyncApp,
) -> (
    startup_owner::RunningExitRequest,
    crate::cas_projection::ShutdownWorkObservation,
) {
    let slot = Rc::new(RefCell::new(None));
    let delivered = slot.clone();
    let gui_thread = std::thread::current().id();
    cx.update(|app| {
        RunningProcessOwner::wait_for_exit(owner, app, move |owner, mut request, app| {
            let invoking = owner
                .borrow()
                .resolve_exit_window(&mut request, app)
                .unwrap();
            let identity = request.identity();
            let command = owner.borrow().window_exit_command(invoking, app).unwrap();
            command.request_exit();
            assert!(
                RunningProcessOwner::observe_exit_work(
                    owner,
                    request,
                    ProjectionCancellationToken::new(),
                    app,
                    move |owner, mut request, result, app| {
                        assert_eq!(std::thread::current().id(), gui_thread);
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        assert_eq!(
                            owner
                                .borrow()
                                .resolve_exit_window(&mut request, app)
                                .unwrap(),
                            invoking
                        );
                        assert!(owner.borrow().exit_requested());
                        assert!(owner.borrow().shutdown_status().is_none());
                        owner
                            .borrow()
                            .test_services()
                            .process
                            .execution_permit()
                            .commit(|| ())
                            .unwrap();
                        let idle = result.unwrap();
                        assert!(!idle.has_work());
                        let cancellation = ProjectionCancellationToken::new();
                        cancellation.cancel();
                        assert!(
                            RunningProcessOwner::observe_exit_work(
                                owner,
                                request,
                                cancellation,
                                app,
                                move |owner, mut request, result, app| {
                                    assert_eq!(std::thread::current().id(), gui_thread);
                                    assert!(Rc::ptr_eq(&identity, &request.identity()));
                                    assert_eq!(
                                        owner
                                            .borrow()
                                            .resolve_exit_window(&mut request, app)
                                            .unwrap(),
                                        invoking
                                    );
                                    assert!(matches!(
                                        result,
                                        Err(AppServiceCloseError::Work(
                                            crate::cas_projection::ShutdownWorkError::Work(
                                                crate::cas_projection::ProcessWorkError::Cancelled
                                            )
                                        ))
                                    ));
                                    assert!(owner.borrow().exit_requested());
                                    assert!(owner.borrow().shutdown_status().is_none());
                                    owner
                                        .borrow()
                                        .test_services()
                                        .process
                                        .execution_permit()
                                        .commit(|| ())
                                        .unwrap();
                                    assert!(
                                        delivered.borrow_mut().replace((request, idle)).is_none()
                                    );
                                },
                            )
                            .is_ok()
                        );
                    },
                )
                .is_ok()
            );
        })
        .unwrap();
    })
    .unwrap();
    wait(&slot, cx).await;
    slot.borrow_mut().take().unwrap()
}
