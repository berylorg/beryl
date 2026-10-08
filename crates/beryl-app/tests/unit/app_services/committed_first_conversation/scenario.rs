use super::*;

pub(super) fn run(
    cx: &mut TestAppContext,
    reject_release: bool,
    transient_reconciliation: bool,
    opening_classification: bool,
    stale_state: bool,
    cancel_publication: bool,
) {
    let (directory, process, prepared, appearance, faults) =
        std::thread::spawn(prepared_process).join().unwrap();
    let (owner, window, original_generation) = cx.update(|app| {
        let appearance =
            GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(4).unwrap(), app);
        let windows = PublishedMainWindowRestoreSet::from_virtual_prepared_test(
            prepared,
            appearance.clone(),
            app,
        )
        .unwrap();
        let window = windows.shells()[0].window();
        assert!(
            window
                .read(app)
                .unwrap()
                .controller()
                .unwrap()
                .composer_mount()
                .is_none()
        );
        let original_generation = process
            .graph()
            .unwrap()
            .home()
            .health()
            .generation()
            .unwrap();
        let owner = RunningProcessOwner::test_start_unmounted(
            StartedProcess {
                configuration: configuration(),
                services: process,
                windows,
                appearance,
                startup_surface: None,
                commands: StartupCommands::test_running(),
            },
            app,
        );
        (owner, window, original_generation)
    });
    let (thread, draft, exact_window) = {
        let mut process = owner.borrow_mut().test_take_services();
        let original_faults = faults.clone();
        let result = std::thread::spawn(move || {
            let result = commit_onboarding(
                &process,
                transient_reconciliation.then_some(&original_faults),
            );
            (process, result)
        })
        .join()
        .unwrap();
        process = result.0;
        owner.borrow_mut().test_restore_services(process);
        result.1
    };
    if stale_state {
        let process = owner.borrow_mut().test_take_services();
        let process = std::thread::spawn(move || {
            let graph = process.graph().unwrap();
            let home = graph.home();
            let state = graph.state();
            let snapshot = state.session().minimal_bootstrap(home).unwrap().unwrap();
            let current = &snapshot.windows()[0];
            let placement = beryl_model::WindowPlacement::new(
                beryl_model::WindowBounds::new(20, 20, 840, 620).unwrap(),
                beryl_model::WindowDisplayState::Normal,
                None,
                None,
            );
            let mut command = HomeCommand::new(home.home_revision().unwrap());
            command
                .add(state.session().update_placement(
                    state.session().revision(home).unwrap(),
                    beryl_state::UpdateWindowPlacement::new(
                        snapshot.header().revision(),
                        current.window_id(),
                        current.revision(),
                        placement,
                    ),
                ))
                .unwrap();
            assert!(matches!(
                home.execute(command),
                beryl_home_store::CommandOutcome::Committed { .. }
            ));
            process
        })
        .join()
        .unwrap();
        owner.borrow_mut().test_restore_services(process);
    }
    let mut opening_release = None;
    if opening_classification {
        let (reached, reached_receiver) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let process = owner.borrow_mut().test_take_services();
        let original_faults = faults.clone();
        let original_window = exact_window.clone();
        let process = std::thread::spawn(move || {
            let graph = process.graph().unwrap();
            let mut first = MainWindowFirstConversationPreparation::new_fresh(
                Arc::new(graph.home().service_reference()),
                graph.state(),
                graph.syndic().clone(),
                original_window,
                thread,
                draft,
            )
            .unwrap();
            first.test_arm_before_open_classification(move |home, _| {
                original_faults.fail_next(FaultPoint::BeforeReadConfirmation);
                let _ = home.home_revision();
                let deadline = Instant::now() + Duration::from_secs(5);
                while home.health().state() != HomeHealthState::Failed {
                    assert!(
                        Instant::now() < deadline,
                        "original opening did not fail its home generation"
                    );
                    std::thread::sleep(Duration::from_millis(2));
                }
                reached.send(()).unwrap();
                released.recv().unwrap();
            });
            graph
                .runtime_setup()
                .test_first_flight()
                .start_first_preparation(first)
                .unwrap_or_else(|(_, error)| panic!("original first editor worker: {error}"));
            process
        })
        .join()
        .unwrap();
        owner.borrow_mut().test_restore_services(process);
        wait(
            cx,
            |_| reached_receiver.try_recv().is_ok(),
            "original durable opening did not reach classification",
        );
        opening_release = Some(release);
    }
    if reject_release {
        owner
            .borrow_mut()
            .test_reject_first_conversation_widget_release();
    }
    if cancel_publication {
        owner
            .borrow_mut()
            .test_cancel_recovery_before_publication_validation();
    }
    cx.update(|app| {
        assert!(
            window
                .read(app)
                .unwrap()
                .controller()
                .unwrap()
                .is_threadless()
        );
        let retained = owner.borrow();
        assert!(
            retained
                .test_services()
                .windows
                .test_close_is_blocked(&[exact_window.window_id()])
        );
        assert!(
            retained
                .test_services()
                .windows
                .test_process_closing_is_blocked()
        );
    });
    {
        let process = owner.borrow_mut().test_take_services();
        let faults = faults.clone();
        let process = std::thread::spawn(move || {
            let home = process.graph().unwrap().home();
            if home.health().state() != HomeHealthState::Failed {
                faults.fail_next(FaultPoint::BeforeReadConfirmation);
                let _ = home.home_revision();
                let deadline = Instant::now() + Duration::from_secs(5);
                while home.health().state() != HomeHealthState::Failed {
                    assert!(
                        Instant::now() < deadline,
                        "ordinary recovery fixture did not fail its original generation"
                    );
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            process
        })
        .join()
        .unwrap();
        owner.borrow_mut().test_restore_services(process);
    }
    cx.update(|app| RunningProcessOwner::test_observe_running_home_failure(&owner, app));
    if stale_state {
        wait(
            cx,
            |_| {
                owner
                    .borrow()
                    .automatic_recovery_failure()
                    .is_some_and(|failure| failure.is_some())
            },
            "stale first window was not refused by original candidate authentication",
        );
        cx.update(|app| {
            assert!(
                window
                    .read(app)
                    .unwrap()
                    .controller()
                    .unwrap()
                    .composer_mount()
                    .is_none()
            );
            if !owner.borrow().test_services_on_worker() {
                assert!(owner.borrow().test_services().graph().is_none());
            }
            owner.borrow().cancel_automatic_recovery();
        });
        wait(
            cx,
            |_| {
                matches!(
                    owner.borrow().automatic_recovery_outcome().as_deref(),
                    Some(InterruptedExitRecoveryOutcome::Cancelled)
                )
            },
            "stale first window cancellation did not settle",
        );
        dispose_refused(owner, window, cx);
        assert_reopens(&directory);
        return;
    }
    if let Some(release) = opening_release {
        cx.update(|app| {
            assert!(
                owner
                    .borrow()
                    .test_services()
                    .graph()
                    .unwrap()
                    .runtime_setup()
                    .has_pending_flights()
            );
            assert!(
                owner
                    .borrow()
                    .test_running_home_recovery_identity()
                    .is_none()
            );
            RunningProcessOwner::test_observe_running_home_failure(&owner, app);
            let close = owner
                .borrow()
                .window_exit_command(exact_window.window_id(), app)
                .unwrap();
            assert!(close.disabled_reason().is_some());
            close.request_close();
            close.request_exit();
            assert!(!owner.borrow().exit_requested());
        });
        release.send(()).unwrap();
    }
    if reject_release {
        wait(
            cx,
            |_| {
                matches!(
                    owner.borrow().automatic_recovery_outcome().as_deref(),
                    Some(InterruptedExitRecoveryOutcome::Unavailable(_))
                )
            },
            "partial fresh attachment refusal did not settle",
        );
        cx.update(|app| {
            let root = window.read(app).unwrap();
            let mount = root.controller().unwrap().composer_mount().unwrap();
            let resident = mount.read(app).contribution().unwrap();
            assert!(
                resident
                    .read(app)
                    .test_fresh_recovery_release_requests()
                    .is_some(),
                "actual rejected widget release requests must remain owned"
            );
            assert!(!owner.borrow().exit_requested());
        });
        let settled = Rc::new(std::cell::Cell::new(false));
        let done = settled.clone();
        let retained = owner.clone();
        cx.update(|app| {
            app.spawn(async move |cx| {
                RunningProcessOwner::test_settle_running_home_recovery_cancellation(&retained, cx)
                    .await
                    .unwrap();
                done.set(true);
            })
            .detach();
        });
        wait(
            cx,
            |_| settled.get(),
            "actual fresh widget release retry did not settle",
        );
        cx.update(|app| {
            assert!(
                window
                    .read(app)
                    .unwrap()
                    .controller()
                    .unwrap()
                    .composer_mount()
                    .is_none()
            );
            RunningProcessOwner::test_retry_running_home_recovery(&owner, app);
        });
    }
    if cancel_publication {
        wait(
            cx,
            |_| {
                matches!(
                    owner.borrow().automatic_recovery_outcome().as_deref(),
                    Some(InterruptedExitRecoveryOutcome::Cancelled)
                )
            },
            "first conversation publication cancellation did not settle",
        );
        cx.update(|app| {
            let retained = owner.borrow();
            assert!(retained.test_services().graph().is_none());
            assert!(
                window
                    .read(app)
                    .unwrap()
                    .controller()
                    .unwrap()
                    .composer_mount()
                    .is_none()
            );
            assert!(!retained.exit_requested());
            drop(retained);
            RunningProcessOwner::test_retry_running_home_recovery(&owner, app);
        });
    }
    let completion_deadline = Instant::now() + Duration::from_secs(19);
    wait(
        cx,
        |_| match owner.borrow().automatic_recovery_outcome().as_deref() {
            Some(InterruptedExitRecoveryOutcome::Completed) => true,
            Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => panic!(
                "committed first conversation unavailable: {error}; {}",
                recovery_stage(&owner)
            ),
            Some(InterruptedExitRecoveryOutcome::Cancelled) => {
                panic!("committed first conversation cancelled")
            }
            _ => {
                assert!(
                    Instant::now() < completion_deadline,
                    "committed first conversation pending: {}",
                    recovery_stage(&owner)
                );
                false
            }
        },
        "ordinary committed first conversation recovery did not complete",
    );
    cx.update(|app| {
        let retained = owner.borrow();
        assert_eq!(retained.test_process().windows.shells()[0].window(), window);
        assert!(!retained.exit_requested());
        let graph = retained.test_services().graph().unwrap();
        assert_ne!(
            graph.home().health().generation().unwrap(),
            original_generation
        );
        assert_eq!(graph.home().health().state(), HomeHealthState::Healthy);
        let root = window.read(app).unwrap();
        let controller = root.controller().unwrap();
        assert!(!controller.is_threadless());
        assert_eq!(controller.placement(), exact_window.placement());
        let mount = controller.composer_mount().unwrap();
        let resident = mount.read(app).contribution().unwrap();
        let input = resident.read(app).gpui_input();
        assert!(
            input.read(app).realization_diagnostics().frame_generation > 0,
            "published first conversation requires an actual widget realization frame"
        );
        let selection = resident.read(app).selection_identity();
        assert_eq!(selection.claim().thread_id(), thread);
        assert_eq!(selection.binding().candidate().draft_id(), draft);
        assert_eq!(selection.claim(), exact_window.selected_thread().unwrap());
        assert!(root.test_first_conversation_transcript_claim(selection.claim(), app));
        assert!(
            retained
                .window_exit_command(exact_window.window_id(), app)
                .unwrap()
                .disabled_reason()
                .is_none()
        );
    });
    dispose_recovered(owner, window, cx);
    assert_reopens(&directory);
}
