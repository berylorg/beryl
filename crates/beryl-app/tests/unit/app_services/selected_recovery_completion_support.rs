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
        assert_retained(owner, request, appearance, &original, 0, cx);
    }
    for deferred_mount in [0, 1] {
        await_discussion_cohort(owner, cx).await;
        verify_pending_wait(owner, request, appearance, &original, deferred_mount, cx).await;
    }
    verify_discussion_cohort_rejects_home_mutation(owner, request, appearance, &original, cx).await;
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

async fn await_discussion_cohort(owner: &Rc<RefCell<RunningProcessOwner>>, cx: &mut AsyncApp) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let ready = cx.update(|app| {
            let retained = owner.borrow();
            let mut cohort = None;
            for shell in retained.test_process().windows.shells() {
                let root = shell.window().read(app).unwrap();
                let Some((snapshot, gated, _, _, fresh)) = root.test_discussion_presentation(app) else {
                    return false;
                };
                if !fresh || !gated || !snapshot.current() {
                    return false;
                }
                if cohort.as_ref().is_some_and(|current: &crate::discussion_presentation::DiscussionPresentationSnapshot| !current.same_fence(&snapshot)) {
                    return false;
                }
                cohort = Some(snapshot);
            }
            cohort.is_some()
        }).unwrap();
        if ready {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "recovered discussion cohort did not become current"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

async fn verify_discussion_cohort_rejects_home_mutation(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
    original: &str,
    cx: &mut AsyncApp,
) {
    await_discussion_cohort(owner, cx).await;
    let editors = cx
        .update(|app| {
            owner
                .borrow()
                .test_process()
                .windows
                .shells()
                .iter()
                .map(|shell| {
                    let mount = shell
                        .window()
                        .read(app)
                        .unwrap()
                        .controller()
                        .unwrap()
                        .composer_mount()
                        .unwrap();
                    let resident = mount.read(app).contribution().unwrap();
                    (
                        resident.entity_id(),
                        resident.read(app).selection_identity(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap();
    let reached = Rc::new(std::cell::Cell::new(false));
    let observed = reached.clone();
    let cancellation = CommandCancellation::new();
    let cancel_after_mutation = cancellation.clone();
    owner
        .borrow_mut()
        .test_before_thread_creation_reopen(Box::new(move |services| {
            let graph = services.graph().unwrap();
            let home = graph.home().service_reference();
            let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
            command
                .add(graph.syndic().create_thread(
                    graph.syndic().revision(&home).unwrap(),
                    syndic_storage::CreateThread::ordinary(
                        beryl_model::SyndicThreadId::from_bytes([237; 16]),
                        beryl_model::SyndicDraftId::from_bytes([238; 16]),
                        crate::support::exact_cas::execution_binding(),
                        syndic_storage::SyndicTimestamp::from_unix_millis(9),
                        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                    ),
                ))
                .unwrap();
            assert!(matches!(
                home.execute(command),
                beryl_home_store::CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
            observed.set(true);
            cancel_after_mutation.cancel();
        }));
    assert!(
        RunningProcessOwner::await_interrupted_exit_completion(owner, request, cancellation, cx)
            .await
            .is_err(),
        "Home mutation after snapshot capture must refuse native publication"
    );
    assert!(reached.get(), "cohort race injection was not reached");
    assert_retained(owner, request, appearance, original, editors.len(), cx);
    assert!(
        owner
            .borrow()
            .test_services()
            .test_recovery_process_is_fenced()
    );
    cx.update(|app| {
        for (shell, (editor, selection)) in owner
            .borrow()
            .test_process()
            .windows
            .shells()
            .iter()
            .zip(editors)
        {
            let mount = shell
                .window()
                .read(app)
                .unwrap()
                .controller()
                .unwrap()
                .composer_mount()
                .unwrap();
            let resident = mount.read(app).contribution().unwrap();
            assert_eq!(resident.entity_id(), editor);
            assert_eq!(resident.read(app).selection_identity(), selection);
            let root = shell.window().read(app).unwrap();
            assert!(root.coherent_viewed_thread(app).is_none());
            assert!(root.test_discussion_presentation(app).unwrap().1);
        }
    })
    .unwrap();
    await_discussion_cohort(owner, cx).await;
}

async fn verify_pending_wait(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
    original: &str,
    deferred_mount: usize,
    cx: &mut AsyncApp,
) {
    use std::{future::Future, task::Poll};

    let mount = cx
        .update(|app| {
            let retained = owner.borrow();
            let root = retained
                .test_process()
                .windows
                .shells()
                .get(deferred_mount)
                .unwrap()
                .window();
            let mount = root
                .read(app)
                .unwrap()
                .controller()
                .unwrap()
                .composer_mount()
                .unwrap();
            mount
        })
        .unwrap();
    for cancel in [true, false] {
        cx.update(|app| {
            mount.update(app, |mount, _| {
                assert!(!mount.test_set_recovered_mount_deferred(true));
            })
        })
        .unwrap();
        let cancellation = CommandCancellation::new();
        let mut drive_cx = cx.clone();
        let mut drive = Box::pin(RunningProcessOwner::await_interrupted_exit_completion(
            owner,
            request,
            cancellation.clone(),
            &mut drive_cx,
        ));
        std::future::poll_fn(|task| {
            let observed = drive.as_mut().poll(task);
            assert!(observed.is_pending(), "cancel={cancel}: {observed:?}");
            Poll::Ready(())
        })
        .await;
        cx.update(|app| {
            mount.update(app, |mount, _| {
                assert!(!mount.test_set_recovered_mount_deferred(false));
            })
        })
        .unwrap();
        assert_retained(owner, request, appearance, original, deferred_mount, cx);
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
        assert_retained(owner, request, appearance, original, deferred_mount, cx);
        if cancel {
            cancellation.cancel();
            assert!(drive.await.unwrap_err().contains("cancelled"));
        } else {
            drop(drive);
        }
        assert_retained(owner, request, appearance, original, deferred_mount, cx);
    }
}

fn assert_retained(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
    original: &str,
    prepared_mounts: usize,
    cx: &mut AsyncApp,
) {
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
        for (index, shell) in retained.test_process().windows.shells().iter().enumerate() {
            let root = shell.window().read(app).unwrap();
            assert_eq!(root.test_exit_presentation().0, "Exiting…");
            assert!(root.test_notices_inert());
            let mount = root.controller().unwrap().composer_mount().unwrap();
            let resident = mount.read(app).contribution().unwrap();
            let input = resident.read(app).gpui_input();
            assert_eq!(input.read(app).is_enabled(), index < prepared_mounts);
            if index < prepared_mounts {
                input.update(app, |input, cx| {
                    assert!(matches!(
                        input.insert_inline_object_at_selection(
                            gpui_text_input::InlineObjectId::new(1),
                            gpui_text_input::InlineObjectOrder::new(1),
                            0,
                            0,
                            cx,
                        ),
                        Err(gpui_text_input::RangeTextInputError::ReadOnly)
                    ));
                });
            }
        }
    })
    .unwrap();
}
