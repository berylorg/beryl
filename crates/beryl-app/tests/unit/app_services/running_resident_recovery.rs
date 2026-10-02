#[allow(dead_code)]
#[path = "running_resident_recovery_support.rs"]
pub(super) mod resident_fixture;

mod attachment {
    use super::*;
    include!("running_resident_attachment.rs");
}

mod attachment_driver {
    use super::*;
    include!("recovery_resident_driver_support.rs");
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
    DrivenAttachment,
    DrivenCancelledAttachment,
    DrivenStaleAttachment,
    DrivenCapacityAttachment,
    DrivenPendingCancellation,
    DrivenAppearanceRefusal,
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
                        let job = owner
                            .borrow()
                            .test_services()
                            .prepare_shutdown_observation()
                            .unwrap();
                        let idle = cx
                            .background_executor()
                            .spawn(async move {
                                job.collect(&ProjectionCancellationToken::new()).unwrap()
                            })
                            .await;
                        cx.update(|app| {
                            owner.borrow_mut().try_begin_idle_shutdown(
                                invoking,
                                crate::running_owner::ShutdownIntent::ApplicationExit,
                                &idle,
                                app,
                            )
                        })
                        .unwrap()
                        .unwrap();
                        let (resident_fixture::Resident {
                            window,
                            composer,
                            close,
                            candidate,
                            retired,
                            directory: resident_directory,
                            mount,
                            drafts,
                            mut shell,
                        }, previous_appearance) = resident_fixture::prepare_for_binding(cx).await;
                        let drafts = Rc::new(RefCell::new(drafts));
                        owner
                            .borrow_mut()
                            .test_replace_recovery_drafts(Some(drafts.clone()));
                        eprintln!("running graph fixture: {}", resident_directory.path().display());
                        let home = candidate.home_id();
                        let generation = candidate.generation();
                        let reference = candidate.service_reference();
                        let graph = cx.background_executor().spawn(async move {
                            crate::app_services::recovery_graph::resident_test_support::prepared(candidate)
                        }).await;
                        let graph_appearance = graph.appearance();
                        owner.borrow_mut().test_retain_resident_recovery(
                            &request,
                            (window.into(), composer.entity_id(), close),
                            graph,
                        );
                        let appearance = cx.update(|app| {
                            crate::theme_runtime::GpuiAppearanceWindowSet::new(
                                graph_appearance,
                                std::num::NonZeroUsize::new(4).unwrap(), app,
                            )
                        }).unwrap();
                        let mut adapters = None;
                        let current = Rc::new(RefCell::new(None));
                        let captured_current = current.clone();
                        let cleanup = Rc::new(RefCell::new(None));
                        let captured_cleanup = cleanup.clone();
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
                        cx.update(|app| {
                                assert!(
                                    RunningProcessOwner::prepare_interrupted_exit_resident(
                                        &owner,
                                        &foreign,
                                        &composer,
                                        close,
                                        window.into(),
                                        generation,
                                        &mut retired,
                                        resident_fixture::environment,
                                        app,
                                        |_, _| panic!("foreign admission"),
                                    )
                                    .is_err()
                                );
                                assert!(retired.is_some());
                                for (next, expected) in [
                                    (None, "graph retirement has not returned"),
                                    (
                                        Some(Err("injected retirement failure".into())),
                                        "graph retirement has not returned",
                                    ),
                                    (Some(Ok(())), "injected retirement failure"),
                                ] {
                                    let error =
                                        RunningProcessOwner::prepare_interrupted_exit_resident(
                                            &owner,
                                            &request,
                                            &composer,
                                            close,
                                            window.into(),
                                            generation,
                                            &mut retired,
                                            |_, _, _| {
                                                panic!("unretired graph admitted preparation")
                                            },
                                            app,
                                            |_, _| panic!("unretired graph delivered completion"),
                                        )
                                        .err()
                                        .expect("preparation requires successful graph retirement");
                                    assert!(error.contains(expected), "{error}");
                                    assert!(retired.is_some());
                                    assert_eq!(retired.as_ref().unwrap().close_ticket(), close);
                                    assert_eq!(completions.get(), 0);
                                    let resident = composer.read(app);
                                    assert_eq!(
                                        *resident.recovery_snapshot().unwrap().restoration(),
                                        original
                                    );
                                    assert!(!resident.gpui_input().read(app).is_enabled());
                                    owner.borrow().test_set_resident_graph_retirement(next);
                                }
                                let foreign_window =
                                    owner.borrow().test_process().windows.shells()[0].window();
                                assert!(
                                    RunningProcessOwner::prepare_interrupted_exit_resident(
                                        &owner,
                                        &request,
                                        &composer,
                                        close,
                                        foreign_window.into(),
                                        generation,
                                        &mut retired,
                                        resident_fixture::environment,
                                        app,
                                        |_, _| panic!("unassociated resident"),
                                    )
                                    .err()
                                    .unwrap()
                                    .contains("not captured")
                                );
                                assert!(retired.is_some());
                                let old_generation = composer.read(app).recovery_snapshot().unwrap()
                                    .selection().binding().home_generation();
                                assert!(RunningProcessOwner::prepare_interrupted_exit_resident(
                                    &owner, &request, &composer, close, window.into(),
                                    old_generation, &mut retired, resident_fixture::environment,
                                    app, |_, _| panic!("old generation admitted"),
                                ).is_err());
                                owner.borrow().interrupted_exit_services_result(&request).unwrap();
                                assert!(retired.is_some());
                                adapters = Some(owner.borrow().interrupted_exit_composer_adapters(
                                    &request, home, generation,
                                    crate::app_services::tests::configuration().projection.turn_start_admission_requirement(),
                                ).unwrap());
                            }).unwrap();
                        let layout = cx.update(|app| composer.read(app).gpui_input().read(app).resident_layout_snapshot()).unwrap();
                        let admit = |app: &mut gpui::App| {
                                let key = RunningProcessOwner::prepare_interrupted_exit_resident(
                                    &owner,
                                    &request,
                                    &composer,
                                    close,
                                    window.into(),
                                    generation,
                                    &mut retired,
                                    move |seed, selection, window| {
                                        if scenario == ResidentScenario::Environment {
                                            Err("injected environment refusal".into())
                                        } else {
                                            let (environment, mut capacity) =
                                                resident_fixture::environment_with_layout(
                                                    seed, selection, window,
                                                    (scenario == ResidentScenario::DrivenAppearanceRefusal).then_some(&layout),
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
                                assert!(retired.is_none());
                                assert!(owner.borrow().interrupted_exit_services_result(&request).is_err());
                                assert!(RunningProcessOwner::cancel_interrupted_exit_services(
                                    &owner, &request, app, |_, _| panic!("overlapping disposal"),
                                ).is_err());
                                assert!(
                                    RunningProcessOwner::prepare_interrupted_exit_resident(
                                        &owner,
                                        &request,
                                        &composer,
                                        close,
                                        window.into(),
                                        generation,
                                        &mut retired,
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
                                        &mut None,
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
                                Ok(key)
                        };
                        let key = if matches!(scenario, ResidentScenario::DrivenAttachment | ResidentScenario::DrivenPendingCancellation) {
                            attachment_driver::interrupt_pending(
                                &owner, &request, admit, window, &appearance, &mut adapters,
                                scenario == ResidentScenario::DrivenPendingCancellation, cx,
                            ).await
                        } else {
                            cx.update(admit).unwrap().unwrap()
                        };
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
                                | ResidentScenario::DrivenAttachment
                                | ResidentScenario::DrivenCancelledAttachment
                                | ResidentScenario::DrivenStaleAttachment
                                | ResidentScenario::DrivenCapacityAttachment
                                | ResidentScenario::DrivenAppearanceRefusal
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
                            ResidentScenario::DrivenAttachment
                                | ResidentScenario::DrivenCancelledAttachment
                                | ResidentScenario::DrivenStaleAttachment
                                | ResidentScenario::DrivenCapacityAttachment
                                | ResidentScenario::DrivenAppearanceRefusal
                        ) {
                            owner.borrow_mut().test_process_mut().windows.test_swap_recovery_shell(&mut shell);
                            cx.update(|app| {
                                previous_appearance.update(app, |set, _| set.retire());
                            }).unwrap();
                            let attached = attachment_driver::attempt(
                                &owner, &request, &key, window, &appearance, &previous_appearance, &composer, &mount, &drafts,
                                close, &mut adapters, current.borrow().unwrap(), scenario, cx,
                            ).await;
                            owner.borrow_mut().test_process_mut().windows.test_swap_recovery_shell(&mut shell);
                            attached
                        } else if matches!(
                            scenario,
                            ResidentScenario::Attach
                                | ResidentScenario::StaleAttachment
                                | ResidentScenario::CancelledAttachment
                                | ResidentScenario::CapacityAttachment
                        ) {
                            window
                                .update(cx, |root, window, app| {
                                    attachment::attempt(
                                        &owner,
                                        &request,
                                        &key,
                                        &mount,
                                        root,
                                        &drafts,
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
                            false
                        };
                        if attached {
                            window
                                .update(cx, |_, window, app| {
                                    let input = composer.read(app).gpui_input();
                                    input.update(app, |input, cx| {
                                        assert!(input.dispose(window, cx).is_empty())
                                    });
                                })
                                .unwrap();
                        } else {
                            let resources = attachment_driver::drain(
                                &owner, &request, &key,
                                scenario == ResidentScenario::DrivenCapacityAttachment, cx,
                            ).await;
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
                                            generation,
                                            &mut retired,
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
                            drop(resources);
                        };
                        drop(adapters);
                        drop(owner.borrow_mut().test_replace_recovery_drafts(None));
                        drop(drafts);
                        if scenario != ResidentScenario::Window {
                            window
                                .update(cx, |_, window, _| window.remove_window())
                                .unwrap();
                        }
                        drop((composer, mount, shell));
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
                        owner.borrow_mut().test_replace_interrupted_exit_request(&request);
                        owner.borrow().interrupted_exit_services_result(&request).unwrap();
                        drop(owner.borrow().interrupted_exit_composer_adapters(
                            &request, home, generation,
                            crate::app_services::tests::configuration().projection.turn_start_admission_requirement(),
                        ).unwrap());
                        assert_eq!(reference.health().state(), beryl_home_store::HomeHealthState::Reopening);
                        let (disposed, disposal) = futures_channel::oneshot::channel();
                        cx.update(|app| {
                            RunningProcessOwner::cancel_interrupted_exit_services(
                                &owner, &request, app, move |_, _| { disposed.send(()).unwrap(); },
                            ).unwrap();
                        }).unwrap();
                        disposal.await.unwrap();
                        let failure = owner.borrow().test_take_resident_graph_failure();
                        cx.background_executor().spawn(async move { failure.close().unwrap(); }).await;
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
