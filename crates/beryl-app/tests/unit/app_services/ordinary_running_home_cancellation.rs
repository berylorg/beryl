use super::*;

#[derive(Clone, Copy, PartialEq)]
enum Cancellation {
    Reopening,
    ResidentPreparation,
    PublicationValidation,
    PartialAttachment,
    AfterPublication,
}

#[test]
fn native_ordinary_failed_home_cancelled_reopen_retains_fenced_window_custody() {
    run(Cancellation::Reopening);
}

#[test]
fn native_ordinary_failed_home_cancelled_resident_preparation_joins_before_disposal() {
    run(Cancellation::ResidentPreparation);
}

#[test]
fn native_ordinary_failed_home_cancelled_publication_validation_disposes_candidate_bindings() {
    run(Cancellation::PublicationValidation);
}

#[test]
fn native_ordinary_failed_home_partial_attachment_refusal_retains_then_disposes_exact_custody() {
    run(Cancellation::PartialAttachment);
}

#[test]
fn native_ordinary_failed_home_late_cancellation_preserves_published_graph_for_shutdown() {
    run(Cancellation::AfterPublication);
}

fn run(cancellation: Cancellation) {
    let directory = resident_fixture::selected_home_with_windows(2);
    eprintln!(
        "native ordinary cancelled recovery fixture: {}",
        directory.path().display()
    );
    let faults = FaultController::new();
    let opening_faults = faults.clone();
    let mut configuration = input(directory.path(), move |path, _| {
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
            opening_faults.clone(),
        )
        .unwrap();
        let state = beryl_state::BerylState::register(&mut candidate).unwrap();
        let syndic = syndic_storage::SyndicStorage::register(&mut candidate).unwrap();
        let candidate = candidate
            .prepare_publication(
                beryl_state::BerylState::required_domains()
                    .unwrap()
                    .merge(syndic_storage::SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap();
        StartupHomeOpen::Ready {
            candidate,
            state,
            syndic,
        }
    });
    configuration.windows = resident_fixture::selected_inputs();
    let completed = Rc::new(Cell::new(false));
    let observed = completed.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                configuration,
                move |result, app| {
                    let StartupCompletion::Running(process) = result else {
                        panic!("ordinary cancellation fixture startup failed")
                    };
                    let owner = RunningProcessOwner::start(process, app);
                    app.spawn(async move |cx| {
                        let original_windows = windows(&owner);
                        assert_eq!(original_windows.len(), 2);
                        let mut residents = Vec::new();
                let mut natives = Vec::new();
                        for window in &original_windows {
                            residents.push(composer(*window, cx).await);
                    natives.push(native(*window, cx).await);
                }
                let original_selections = cx.update(|app| residents.iter().map(|resident| resident.read(app).selection_identity()).collect::<Vec<_>>()).unwrap();
                let original_session = snapshot(&owner);
                let (original_generation, session_revision) = {
                    let retained = owner.borrow();
                    let graph = retained.test_services().graph().unwrap();
                    (graph.home().health().generation().unwrap(), graph.state().session().revision(graph.home()).unwrap())
                };
                        let home = owner
                            .borrow()
                            .test_services()
                            .graph()
                            .unwrap()
                            .home()
                            .service_reference();
                        let read_faults = faults.clone();
                        cx.background_executor()
                            .spawn(async move {
                                read_faults.fail_next(FaultPoint::BeforeReadConfirmation);
                                assert!(home.home_revision().is_err());
                                assert_eq!(home.health().state(), HomeHealthState::Failed);
                            })
                            .await;
                        let blocked = faults.block_next(FaultPoint::BeforeReopen);
                        match cancellation {
                            Cancellation::Reopening => {}
                            Cancellation::ResidentPreparation => owner
                                .borrow_mut()
                                .test_cancel_recovery_after_resident_admission(),
                    Cancellation::PublicationValidation => owner
                        .borrow_mut()
                        .test_cancel_recovery_before_publication_validation(),
                    Cancellation::PartialAttachment => owner.borrow_mut().test_reject_ordinary_recovery_attachment_after(1),
                    Cancellation::AfterPublication => owner.borrow_mut().test_cancel_recovery_after_publication(),
                        }
                        cx.update(|app| {
                            RunningProcessOwner::test_observe_running_home_failure(&owner, app)
                        })
                        .unwrap();
                        let identity = owner
                            .borrow()
                            .test_running_home_recovery_identity()
                            .unwrap();
                        let blocked = cx
                            .background_executor()
                            .spawn(async move {
                                assert!(
                                    blocked.wait_until_reached(Duration::from_secs(10)),
                                    "cancelled recovery never reached candidate reopening"
                                );
                                blocked
                            })
                            .await;
                        if cancellation == Cancellation::Reopening {
                            owner.borrow().cancel_automatic_recovery();
                        }
                        blocked.release();
                wait_until(
                    cx,
                    || {
                        match owner.borrow().automatic_recovery_outcome().as_deref() {
                            Some(InterruptedExitRecoveryOutcome::Unavailable(_)) => matches!(cancellation, Cancellation::PartialAttachment | Cancellation::AfterPublication),
                            Some(InterruptedExitRecoveryOutcome::Cancelled) => !matches!(cancellation, Cancellation::PartialAttachment | Cancellation::AfterPublication),
                            _ => false,
                        }
                    },
                    "ordinary cancelled candidate cleanup",
                )
                .await;
                if cancellation == Cancellation::PartialAttachment {
                    cx.update(|app| {
                        let first = residents[0].read(app).selection_identity();
                        let second = residents[1].read(app).selection_identity();
                        assert_eq!(first.claim(), original_selections[0].claim());
                        assert_ne!(first.binding().home_generation(), original_selections[0].binding().home_generation());
                        assert_eq!(second, original_selections[1]);
                        assert_eq!(windows(&owner), original_windows);
                        for resident in &residents {
                            assert!(!resident.read(app).gpui_input().read(app).is_enabled());
                        }
                    }).unwrap();
                    assert!(owner.borrow().test_services().graph().is_none());
                    RunningProcessOwner::test_settle_running_home_recovery_cancellation(&owner, cx).await.unwrap();
                    cx.update(|app| {
                        for (index, window) in original_windows.iter().enumerate() {
                            let mount = window.read(app).unwrap().controller().unwrap().composer_mount().unwrap();
                            assert_eq!(mount.update(app, |mount, _| mount.test_unpublished_recovery_detached()), index == 0);
                        }
                        assert_eq!(residents[1].read(app).selection_identity(), original_selections[1]);
                        for resident in &residents {
                            assert!(!resident.read(app).gpui_input().read(app).is_enabled());
                        }
                        assert_eq!(windows(&owner), original_windows);
                    }).unwrap();
                }
                        assert!(!owner.borrow().exit_requested());
                        assert!(owner.borrow().shutdown_session().is_none());
                assert!(owner.borrow().shutdown_status().is_none());
                assert!(!owner.borrow().test_services_on_worker());
                if cancellation == Cancellation::AfterPublication {
                    let retained = owner.borrow();
                    let services = retained.test_services();
                    let graph = services.graph().unwrap();
                    assert_eq!(graph.home().health().state(), HomeHealthState::Healthy);
                    assert_ne!(graph.home().health().generation().unwrap(), original_generation);
                    assert_eq!(graph.state().session().revision(graph.home()).unwrap(), session_revision);
                    assert_eq!(graph.state().session().minimal_bootstrap(graph.home()).unwrap().unwrap(), original_session);
                    assert!(services.process.execution_permit().reserve().is_err());
                    drop(retained);
                    assert!(RunningProcessOwner::test_settle_running_home_recovery_cancellation(&owner, cx).await.is_err());
                } else {
                    assert!(owner.borrow().test_services().graph().is_none());
                }
                        assert!(Rc::ptr_eq(
                            &identity,
                            &owner
                                .borrow()
                                .test_running_home_recovery_identity()
                                .unwrap()
                        ));
                        cx.update(|app| {
                            RunningProcessOwner::test_observe_running_home_failure(&owner, app);
                            assert!(Rc::ptr_eq(
                                &identity,
                                &owner
                                    .borrow()
                                    .test_running_home_recovery_identity()
                                    .unwrap()
                            ));
                            assert_eq!(app.windows().len(), original_windows.len());
                            for ((window, resident), handle) in
                                original_windows.iter().zip(&residents).zip(&natives)
                            {
                                assert!(unsafe { IsWindow(Some(*handle)).as_bool() });
                                let root = window.read(app).unwrap();
                                assert!(!root.test_exit_command_enabled());
                                let input = resident.read(app).gpui_input();
                                assert!(!input.read(app).is_enabled());
                                assert!(input.read(app).is_quiescent());
                            }
                        })
                        .unwrap();
                        cx.background_executor()
                            .timer(Duration::from_millis(150))
                            .await;
                assert!(matches!(
                    owner.borrow().automatic_recovery_outcome().as_deref(),
                    Some(InterruptedExitRecoveryOutcome::Cancelled | InterruptedExitRecoveryOutcome::Unavailable(_))
                ));
                assert_eq!(owner.borrow().test_services().graph().is_some(), cancellation == Cancellation::AfterPublication);
                        RunningProcessOwner::test_stop_running_observers(&owner, cx).await.unwrap();
                        drop(residents);
                        drop(identity);
                        let mut running = Rc::try_unwrap(owner)
                            .ok()
                            .expect("settled observers release the mounted GUI owner")
                            .into_inner()
                            .test_into_process();
                        cx.update(|app| {
                            for shell in running.windows.shells() {
                                shell
                                    .window()
                                    .update(app, |_, window, _| window.remove_window())
                                    .unwrap();
                            }
                            running
                                .appearance
                                .update(app, |appearance, _| appearance.retire());
                            drop(running.windows);
                        })
                        .unwrap();
                cx.background_executor()
                    .spawn(async move {
                        if cancellation == Cancellation::AfterPublication {
                            super::super::close(&mut running.services);
                        } else {
                            running.services.test_retired_service_home()
                                .expect("cancelled reopening retains the failed home lock")
                                .close().unwrap();
                        }
                            })
                            .await;
                        for handle in natives {
                            assert!(!unsafe { IsWindow(Some(handle)).as_bool() });
                        }
                        observed.set(true);
                        cx.update(|app| app.quit()).unwrap();
                    })
                    .detach();
                },
                app,
            );
            support::watchdog_with_timeout(app, Duration::from_secs(30));
        });
    assert!(
        completed.get(),
        "cancelled ordinary recovery fixture did not finish cleanup"
    );
    super::super::assert_reopens(&directory);
}
