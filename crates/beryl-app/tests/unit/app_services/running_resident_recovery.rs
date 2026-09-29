#[allow(dead_code)]
#[path = "running_resident_recovery_support.rs"]
mod resident_fixture;

mod attachment {
    use super::*;
    include!("running_resident_attachment.rs");
}

#[test]
fn native_recovery_owner_drives_resident_and_retains_abandoned_delivery() {
    resident_run(ResidentScenario::Ready);
}
#[test]
fn native_recovery_owner_cancels_pending_authentication() {
    resident_run(ResidentScenario::Cancel);
}
#[test]
fn native_recovery_owner_rejects_stale_request_on_return() {
    resident_run(ResidentScenario::Stale);
}
#[test]
fn native_recovery_owner_cleans_up_environment_refusal() {
    resident_run(ResidentScenario::Environment);
}
#[test]
fn native_recovery_owner_cleans_up_abandoned_window() {
    resident_run(ResidentScenario::Window);
}

#[derive(Clone, Copy, PartialEq)]
enum ResidentScenario {
    Ready,
    Cancel,
    Stale,
    Environment,
    Window,
    SuspendedFrame,
    Capacity,
    Attach,
    StaleAttachment,
    CancelledAttachment,
    CapacityAttachment,
}

#[test]
fn native_recovery_owner_cancels_a_suspended_frame() {
    resident_run(ResidentScenario::SuspendedFrame);
}

#[test]
fn native_recovery_owner_cleans_up_capacity_refusal() {
    resident_run(ResidentScenario::Capacity);
}

fn resident_run(scenario: ResidentScenario) {
    let directory = support::native_home();
    let configuration = input(directory.path(), |path, _| support::open(path));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                configuration,
                move |result, app| {
                    let StartupCompletion::Running(running) = result else {
                        panic!("startup failed")
                    };
                    let invoking = running.windows.window_ids()[0];
                    let owner = RunningProcessOwner::start(running, app);
                    owner
                        .borrow()
                        .window_exit_command(invoking, app)
                        .unwrap()
                        .request_exit();
                    app.spawn(async move |cx| {
                        let request = next_request(&owner, cx).await;
                        let resident_fixture::Resident {
                            window,
                            composer,
                            close,
                            mut candidate,
                            retired,
                            directory: resident_directory,
                            mount,
                        } = resident_fixture::prepare(cx).await;
                        owner.borrow_mut().test_retain_resident_recovery(
                            &request,
                            (window.into(), composer.entity_id(), close),
                        );
                        let state = BerylState::reacquire_candidate(&candidate).unwrap();
                        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
                        let generation = candidate.generation();
                        let mut adapters = Some(
                            crate::app_services::recovery_composer::test_support::adapters(
                                &mut candidate,
                            ),
                        );
                        let current = Rc::new(RefCell::new(None));
                        let captured_current = current.clone();
                        let cleanup = Rc::new(RefCell::new(None));
                        let captured_cleanup = cleanup.clone();
                        let mut candidate = Some(crate::running_owner::InterruptedExitCandidate {
                            candidate,
                            session: state.session(),
                        });
                        let mut retired = Some(retired);
                        let original = composer
                            .read_with(cx, |composer, _| {
                                *composer.recovery_snapshot().unwrap().restoration()
                            })
                            .unwrap();
                        let (sender, receiver) = futures_channel::oneshot::channel::<()>();
                        drop(receiver);
                        let completions = Rc::new(Cell::new(0));
                        let completed = completions.clone();
                        let foreign = request.test_foreign();
                        let key = cx
                            .update(|app| {
                                assert!(
                                    RunningProcessOwner::prepare_interrupted_exit_resident(
                                        &owner,
                                        &foreign,
                                        &composer,
                                        close,
                                        window.into(),
                                        &mut candidate,
                                        &mut retired,
                                        storage.clone(),
                                        state.clone(),
                                        resident_fixture::environment,
                                        app,
                                        |_, _| panic!("foreign admission"),
                                    )
                                    .is_err()
                                );
                                assert!(candidate.is_some() && retired.is_some());
                                let foreign_window =
                                    owner.borrow().test_process().windows.shells()[0].window();
                                assert!(
                                    RunningProcessOwner::prepare_interrupted_exit_resident(
                                        &owner,
                                        &request,
                                        &composer,
                                        close,
                                        foreign_window.into(),
                                        &mut candidate,
                                        &mut retired,
                                        storage.clone(),
                                        state.clone(),
                                        resident_fixture::environment,
                                        app,
                                        |_, _| panic!("unassociated resident"),
                                    )
                                    .err()
                                    .unwrap()
                                    .contains("not captured")
                                );
                                assert!(candidate.is_some() && retired.is_some());
                                let key = RunningProcessOwner::prepare_interrupted_exit_resident(
                                    &owner,
                                    &request,
                                    &composer,
                                    close,
                                    window.into(),
                                    &mut candidate,
                                    &mut retired,
                                    storage.clone(),
                                    state.clone(),
                                    move |seed, selection, window| {
                                        if scenario == ResidentScenario::Environment {
                                            Err("injected environment refusal".into())
                                        } else {
                                            let (environment, mut capacity) =
                                                resident_fixture::environment(
                                                    seed, selection, window,
                                                )?;
                                            if scenario == ResidentScenario::Capacity {
                                                capacity.bytes = 0;
                                                capacity.items = 0;
                                            }
                                            *captured_current.borrow_mut() =
                                                Some(gpui_text_input::RangePrepublicationCurrent {
                                                    binding: seed.binding,
                                                    history: seed.history,
                                                    available_capacity:
                                                        gpui_text_input::RangeSurfaceCharge {
                                                            bytes: capacity.bytes / 2,
                                                            items: capacity.items / 2,
                                                        },
                                                });
                                            *captured_cleanup.borrow_mut() =
                                                Some(environment.clone());
                                            Ok((environment, capacity))
                                        }
                                    },
                                    app,
                                    move |_, _| {
                                        completed.set(completed.get() + 1);
                                        let _ = sender.send(());
                                    },
                                )
                                .unwrap();
                                assert!(candidate.is_none() && retired.is_none());
                                assert!(
                                    RunningProcessOwner::prepare_interrupted_exit_resident(
                                        &owner,
                                        &request,
                                        &composer,
                                        close,
                                        window.into(),
                                        &mut candidate,
                                        &mut retired,
                                        storage.clone(),
                                        state.clone(),
                                        resident_fixture::environment,
                                        app,
                                        |_, _| panic!("duplicate admission"),
                                    )
                                    .is_err()
                                );
                                assert!(
                                    RunningProcessOwner::settle_interrupted_exit_candidate(
                                        &owner,
                                        &request,
                                        &mut candidate,
                                        app,
                                        |_, _| panic!("overlapping settlement")
                                    )
                                    .is_err()
                                );
                                assert!(
                                    RunningProcessOwner::retire_interrupted_exit_graph(
                                        &owner,
                                        &request,
                                        generation,
                                        app,
                                        |_, _| panic!("overlapping retirement")
                                    )
                                    .is_err()
                                );
                                assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                                if scenario == ResidentScenario::Cancel {
                                    RunningProcessOwner::cancel_interrupted_exit_resident(
                                        &owner, &key, app,
                                    )
                                    .unwrap();
                                    assert!(
                                        owner
                                            .borrow_mut()
                                            .take_cancelled_resident_preparation(&key)
                                            .is_none()
                                    );
                                }
                                if scenario == ResidentScenario::Stale {
                                    owner
                                        .borrow_mut()
                                        .test_replace_interrupted_exit_request(&foreign);
                                }
                                if scenario == ResidentScenario::Window {
                                    window
                                        .update(app, |_, window, _| window.remove_window())
                                        .unwrap();
                                }
                                if scenario == ResidentScenario::SuspendedFrame {
                                    window
                                        .update(app, |_, window, _| window.minimize_window())
                                        .unwrap();
                                }
                                key
                            })
                            .unwrap();
                        let deadline = Instant::now() + Duration::from_secs(5);
                        if scenario == ResidentScenario::SuspendedFrame {
                            loop {
                                let waiting = cx
                                    .update(|app| {
                                        let (pending, scheduled, _, _, _) =
                                            owner.borrow().test_resident_preparation_state();
                                        if pending || !scheduled {
                                            return true;
                                        }
                                        for _ in 0..3 {
                                            RunningProcessOwner::cancel_interrupted_exit_resident(
                                                &owner, &key, app,
                                            )
                                            .unwrap();
                                        }
                                        false
                                    })
                                    .unwrap();
                                if !waiting {
                                    break;
                                }
                                assert!(Instant::now() < deadline);
                                cx.background_executor()
                                    .timer(Duration::from_millis(5))
                                    .await;
                            }
                        }
                        while completions.get() == 0 {
                            assert!(
                                Instant::now() < deadline,
                                "resident preparation did not settle: {:?}",
                                owner.borrow().test_resident_preparation_state()
                            );
                            cx.background_executor()
                                .timer(Duration::from_millis(5))
                                .await;
                        }
                        let result = owner.borrow().interrupted_exit_resident_result(&key);
                        if matches!(
                            scenario,
                            ResidentScenario::Ready
                                | ResidentScenario::Attach
                                | ResidentScenario::StaleAttachment
                                | ResidentScenario::CancelledAttachment
                                | ResidentScenario::CapacityAttachment
                        ) {
                            assert_eq!(
                                result.unwrap(),
                                crate::main_window::MainWindowComposerRecoveryProgress::Ready
                            );
                        } else {
                            assert!(result.is_err());
                        }
                        composer
                            .read_with(cx, |composer, app| {
                                let input = composer.gpui_input();
                                assert!(!input.read(app).is_enabled());
                                assert_eq!(
                                    *composer.recovery_snapshot().unwrap().restoration(),
                                    original
                                );
                            })
                            .unwrap();
                        let attached = if matches!(
                            scenario,
                            ResidentScenario::Attach
                                | ResidentScenario::StaleAttachment
                                | ResidentScenario::CancelledAttachment
                                | ResidentScenario::CapacityAttachment
                        ) {
                            window
                                .update(cx, |_, window, app| {
                                    attachment::attempt(
                                        &owner,
                                        &request,
                                        &key,
                                        &mount,
                                        &composer,
                                        close,
                                        &mut adapters,
                                        current.borrow().unwrap(),
                                        scenario,
                                        window,
                                        app,
                                    )
                                })
                                .unwrap()
                        } else {
                            None
                        };
                        let recovered = if let Some(candidate) = attached {
                            window
                                .update(cx, |_, window, app| {
                                    let input = composer.read(app).gpui_input();
                                    input.update(app, |input, cx| {
                                        assert!(input.dispose(window, cx).is_empty())
                                    });
                                })
                                .unwrap();
                            candidate
                        } else {
                            cx.update(|app| {
                                RunningProcessOwner::cancel_interrupted_exit_resident(
                                    &owner, &key, app,
                                )
                            })
                            .unwrap()
                            .unwrap();
                            let resources = loop {
                                if let Some(resources) =
                                    owner.borrow_mut().take_cancelled_resident_preparation(&key)
                                {
                                    break resources;
                                }
                                assert!(Instant::now() < deadline);
                                cx.background_executor()
                                    .timer(Duration::from_millis(5))
                                    .await;
                            };
                            assert!(
                                owner
                                    .borrow_mut()
                                    .take_cancelled_resident_preparation(&key)
                                    .is_none()
                            );
                            assert_eq!(completions.get(), 1);
                            if scenario == ResidentScenario::SuspendedFrame {
                                let refusal = cx
                                    .update(|app| {
                                        RunningProcessOwner::prepare_interrupted_exit_resident(
                                            &owner,
                                            &request,
                                            &composer,
                                            close,
                                            window.into(),
                                            &mut candidate,
                                            &mut retired,
                                            storage.clone(),
                                            state.clone(),
                                            resident_fixture::environment,
                                            app,
                                            |_, _| panic!("second suspended frame"),
                                        )
                                    })
                                    .unwrap()
                                    .err()
                                    .unwrap();
                                assert!(refusal.contains("frame has not returned"));
                            }
                            assert_eq!(resources.candidate.candidate.generation(), generation);
                            let resources = *resources;
                            drop(resources.source);
                            resources.candidate
                        };
                        drop(adapters);
                        if scenario != ResidentScenario::Window {
                            window
                                .update(cx, |_, window, _| window.remove_window())
                                .unwrap();
                        }
                        drop((composer, mount, storage, state));
                        cx.background_executor()
                            .timer(Duration::from_millis(200))
                            .await;
                        if let Some(environment) = cleanup.borrow_mut().take() {
                            let ownership = environment.cleanup().ownership();
                            assert_eq!(
                                (
                                    ownership.active,
                                    ownership.ready,
                                    ownership.awaiting_acknowledgement
                                ),
                                (0, 0, 0)
                            );
                        }
                        cx.background_executor()
                            .spawn(async move {
                                drop(recovered.session);
                                recovered.candidate.abort().close().unwrap();
                            })
                            .await;
                        resident_directory.close().unwrap();
                        let running = Rc::try_unwrap(owner)
                            .ok()
                            .unwrap()
                            .into_inner()
                            .test_into_process();
                        support::dispose_running(running, cx).await;
                        observed.set(true);
                        cx.update(|app| app.quit()).unwrap();
                    })
                    .detach();
                },
                app,
            );
            support::watchdog(app);
        });
    assert!(finished.get());
    assert_reopens(&directory);
}
