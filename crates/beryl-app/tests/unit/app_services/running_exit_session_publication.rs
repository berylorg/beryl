use crate::exit_session::ExitSessionExecution;
use crate::running_owner::{
    ExitProgressError, RunningShutdownDraftProgress, RunningShutdownSession,
};

mod graph_retirement_support {
    use super::*;
    include!("interrupted_exit_graph_retirement_support.rs");
}

mod candidate_settlement_support {
    use super::*;
    include!("interrupted_exit_candidate_settlement_support.rs");
}

mod resident_recovery {
    use super::*;
    include!("running_resident_recovery.rs");
}

mod threadless_recovery {
    use super::*;
    include!("running_threadless_attachment.rs");
}

mod retirement_driver {
    use super::*;
    include!("recovery_retirement_driver_support.rs");
}

#[derive(Clone, Copy)]
enum RetirementDelivery {
    Ready,
    Cancelled,
    Stale,
    Dropped,
}

#[test]
fn native_exit_recovery_driver_retires_failed_graph_before_construction() {
    run_with_retirement_delivery(Some(RetirementDelivery::Ready));
}

#[test]
fn native_exit_recovery_driver_retains_cancelled_retirement() {
    run_with_retirement_delivery(Some(RetirementDelivery::Cancelled));
}

#[test]
fn native_exit_recovery_driver_retains_stale_retirement() {
    run_with_retirement_delivery(Some(RetirementDelivery::Stale));
}

#[test]
fn native_exit_recovery_driver_retains_dropped_retirement() {
    run_with_retirement_delivery(Some(RetirementDelivery::Dropped));
}

fn run_with_retirement_delivery(delivery: Option<RetirementDelivery>) {
    run_with_recovery_options(
        Some(FaultPoint::BeforeCommit),
        true,
        false,
        RecoveryPublicationDelivery::Driven,
        delivery,
    );
}

#[test]
fn native_exit_session_publication_delivers_exact_ready_request() {
    run(None, false);
}

#[test]
fn native_exit_session_publication_retains_noncommit() {
    run(Some(FaultPoint::BeforeCommit), false);
}

#[test]
fn native_exit_session_publication_retains_postcommit_failure() {
    run(Some(FaultPoint::AfterPersist), false);
}

#[test]
fn native_exit_session_publication_retains_indeterminate_custody() {
    run(Some(FaultPoint::AfterCommitBeforePersist), false);
}

#[test]
fn native_exit_attempt_session_publication_retains_ready_request() {
    run(None, true);
}

#[test]
fn native_exit_attempt_session_publication_reports_noncommit() {
    run(Some(FaultPoint::BeforeCommit), true);
}

#[test]
fn native_exit_attempt_session_publication_reports_postcommit_failure() {
    run(Some(FaultPoint::AfterPersist), true);
}

#[test]
fn native_exit_attempt_session_publication_reports_indeterminate() {
    run(Some(FaultPoint::AfterCommitBeforePersist), true);
}

#[test]
fn native_exit_attempt_session_publication_retains_settlement_unwind() {
    run_with_settlement_unwind(Some(FaultPoint::BeforeCommit), true, true);
}

#[derive(Clone, Copy)]
enum RecoveryPublicationDelivery {
    Ready,
    Complete,
    Driven,
    DrivenCancelled,
    DrivenStale,
    Stale,
    Cancelled,
    ThemeActivationFailure,
    ThemeActivationUnwind,
    ThemeActivationCancelled,
}

#[test]
fn native_exit_recovery_drives_prepared_graph_to_running() {
    run_with_recovery_delivery(
        Some(FaultPoint::BeforeCommit),
        true,
        false,
        RecoveryPublicationDelivery::Driven,
    );
}

#[test]
fn native_exit_recovery_driver_stops_after_publication_cancellation() {
    run_with_recovery_delivery(
        Some(FaultPoint::BeforeCommit),
        true,
        false,
        RecoveryPublicationDelivery::DrivenCancelled,
    );
}

#[test]
fn native_exit_recovery_driver_rejects_stale_publication_delivery() {
    run_with_recovery_delivery(
        Some(FaultPoint::BeforeCommit),
        true,
        false,
        RecoveryPublicationDelivery::DrivenStale,
    );
}

#[test]
fn native_exit_recovery_completes_cancelled_request_without_replaying_exit() {
    run_with_recovery_delivery(
        Some(FaultPoint::BeforeCommit),
        true,
        false,
        RecoveryPublicationDelivery::Complete,
    );
}

#[test]
fn native_exit_recovery_theme_activation_retains_failure() {
    run_with_recovery_delivery(
        Some(FaultPoint::BeforeCommit),
        true,
        false,
        RecoveryPublicationDelivery::ThemeActivationFailure,
    );
}

#[test]
fn native_exit_recovery_theme_activation_retains_unwind() {
    run_with_recovery_delivery(
        Some(FaultPoint::BeforeCommit),
        true,
        false,
        RecoveryPublicationDelivery::ThemeActivationUnwind,
    );
}

#[test]
fn native_exit_recovery_theme_activation_retains_worker_cancellation() {
    run_with_recovery_delivery(
        Some(FaultPoint::BeforeCommit),
        true,
        false,
        RecoveryPublicationDelivery::ThemeActivationCancelled,
    );
}

#[test]
fn native_exit_recovery_publication_delivers_success() {
    run_with_recovery_delivery(
        Some(FaultPoint::BeforeCommit),
        true,
        false,
        RecoveryPublicationDelivery::Ready,
    );
}

#[test]
fn native_exit_recovery_publication_retains_cancelled_success() {
    run_with_recovery_delivery(
        Some(FaultPoint::BeforeCommit),
        true,
        false,
        RecoveryPublicationDelivery::Cancelled,
    );
}

fn foreign_recovery_candidate() -> (
    tempfile::TempDir,
    crate::running_owner::InterruptedExitCandidate,
) {
    let directory = support::native_home();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    BerylState::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let candidate = home.recover_same_home().unwrap();
    let session = BerylState::reacquire_candidate(&candidate)
        .unwrap()
        .session();
    (
        directory,
        crate::running_owner::InterruptedExitCandidate { candidate, session },
    )
}

fn run(fault: Option<FaultPoint>, consumer: bool) {
    run_with_settlement_unwind(fault, consumer, false);
}

fn run_with_settlement_unwind(fault: Option<FaultPoint>, consumer: bool, settlement_unwind: bool) {
    run_with_recovery_delivery(
        fault,
        consumer,
        settlement_unwind,
        RecoveryPublicationDelivery::Stale,
    );
}

fn run_with_recovery_delivery(
    fault: Option<FaultPoint>,
    consumer: bool,
    settlement_unwind: bool,
    publication_delivery: RecoveryPublicationDelivery,
) {
    run_with_recovery_options(
        fault,
        consumer,
        settlement_unwind,
        publication_delivery,
        None,
    );
}

fn run_with_recovery_options(
    fault: Option<FaultPoint>,
    consumer: bool,
    settlement_unwind: bool,
    publication_delivery: RecoveryPublicationDelivery,
    retirement_delivery: Option<RetirementDelivery>,
) {
    let directory = support::native_home();
    eprintln!(
        "native Exit publication fixture: {}",
        directory.path().display()
    );
    let faults = FaultController::new();
    let opening_faults = faults.clone();
    let input = input(directory.path(), move |path, _| {
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
            opening_faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let syndic = SyndicStorage::register(&mut candidate).unwrap();
        let candidate = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap();
        StartupHomeOpen::Ready {
            candidate,
            state,
            syndic,
        }
    });
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                input,
                move |result, app| {
                    let StartupCompletion::Running(running) = result else {
                        panic!("startup failed")
                    };
                    let invoking = running.windows.window_ids()[0];
                    let window = running.windows.shells()[0].window();
                    let owner = RunningProcessOwner::start(running, app);
                    owner
                        .borrow()
                        .window_exit_command(invoking, app)
                        .unwrap()
                        .request_exit();
                    app.spawn(async move |cx| {
                        let mut recovered_threadless = false;
                        let mut recovered_appearance = None;
                        let request = next_request(&owner, cx).await;
                        let identity = request.identity();
                        let (request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::publish_exit_session(
                                    &owner,
                                    request,
                                    app,
                                    |_, _, _, _| panic!("unadmitted publication"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::Intent));
                        let observation = observe(&owner, ProjectionCancellationToken::new(), cx)
                            .await
                            .unwrap();
                        cx.update(|app| {
                            owner.borrow_mut().try_begin_idle_shutdown(
                                invoking,
                                ShutdownIntent::ApplicationExit,
                                &observation,
                                app,
                            )
                        })
                        .unwrap()
                        .unwrap();
                        let (sender, receiver) = futures_channel::oneshot::channel();
                        assert!(
                            cx.update(|app| RunningProcessOwner::drive_exit(
                                &owner,
                                request,
                                ProjectionCancellationToken::new(),
                                app,
                                move |_, request, result, _| {
                                    assert!(matches!(
                                        result.unwrap(),
                                        AppServiceShutdownProgress::Ready
                                    ));
                                    sender.send(request).ok().unwrap();
                                }
                            ))
                            .unwrap()
                            .is_ok()
                        );
                        let request = receiver.await.unwrap();
                        let (request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::publish_exit_session(
                                    &owner,
                                    request,
                                    app,
                                    |_, _, _, _| panic!("drafts not ready"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::SessionPublication(_)));
                        assert!(owner.borrow().shutdown_session().is_none());
                        cx.update(|app| {
                            assert_eq!(
                                RunningProcessOwner::advance_shutdown_drafts(&owner, app).unwrap(),
                                RunningShutdownDraftProgress::Ready
                            )
                        })
                        .unwrap();
                        let (request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::publish_exit_session(
                                    &owner,
                                    request,
                                    app,
                                    |_, _, _, _| panic!("placements not ready"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::SessionPublication(_)));
                        assert!(owner.borrow().shutdown_session().is_none());
                        if !consumer {
                        let (sender, receiver) = futures_channel::oneshot::channel();
                        cx.update(|app| {
                            RunningProcessOwner::capture_shutdown_placements(
                                &owner,
                                app,
                                move |_, result, _| {
                                    sender.send(result).ok().unwrap();
                                },
                            )
                        })
                        .unwrap()
                        .unwrap();
                        receiver.await.unwrap().unwrap();
                        }
                        let original_attempt =
                            owner.borrow().test_services().graph().unwrap().shutdown;
                        if let Some(fault) = fault {
                            faults.fail_next(fault);
                        }
                        let (sender, receiver) = futures_channel::oneshot::channel();
                        let weak = Rc::downgrade(&owner);
                        let thread = std::thread::current().id();
                        assert!(
                            cx.update(|app| {
                                let completed = move |owner: &Rc<RefCell<RunningProcessOwner>>, request, result: Result<(), String>, app: &mut gpui::App| {
                                    assert_eq!(thread, std::thread::current().id());
                                    assert!(owner.try_borrow_mut().is_ok());
                                    assert!(!owner.borrow().test_services_on_worker());
                                    assert_eq!(
                                        owner.borrow().test_services().graph().unwrap().shutdown,
                                        original_attempt
                                    );
                                    assert_eq!(result.is_ok(), fault.is_none());
                                    assert!(owner.borrow().exit_requested());
                                    assert!(!RunningProcessOwner::finish_exit(owner, &request));
                                    assert!(
                                        RunningProcessOwner::release_shutdown_drafts(owner, app)
                                            .is_err()
                                    );
                                    assert!(
                                        RunningProcessOwner::release_shutdown_interaction_gate(
                                            owner, app
                                        )
                                        .is_err()
                                    );
                                    match (fault, owner.borrow().shutdown_session().unwrap()) {
                                        (
                                            None,
                                            RunningShutdownSession::Settled(Ok(
                                                ExitSessionExecution::Committed {
                                                    later_failure: None,
                                                    local_finalization: None,
                                                    ..
                                                },
                                            )),
                                        ) => {}
                                        (
                                            Some(FaultPoint::BeforeCommit),
                                            RunningShutdownSession::Settled(Ok(
                                                ExitSessionExecution::NotCommitted { .. },
                                            )),
                                        ) => {}
                                        (
                                            Some(FaultPoint::AfterPersist),
                                            RunningShutdownSession::Settled(Ok(
                                                ExitSessionExecution::Committed {
                                                    later_failure: Some(_),
                                                    local_finalization: Some(_),
                                                    ..
                                                },
                                            )),
                                        ) => {}
                                        (
                                            Some(FaultPoint::AfterCommitBeforePersist),
                                            RunningShutdownSession::Settled(Ok(
                                                ExitSessionExecution::Indeterminate(_),
                                            )),
                                        ) => {}
                                        other => panic!("unexpected publication: {other:?}"),
                                    }
                                    sender.send((owner.clone(), request)).ok().unwrap();
                                };
                                if consumer {
                                    RunningProcessOwner::test_complete_exit_work(&owner, request, app,
                                        move |owner, request, outcome, app| {
                                            assert!(!outcome.command_completed);
                                            let result = match outcome.result {
                                                Ok(crate::running_owner::ExitAttemptCompletion::SessionReady) => Ok(()),
                                                Err(crate::running_owner::ExitAttemptError::SessionPublication(error)) => {
                                                    let notice = window.read(app).unwrap().notice_projection().unwrap();
                                                    assert_eq!(notice.content.title().as_str(), "Couldn't exit Beryl");
                                                    assert_eq!(notice.content.commands().count(), 0);
                                                    assert_eq!(notice.report_count, 1);
                                                    assert_eq!(notice.content.detail().as_str(), format!("Exit session publication failed: {error}"));
                                                    Err(error)
                                                },
                                                other => panic!("unexpected attempt outcome: {other:?}"),
                                            };
                                            if result.is_ok() {
                                                assert!(window.read(app).unwrap().notice_projection().is_none());
                                            }
                                            completed(owner, request, result, app);
                                        });
                                    Ok(())
                                } else {
                                    RunningProcessOwner::publish_exit_session(&owner, request, app, completed)
                                }
                            })
                            .unwrap()
                            .is_ok()
                        );
                        drop(owner);
                        assert!(weak.upgrade().is_some());
                        let (owner, request) = receiver.await.unwrap();
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        let (request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::publish_exit_session(
                                    &owner,
                                    request,
                                    app,
                                    |_, _, _, _| panic!("duplicate publication"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::SessionPublication(_)));
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                        if consumer && fault.is_some() {
                            if matches!(fault, Some(FaultPoint::AfterCommitBeforePersist)) {
                                let (sender, receiver) = futures_channel::oneshot::channel();
                                cx.update(|app| RunningProcessOwner::reconcile_shutdown_session(
                                    &owner, app, move |_, _| { sender.send(()).unwrap(); }
                                )).unwrap().unwrap();
                                receiver.await.unwrap();
                                let borrowed = owner.borrow();
                                let graph = borrowed.test_services().graph().unwrap();
                                assert!(borrowed.shutdown_session().unwrap().require_ready(graph.home(), &graph.state().session()).is_ok());
                                assert!(borrowed.require_shutdown_session_ready().is_err());
                            }
                            let before = format!("{:?}", owner.borrow().shutdown_session().unwrap());
                            let foreign = request.test_foreign();
                            assert!(owner.borrow_mut().retain_interrupted_exit_session(&foreign).is_err());
                            assert!(owner.borrow().interrupted_exit_session().is_none());
                            assert_eq!(before, format!("{:?}", owner.borrow().shutdown_session().unwrap()));
                            cx.update(|app| {
                                assert!(owner.borrow_mut().retire_interrupted_exit_residents(&request, app).is_err());
                            }).unwrap();
                            owner.borrow_mut().retain_interrupted_exit_session(&request).unwrap();
                            let (directory, candidate) = cx.background_executor().spawn(async {
                                foreign_recovery_candidate()
                            }).await;
                            cx.update(|app| {
                                let generation = owner.borrow().test_services().graph().unwrap().home().health().generation().unwrap();
                                let stale = candidate.candidate.generation();
                                assert_ne!(generation, stale);
                                assert!(RunningProcessOwner::retire_interrupted_exit_graph(
                                    &owner, &request, stale, app,
                                    |_, _| panic!("stale generation retirement"),
                                ).unwrap_err().contains("exact published failed generation"));
                                if matches!(fault, Some(FaultPoint::AfterCommitBeforePersist)) {
                                    assert!(RunningProcessOwner::retire_interrupted_exit_graph(
                                        &owner, &request, generation, app,
                                        |_, _| panic!("healthy generation retirement"),
                                    ).unwrap_err().contains("exact published failed generation"));
                                }
                                assert!(!window.read(app).unwrap().test_shell_construction_retired());
                                assert!(!owner.borrow().test_services_on_worker());
                                assert!(owner.borrow().test_services().graph().is_some());
                                assert_eq!(before, format!("{:?}", owner.borrow().interrupted_exit_session().unwrap()));
                                assert!(owner.borrow_mut().retire_interrupted_exit_residents(&foreign, app).is_err());
                                window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(false, cx)).unwrap().unwrap();
                                assert!(owner.borrow_mut().retire_interrupted_exit_residents(&request, app).is_err());
                                let generation = owner.borrow().test_services().graph().unwrap().home().health().generation().unwrap();
                                assert!(RunningProcessOwner::retire_interrupted_exit_graph(
                                    &owner, &request, generation, app,
                                    |_, _| panic!("retirement with an ungated original window"),
                                ).unwrap_err().contains(if matches!(fault, Some(FaultPoint::AfterCommitBeforePersist)) {
                                    "exact published failed generation"
                                } else {
                                    "exact gated shell"
                                }));
                                assert!(!window.read(app).unwrap().test_shell_construction_retired());
                                assert!(!owner.borrow().test_services_on_worker());
                                window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(true, cx)).unwrap().unwrap();
                                if retirement_delivery.is_none() {
                                    assert!(owner.borrow_mut().retire_interrupted_exit_residents(&request, app).unwrap());
                                    assert!(owner.borrow_mut().retire_interrupted_exit_residents(&request, app).unwrap());
                                }
                            }).unwrap();
                            assert_eq!(before, format!("{:?}", owner.borrow().interrupted_exit_session().unwrap()));
                            assert!(matches!(owner.borrow().shutdown_session(), Some(RunningShutdownSession::RecoveryOwned)));
                            assert!(owner.borrow_mut().retain_interrupted_exit_session(&request).is_err());
                            assert!(owner.borrow().require_shutdown_session_ready().is_err());
                            assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                            let mut candidate = Some(candidate);
                            let retired_generation = owner.borrow().test_services().graph().unwrap().home().health().generation().unwrap();
                            let settled_candidate = if let Some(delivery) = retirement_delivery {
                                Some(retirement_driver::verify(&owner, &request, retired_generation, candidate.as_ref().unwrap().candidate.generation(), delivery, cx).await)
                            } else {
                                graph_retirement_support::verify(&owner, &request, fault.unwrap(), &mut candidate, cx).await;
                                None
                            };
                            let candidate = if retirement_delivery.is_some() {
                                candidate.unwrap()
                            } else {
                                candidate_settlement_support::verify(
                                    &owner, &request, candidate, settlement_unwind, cx,
                                ).await
                            };
                            if matches!(fault, Some(FaultPoint::BeforeCommit)) && !settlement_unwind {
                                recovered_appearance = Some(threadless_recovery::verify(&owner, &request, &candidate, retired_generation, &faults, publication_delivery, settled_candidate, cx).await);
                                recovered_threadless = true;
                            }
                            if matches!(publication_delivery, RecoveryPublicationDelivery::Complete | RecoveryPublicationDelivery::Driven) {
                                assert!(owner.borrow().interrupted_exit_session().is_none());
                                assert!(owner.borrow().shutdown_status().is_none());
                                recovered_threadless = false;
                            } else {
                                assert_eq!(before, format!("{:?}", owner.borrow().interrupted_exit_session().unwrap()));
                            }
                            assert!(owner.borrow().require_shutdown_session_ready().is_err());
                            assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                            cx.background_executor().spawn(async move {
                                candidate.candidate.abort().close().unwrap();
                                drop(directory);
                            }).await;
                        } else {
                            assert!(owner.borrow_mut().retain_interrupted_exit_session(&request).is_err());
                            assert!(owner.borrow().interrupted_exit_session().is_none());
                        }
                        let cancelled = ProjectionCancellationToken::new();
                        cancelled.cancel();
                        assert!(
                            cx.update(|app| RunningProcessOwner::advance_shutdown(
                                &owner,
                                cancelled,
                                app,
                                |_, _| panic!("publication fences recovery")
                            ))
                            .unwrap()
                            .is_err()
                        );
                        // Settle only the fixture's installed store operation before disposing its native windows.
                        if !consumer && matches!(fault, Some(FaultPoint::AfterCommitBeforePersist)) {
                            let home = owner
                                .borrow()
                                .test_services()
                                .graph()
                                .unwrap()
                                .home()
                                .service_reference();
                            let pending = home.pending_reconciliations();
                            assert_eq!(pending.len(), 1);
                            cx.background_executor()
                                .spawn(async move {
                                    assert!(matches!(
                                        home.retry_reconciliation(&pending[0]).unwrap(),
                                        beryl_home_store::ReconciliationResolution::ExactNew { .. }
                                    ));
                                })
                                .await;
                        }
                        let running = Rc::try_unwrap(owner)
                            .ok()
                            .unwrap()
                            .into_inner()
                            .test_into_process();
                        if consumer && fault.is_some() {
                            dispose_failed_fixture(running, true, recovered_threadless, recovered_appearance, cx).await;
                        } else if matches!(
                            fault,
                            Some(FaultPoint::BeforeCommit | FaultPoint::AfterPersist)
                        ) {
                            dispose_failed_fixture(running, false, false, None, cx).await;
                        } else {
                            support::dispose_running(running, cx).await;
                        }
                        assert!(weak.upgrade().is_none());
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
    directory.close().unwrap();
}

async fn dispose_failed_fixture(
    mut running: startup_owner::StartedProcess,
    retired: bool,
    unstarted_recovery: bool,
    recovered_appearance: Option<gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>>,
    cx: &mut AsyncApp,
) {
    let fresh_owner = if let Some(owner) = recovered_appearance {
        Some(
            cx.update(|app| {
                let shell = &running.windows.shells()[0];
                shell
                    .window()
                    .update(app, |root, window, cx| {
                        root.set_shutdown_interaction_gated(true, cx).unwrap();
                        let mut draft = root.begin_shutdown_draft(window, cx).unwrap();
                        assert!(root.retire_shutdown_draft(&mut draft, cx).unwrap());
                    })
                    .unwrap();
                owner
            })
            .unwrap(),
        )
    } else {
        None
    };
    if retired {
        cx.update(|app| {
            for shell in running.windows.shells() {
                shell
                    .window()
                    .update(app, |root, window, _| {
                        assert!(root.test_shell_construction_retired());
                        assert!(root.controller().unwrap().is_threadless());
                        assert!(root.controller().unwrap().composer_mount().is_none());
                        window.remove_window();
                    })
                    .unwrap();
            }
            drop(running.windows);
        })
        .unwrap();
        cx.update(|app| assert!(app.windows().is_empty())).unwrap();
        if let Some(owner) = fresh_owner {
            use crate::theme_runtime::AppearancePublicationTarget;
            cx.update(|app| {
                assert_eq!(owner.read(app).target().snapshot().count, 0);
            })
            .unwrap();
        }
    } else {
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            running.windows.test_dispose(
                move |result, _| {
                    assert!(result.retained.is_none());
                    sender.send(()).unwrap();
                },
                app,
            )
        })
        .unwrap();
        receiver.await.unwrap();
    }
    running
        .appearance
        .update(cx, |appearance, _| appearance.retire())
        .unwrap();
    cx.background_executor()
        .spawn(async move {
            if running.services.graph().is_none() {
                let home = running.services.test_retired_service_home();
                assert!(!unstarted_recovery);
                home.unwrap().close().unwrap();
                return;
            }
            if running.services.graph().unwrap().home().health().state() == HomeHealthState::Healthy
            {
                if unstarted_recovery {
                    running
                        .services
                        .graph
                        .take()
                        .unwrap()
                        .dispose_unstarted()
                        .unwrap();
                } else {
                    close(&mut running.services);
                }
                return;
            }
            assert_eq!(
                running.services.graph().unwrap().home().health().state(),
                HomeHealthState::Failed
            );
            running.services.retire_failed_startup().unwrap();
            assert!(running.services.graph().is_none());
            assert!(running.services.retained_close().is_none());
        })
        .await;
}
