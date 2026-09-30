use crate::main_window::*;
use crate::theme_runtime::GpuiAppearanceWindowSet;
use resident_recovery::resident_fixture;

pub(super) async fn verify_and_dispose(
    owner: Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    cx: &mut AsyncApp,
) {
    let windows: Vec<_> = owner
        .borrow()
        .test_process()
        .windows
        .shells()
        .iter()
        .map(|shell| shell.window())
        .collect();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let prepared = owner.borrow().interrupted_exit_appearance(request).unwrap();
    let home = prepared.prepared().home().home_id();
    let generation = prepared.prepared().home().home_generation();
    let previous = owner.borrow().test_process_appearance();
    let appearance = cx
        .update(|app| {
            previous.update(app, |set, _| set.retire());
            GpuiAppearanceWindowSet::new(prepared, std::num::NonZeroUsize::new(4).unwrap(), app)
        })
        .unwrap();
    let mut residents = Vec::new();
    for (index, window) in windows.iter().copied().enumerate() {
        let (mount, composer, input, selection, close, mut retirement) = window
            .update(cx, |root, _, app| {
                let mount = root.controller().unwrap().composer_mount().unwrap();
                let composer = mount.read(app).contribution().unwrap();
                let resident = composer.read(app);
                let input = resident.gpui_input();
                let selection = resident.selection_identity().claim();
                let close = resident.recovery_snapshot().unwrap().close_ticket();
                let retirement = mount
                    .update(app, |mount, cx| {
                        mount.take_interrupted_exit_retirement(close, cx)
                    })
                    .unwrap();
                assert!(retirement.is_some());
                (mount, composer, input, selection, close, retirement)
            })
            .unwrap();
        let mut adapters = Some(
            owner
                .borrow()
                .interrupted_exit_composer_adapters(
                    request,
                    home,
                    generation,
                    configuration()
                        .projection
                        .turn_start_admission_requirement(),
                )
                .unwrap(),
        );
        let mut configurator: Option<MainWindowConversationComposerConfigurator> =
            Some(Box::new(resident_fixture::configure));
        let mut preparation = None;
        let foreign = request.test_foreign();
        let cancelled = CommandCancellation::new();
        cancelled.cancel();
        for (attempt, cancellation) in
            [(&foreign, CommandCancellation::new()), (request, cancelled)]
        {
            assert!(
                RunningProcessOwner::prepare_and_attach_interrupted_exit_resident_window(
                    &owner,
                    attempt,
                    &mut preparation,
                    |_| panic!("refused entry admitted preparation"),
                    window,
                    &appearance,
                    &mut adapters,
                    &mut configurator,
                    |_, _| panic!("refused entry attached resident"),
                    cancellation,
                    cx,
                )
                .await
                .is_err()
            );
            assert!(preparation.is_none());
            assert!(retirement.is_some() && adapters.is_some() && configurator.is_some());
        }
        let current = Rc::new(RefCell::new(None));
        let captured = current.clone();
        let refuse_appearance = windows.len() > 1 && index + 1 == windows.len();
        let attached = RunningProcessOwner::prepare_and_attach_interrupted_exit_resident_window(
            &owner,
            request,
            &mut preparation,
            |app| {
                RunningProcessOwner::prepare_interrupted_exit_resident(
                    &owner,
                    request,
                    &composer,
                    close,
                    window.into(),
                    generation,
                    &mut retirement,
                    move |seed, selection, window| {
                        let (environment, capacity) =
                            resident_fixture::environment(seed, selection, window)?;
                        *captured.borrow_mut() =
                            Some(gpui_text_input::RangePrepublicationCurrent {
                                binding: seed.binding,
                                history: seed.history,
                                available_capacity: gpui_text_input::RangeSurfaceCharge {
                                    bytes: capacity.bytes / 2,
                                    items: capacity.items / 2,
                                },
                            });
                        Ok((environment, capacity))
                    },
                    app,
                    |_, _| {},
                )
            },
            window,
            if refuse_appearance {
                &previous
            } else {
                &appearance
            },
            &mut adapters,
            &mut configurator,
            |_, _| Ok(current.borrow_mut().take().unwrap()),
            CommandCancellation::new(),
            cx,
        )
        .await;
        assert!(preparation.is_none() && retirement.is_none());
        assert!(adapters.is_none() && configurator.is_none());
        let (renewed, window_id) = if refuse_appearance {
            assert_eq!(
                attached.unwrap_err(),
                "Recovery appearance candidate identity changed"
            );
            let renewed = owner
                .borrow()
                .test_captured_recovery_ticket(composer.entity_id())
                .unwrap();
            assert_ne!(renewed, close);
            let window_id = cx
                .update(|app| {
                    assert!(
                        owner
                            .borrow()
                            .validate_interrupted_exit_bindings(request, &appearance, app)
                            .is_err()
                    );
                    assert!(owner.borrow().test_services().graph().is_none());
                    for retained_window in &windows {
                        let root = retained_window.read(app).unwrap();
                        assert_eq!(root.test_exit_presentation().0, "Exiting…");
                        if *retained_window != window {
                            assert!(root.test_notices_inert());
                        }
                        let retained_mount = root.controller().unwrap().composer_mount().unwrap();
                        let retained_composer = retained_mount.read(app).contribution().unwrap();
                        assert!(
                            !retained_composer
                                .read(app)
                                .gpui_input()
                                .read(app)
                                .is_enabled()
                        );
                    }
                    let root = window.read(app).unwrap();
                    assert_eq!(
                        root.controller().unwrap().composer_mount().as_ref(),
                        Some(&mount)
                    );
                    assert_eq!(mount.read(app).contribution().as_ref(), Some(&composer));
                    assert_eq!(composer.read(app).gpui_input(), input);
                    assert_eq!(composer.read(app).selection_identity().claim(), selection);
                    assert!(composer.read(app).recovery_snapshot().is_none());
                    root.controller().unwrap().window_id()
                })
                .unwrap();
            assert_eq!(
                original,
                format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
            );
            assert!(owner.borrow().exit_requested());
            assert!(!RunningProcessOwner::finish_exit(&owner, request));
            cx.update(|app| {
                owner.borrow_mut().bind_interrupted_exit_appearance(
                    request,
                    window,
                    &appearance,
                    app,
                )
            })
            .unwrap()
            .unwrap();
            (renewed, window_id)
        } else {
            let (renewed, record) = attached.unwrap();
            (renewed, record.window_id())
        };
        assert_ne!(renewed, close);
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(owner.borrow().exit_requested());
        assert!(!RunningProcessOwner::finish_exit(&owner, request));
        cx.update(|app| {
            let complete =
                owner
                    .borrow()
                    .validate_interrupted_exit_bindings(request, &appearance, app);
            assert_eq!(complete.is_ok(), index + 1 == windows.len());
            for retained_window in &windows {
                let root = retained_window.read(app).unwrap();
                assert_eq!(root.test_exit_presentation().0, "Exiting…");
                let retained_mount = root.controller().unwrap().composer_mount().unwrap();
                let retained_composer = retained_mount.read(app).contribution().unwrap();
                assert!(
                    !retained_composer
                        .read(app)
                        .gpui_input()
                        .read(app)
                        .is_enabled()
                );
            }
            assert!(owner.borrow().test_services().graph().is_none());
            assert_eq!(
                owner
                    .borrow()
                    .test_captured_recovery_ticket(composer.entity_id()),
                Some(renewed)
            );
            let root = window.read(app).unwrap();
            assert_eq!(root.test_exit_presentation().0, "Exiting…");
            assert!(root.test_notices_inert());
            assert_eq!(
                root.controller().unwrap().composer_mount().as_ref(),
                Some(&mount)
            );
            assert_eq!(mount.read(app).contribution().as_ref(), Some(&composer));
            assert_eq!(composer.read(app).gpui_input(), input);
            assert_eq!(composer.read(app).selection_identity().claim(), selection);
            assert!(composer.read(app).recovery_snapshot().is_none());
            assert!(!input.read(app).is_enabled());
        })
        .unwrap();
        residents.push((mount, composer, input, selection, window_id));
    }

    RunningProcessOwner::publish_and_complete_interrupted_exit(
        &owner,
        request,
        retired,
        generation,
        &appearance,
        CommandCancellation::new(),
        cx,
    )
    .await
    .unwrap();
    assert!(!owner.borrow().exit_requested());
    assert!(owner.borrow().interrupted_exit_session().is_none());
    assert!(owner.borrow().shutdown_status().is_none());
    assert!(!RunningProcessOwner::finish_exit(&owner, request));
    assert_eq!(owner.borrow().test_process_appearance(), appearance);
    {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        assert_eq!(graph.home().home_id(), home);
        assert_eq!(graph.home().health().generation().unwrap(), generation);
        drop(
            retained
                .test_services()
                .process
                .execution_permit()
                .reserve()
                .unwrap(),
        );
    }
    for (window, (mount, composer, input, selection, window_id)) in windows.iter().zip(&residents) {
        window
            .update(cx, |root, _, app| {
                assert_eq!(root.test_exit_presentation().0, "Exit");
                assert!(!root.test_notices_inert());
                assert_eq!(
                    root.controller().unwrap().composer_mount().as_ref(),
                    Some(mount)
                );
                assert_eq!(mount.read(app).contribution().as_ref(), Some(composer));
                assert_eq!(&composer.read(app).gpui_input(), input);
                assert_eq!(&composer.read(app).selection_identity().claim(), selection);
                assert!(input.read(app).is_enabled());
                assert_eq!(&root.controller().unwrap().window_id(), window_id);
            })
            .unwrap();
    }
    assert!(
        RunningProcessOwner::publish_and_complete_interrupted_exit(
            &owner,
            request,
            retired,
            generation,
            &appearance,
            CommandCancellation::new(),
            cx,
        )
        .await
        .is_err()
    );
    assert_eq!(
        owner
            .borrow()
            .test_process()
            .windows
            .shells()
            .iter()
            .map(|shell| shell.window())
            .collect::<Vec<_>>(),
        windows
    );

    for window in &windows {
        let mut draft = window
            .update(cx, |root, window, app| {
                root.set_shutdown_interaction_gated(true, app).unwrap();
                root.begin_shutdown_draft(window, app).unwrap()
            })
            .unwrap();
        loop {
            let ready = window
                .update(cx, |root, window, app| {
                    root.advance_shutdown_draft(&draft, window, app).unwrap()
                        == MainWindowShutdownDraftAdvance::Resident(
                            MainWindowConversationComposerCloseAdvance::Ready,
                        )
                })
                .unwrap();
            if ready {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        loop {
            if window
                .update(cx, |root, _, app| {
                    root.retire_shutdown_draft(&mut draft, app).unwrap()
                })
                .unwrap()
            {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        drop(draft);
    }
    let mut running = Rc::try_unwrap(owner)
        .ok()
        .unwrap()
        .into_inner()
        .test_into_process();
    cx.update(|app| {
        for window in windows {
            window
                .update(app, |_, window, _| window.remove_window())
                .unwrap();
        }
        drop(running.windows);
        appearance.update(app, |set, _| set.retire());
        assert!(app.windows().is_empty());
    })
    .unwrap();
    drop(residents);
    cx.background_executor()
        .spawn(async move {
            super::close(&mut running.services);
        })
        .await;
}
