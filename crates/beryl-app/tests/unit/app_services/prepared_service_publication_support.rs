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
    let foreign = request.test_foreign();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    for mode in ["unproven", "unwind", "cancel", "publish"] {
        let replaced = (mode == "unproven").then(|| {
            owner
                .borrow()
                .test_replace_interrupted_exit_session(RunningShutdownSession::Unwound)
        });
        let cancellation = CommandCancellation::new();
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            assert!(
                owner
                    .borrow()
                    .release_interrupted_exit_mounts(request, appearance, app)
                    .is_err()
            );
            let refused =
                |request, retired, generation, appearance, cancellation, app: &mut gpui::App| {
                    RunningProcessOwner::publish_interrupted_exit_services(
                        owner,
                        request,
                        retired,
                        generation,
                        appearance,
                        cancellation,
                        app,
                        |_, _, _| panic!("refused publication callback"),
                    )
                };
            assert!(
                refused(
                    &foreign,
                    retired,
                    generation,
                    appearance,
                    CommandCancellation::new(),
                    app
                )
                .is_err()
            );
            assert!(
                refused(
                    request,
                    generation,
                    generation,
                    appearance,
                    CommandCancellation::new(),
                    app
                )
                .is_err()
            );
            assert!(
                refused(
                    request,
                    retired,
                    retired,
                    appearance,
                    CommandCancellation::new(),
                    app
                )
                .is_err()
            );
            let cancelled = CommandCancellation::new();
            cancelled.cancel();
            assert!(refused(request, retired, generation, appearance, cancelled, app).is_err());
            let previous = owner.borrow().test_process_appearance();
            assert!(
                refused(
                    request,
                    retired,
                    generation,
                    &previous,
                    CommandCancellation::new(),
                    app
                )
                .is_err()
            );
            let drafts = owner
                .borrow_mut()
                .test_replace_recovery_drafts(None)
                .unwrap();
            assert!(
                refused(
                    request,
                    retired,
                    generation,
                    appearance,
                    CommandCancellation::new(),
                    app
                )
                .is_err()
            );
            owner
                .borrow_mut()
                .test_replace_recovery_drafts(Some(drafts.clone()));
            let busy = drafts.borrow_mut();
            assert!(
                refused(
                    request,
                    retired,
                    generation,
                    appearance,
                    CommandCancellation::new(),
                    app
                )
                .is_err()
            );
            drop(busy);
            for retirement in [None, Some(Err("failed retirement".into()))] {
                owner
                    .borrow()
                    .test_set_resident_graph_retirement(retirement);
                assert!(
                    refused(
                        request,
                        retired,
                        generation,
                        appearance,
                        CommandCancellation::new(),
                        app
                    )
                    .is_err()
                );
            }
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Ok(())));
            let before = cancellation.clone();
            let after = cancellation.clone();
            RunningProcessOwner::test_publish_interrupted_exit_services(
                owner,
                request,
                retired,
                generation,
                appearance,
                cancellation,
                app,
                move |owner, result, _| {
                    assert!(!owner.borrow().test_services_on_worker());
                    assert!(owner.borrow().interrupted_exit_session().is_some());
                    sender.send(result).unwrap();
                },
                move || match mode {
                    "unwind" => panic!("injected recovery publication validation unwind"),
                    "cancel" => before.cancel(),
                    _ => {}
                },
                move || {
                    if mode == "publish" && !matches!(delivery, RecoveryPublicationDelivery::Ready)
                    {
                        after.cancel();
                    }
                },
            )
            .unwrap();
            assert!(owner.borrow().test_services_on_worker());
            assert!(owner.borrow().interrupted_exit_session().is_none());
            assert!(
                owner
                    .borrow()
                    .release_interrupted_exit_mounts(request, appearance, app)
                    .is_err()
            );
            assert!(
                owner
                    .borrow()
                    .validate_interrupted_exit_bindings(request, appearance, app)
                    .is_err()
            );
            assert!(
                owner
                    .borrow()
                    .release_interrupted_exit_drafts(request, appearance, app)
                    .is_err()
            );
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_publication_result(request)
                    .is_err()
            );
            assert!(
                refused(
                    request,
                    retired,
                    generation,
                    appearance,
                    CommandCancellation::new(),
                    app
                )
                .is_err()
            );
            assert!(
                RunningProcessOwner::cancel_interrupted_exit_services(
                    owner,
                    request,
                    app,
                    |_, _| panic!("publication excludes graph disposal")
                )
                .is_err()
            );
            assert!(!RunningProcessOwner::finish_exit(owner, request));
            if mode == "publish" && matches!(delivery, RecoveryPublicationDelivery::Stale) {
                owner
                    .borrow_mut()
                    .test_replace_interrupted_exit_request(&foreign);
            }
        })
        .unwrap();
        let delivered = receiver.await.unwrap();
        if mode == "publish" {
            match delivery {
                RecoveryPublicationDelivery::Ready => delivered.unwrap(),
                RecoveryPublicationDelivery::Cancelled => {
                    assert!(delivered.unwrap_err().contains("cancelled"))
                }
                RecoveryPublicationDelivery::Stale => {
                    assert!(delivered.unwrap_err().contains("request changed"));
                    assert!(
                        owner
                            .borrow()
                            .interrupted_exit_publication_result(request)
                            .is_err()
                    );
                    cx.update(|app| {
                        assert!(
                            owner
                                .borrow()
                                .release_interrupted_exit_mounts(request, appearance, app)
                                .is_err()
                        );
                        assert!(
                            owner
                                .borrow()
                                .validate_interrupted_exit_bindings(request, appearance, app)
                                .is_err()
                        );
                    })
                    .unwrap();
                    owner
                        .borrow_mut()
                        .test_replace_interrupted_exit_request(request);
                }
            }
            owner
                .borrow()
                .interrupted_exit_publication_result(request)
                .unwrap();
            assert!(owner.borrow().test_services().graph().is_some());
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_services_result(request)
                    .is_err()
            );
            cx.update(|app| {
                verify_published_bindings(owner, request, appearance, app);
                assert!(
                    RunningProcessOwner::publish_interrupted_exit_services(
                        owner,
                        request,
                        retired,
                        generation,
                        appearance,
                        CommandCancellation::new(),
                        app,
                        |_, _, _| panic!("published graph cannot be published twice"),
                    )
                    .is_err()
                );
            })
            .unwrap();
            let start = owner.borrow().test_take_interrupted_exit_start();
            let gate = start.gate();
            drop(start);
            assert!(!gate.wait());
            cx.update(|app| {
                assert!(
                    owner
                        .borrow()
                        .release_interrupted_exit_mounts(request, appearance, app)
                        .is_err()
                );
                assert!(
                    owner
                        .borrow()
                        .validate_interrupted_exit_bindings(request, appearance, app)
                        .is_err()
                );
            })
            .unwrap();
        } else {
            let reason = match mode {
                "unproven" => "unproven",
                "unwind" => "unwound",
                _ => "cancelled",
            };
            assert!(delivered.unwrap_err().contains(reason));
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_publication_result(request)
                    .unwrap_err()
                    .contains(reason)
            );
            assert!(owner.borrow().test_services().graph().is_none());
            cx.update(|app| {
                assert!(
                    owner
                        .borrow()
                        .release_interrupted_exit_mounts(request, appearance, app)
                        .is_err()
                );
            })
            .unwrap();
            owner
                .borrow()
                .interrupted_exit_services_result(request)
                .unwrap();
        }
        if let Some(replaced) = replaced {
            owner
                .borrow()
                .test_replace_interrupted_exit_session(replaced);
        }
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(owner.borrow().require_shutdown_session_ready().is_err());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        cx.update(|app| {
            owner.borrow().test_process().windows.shells()[0]
                .window()
                .update(app, |root, _, _| {
                    assert_eq!(root.test_exit_presentation().0, "Exiting…")
                })
                .unwrap();
        })
        .unwrap();
    }
}

fn verify_published_bindings(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
    app: &mut gpui::App,
) {
    use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet};
    let mut running = owner.borrow_mut();
    let refused = |running: &RunningProcessOwner, request, appearance, app: &mut gpui::App| {
        assert!(
            running
                .release_interrupted_exit_mounts(request, appearance, app)
                .is_err()
        );
        assert!(
            running
                .validate_interrupted_exit_bindings(request, appearance, app)
                .is_err()
        );
        assert!(
            running
                .release_interrupted_exit_drafts(request, appearance, app)
                .is_err()
        );
    };
    let foreign = request.test_foreign();
    let previous = running.test_process_appearance();
    refused(&running, &foreign, appearance, app);
    refused(&running, request, &previous, app);
    let unbound = GpuiAppearanceWindowSet::new(
        appearance.read(app).target().snapshot().current,
        NonZeroUsize::new(4).unwrap(),
        app,
    );
    refused(&running, request, &unbound, app);
    unbound.update(app, |set, _| set.retire());
    refused(&running, request, &unbound, app);
    let drafts = running.test_replace_recovery_drafts(None).unwrap();
    refused(&running, request, appearance, app);
    running.test_replace_recovery_drafts(Some(drafts.clone()));
    let busy = drafts.borrow_mut();
    refused(&running, request, appearance, app);
    drop(busy);
    drafts.borrow_mut().test_recovery_driving(true);
    refused(&running, request, appearance, app);
    drafts.borrow_mut().test_recovery_driving(false);
    for retirement in [None, Some(Err("failed retirement".into()))] {
        running.test_set_resident_graph_retirement(retirement);
        refused(&running, request, appearance, app);
    }
    running.test_set_resident_graph_retirement(Some(Ok(())));
    for target in [&previous, &unbound] {
        assert!(
            drafts
                .borrow()
                .release_recovered_mounts(&running.test_process().windows, target, app)
                .is_err()
        );
    }
    drafts.borrow_mut().test_recovery_driving(true);
    assert!(
        drafts
            .borrow()
            .release_recovered_mounts(&running.test_process().windows, appearance, app)
            .is_err()
    );
    drafts.borrow_mut().test_recovery_driving(false);
    for _ in 0..2 {
        running
            .validate_interrupted_exit_bindings(request, appearance, app)
            .unwrap();
        assert!(
            running
                .release_interrupted_exit_drafts(request, appearance, app)
                .unwrap()
        );
        running
            .interrupted_exit_publication_result(request)
            .unwrap();
        assert!(
            running
                .release_interrupted_exit_mounts(request, appearance, app)
                .unwrap()
        );
    }
    assert!(!drafts.borrow().test_recovery_ready());
    assert_ne!(running.test_process_appearance(), *appearance);
}
