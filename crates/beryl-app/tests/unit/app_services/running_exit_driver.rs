use crate::running_owner::ExitProgressError;
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_state::{
    ApplySettings, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};

#[test]
fn native_exit_driver_returns_ready_with_original_custody() {
    run_driver(false, false, false);
}

#[test]
fn native_exit_driver_returns_cancelled_with_proven_reopening() {
    run_driver(true, false, false);
}

#[test]
fn native_exit_driver_waits_for_home_mutation_then_resumes() {
    run_driver(false, true, false);
}

#[test]
fn native_exit_progress_delivery_notice_preserves_unconsumed_progress_and_admission() {
    run_driver(false, false, true);
}

fn run_driver(cancelled: bool, waiting: bool, report_refusal: bool) {
    run_driver_with_work_failure(cancelled, waiting, report_refusal, None);
}

#[test]
fn native_exit_unattributed_source_failure_preserves_admission() {
    run_driver_with_work_failure(
        false,
        false,
        false,
        Some(crate::cas_projection::ShutdownFailure::SourceUnavailable),
    );
}

#[test]
fn native_exit_unattributed_stop_failure_preserves_admission() {
    run_driver_with_work_failure(
        false,
        false,
        false,
        Some(crate::cas_projection::ShutdownFailure::StopFailed),
    );
}

#[test]
fn native_exit_unattributed_cleanup_failure_preserves_admission() {
    run_driver_with_work_failure(
        false,
        false,
        false,
        Some(crate::cas_projection::ShutdownFailure::CleanupFailed),
    );
}

#[test]
fn native_exit_unviewed_execution_failure_preserves_admission() {
    run_driver_with_work_failure(
        false,
        false,
        false,
        Some(crate::cas_projection::ShutdownFailure::UnprovenExecution {
            thread: beryl_model::SyndicThreadId::from_bytes([211; 16]),
            turn: beryl_model::SyndicTurnId::from_bytes([212; 16]),
        }),
    );
}

#[test]
fn native_exit_unviewed_compaction_failure_preserves_admission() {
    run_driver_with_work_failure(
        false,
        false,
        false,
        Some(crate::cas_projection::ShutdownFailure::UnprovenCompaction {
            operation: syndic_storage::CompactionOperationId::new(
                beryl_model::SyndicThreadId::from_bytes([213; 16]),
                syndic_storage::CompactionOperationNonce::from_bytes([214; 16]),
            ),
        }),
    );
}

fn run_driver_with_work_failure(
    cancelled: bool,
    waiting: bool,
    report_refusal: bool,
    work_failure: Option<crate::cas_projection::ShutdownFailure>,
) {
    let directory = support::native_home();
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
                    let command = owner.borrow().window_exit_command(invoking, app).unwrap();
                    command.request_exit();
                    app.spawn(async move |cx| {
                        let request = next_request(&owner, cx).await;
                        let identity = request.identity();
                        let (mut request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::drive_exit(
                                    &owner,
                                    request,
                                    ProjectionCancellationToken::new(),
                                    app,
                                    |_, _, _, _| panic!("refusal must not notify"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::Intent));
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        assert!(!owner.borrow().test_services_on_worker());
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
                        if let Some(reason) = work_failure {
                            cx.update(|app| {
                                RunningProcessOwner::test_report_exit_work_failure(&owner, &request, reason, false, app);
                                let root = window.read(app).unwrap();
                                assert!(root.controller().unwrap().is_threadless());
                                let notice = root.notice_projection().unwrap();
                                assert_eq!(notice.content.title().as_str(), "Couldn't exit Beryl");
                                assert!(notice.content.detail().as_str().contains(&format!("{reason:?}")));
                                let expected_step = match reason {
                                    crate::cas_projection::ShutdownFailure::SourceUnavailable => "Shutdown work observation is unavailable",
                                    crate::cas_projection::ShutdownFailure::StopFailed => "Shutdown could not stop work",
                                    crate::cas_projection::ShutdownFailure::CleanupFailed => "Shutdown could not complete work cleanup",
                                    _ => "Shutdown could not prove that work settled",
                                };
                                assert!(notice.content.detail().as_str().starts_with(expected_step));
                                assert_eq!(notice.content.commands().count(), 0);
                                assert_eq!(root.notice_diagnostics().retained_records, 1);
                                assert!(Rc::ptr_eq(&identity, &request.identity()));
                                assert!(owner.borrow().exit_requested());
                                assert!(matches!(owner.borrow().shutdown_status(), Some((window, ShutdownIntent::ApplicationExit, RunningShutdownStatus::Admitted)) if window == invoking));
                                assert!(!owner.borrow().test_services_on_worker());
                                assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                            }).unwrap();
                        }
                        if report_refusal {
                            let settled = Rc::new(Cell::new(false));
                            let signalled = settled.clone();
                            cx.update(|app| {
                                RunningProcessOwner::advance_shutdown(
                                    &owner,
                                    ProjectionCancellationToken::new(),
                                    app,
                                    move |_, _| signalled.set(true),
                                )
                            })
                            .unwrap()
                            .unwrap();
                            let deadline = Instant::now() + Duration::from_secs(5);
                            while !settled.get() {
                                assert!(Instant::now() < deadline);
                                cx.background_executor()
                                    .timer(Duration::from_millis(1))
                                    .await;
                            }
                            let (returned, error) = cx
                                .update(|app| {
                                    RunningProcessOwner::drive_exit(
                                        &owner,
                                        request,
                                        ProjectionCancellationToken::new(),
                                        app,
                                        |_, _, _, _| panic!("refused progress must not notify"),
                                    )
                                })
                                .unwrap()
                                .err()
                                .unwrap();
                            request = returned;
                            assert!(matches!(&error, ExitProgressError::Scheduling(_)));
                            let command_completed =
                                RunningProcessOwner::finish_exit(&owner, &request);
                            assert!(!command_completed);
                            cx.update(|app| {
                                RunningProcessOwner::test_report_exit_delivery_failure(
                                    &owner,
                                    &request,
                                    crate::running_owner::ExitAttemptError::Progress(error),
                                    command_completed,
                                    app,
                                );
                                let root = window.read(app).unwrap();
                                let notice = root.notice_projection().unwrap();
                                assert_eq!(notice.content.title().as_str(), "Couldn't exit Beryl");
                                assert!(
                                    notice
                                        .content
                                        .detail()
                                        .as_str()
                                        .contains("unconsumed result")
                                );
                                assert_eq!(notice.content.commands().count(), 0);
                                assert_eq!(root.notice_diagnostics().retained_records, 1);
                            })
                            .unwrap();
                            assert!(Rc::ptr_eq(&identity, &request.identity()));
                            assert!(owner.borrow().exit_requested());
                            assert!(owner.borrow().test_shutdown_progress_settled());
                            assert!(!owner.borrow().test_services_on_worker());
                            assert!(matches!(owner.borrow().shutdown_status(),
                                Some((window, ShutdownIntent::ApplicationExit,
                                    RunningShutdownStatus::WorkReady)) if window == invoking));
                            assert!(matches!(
                                owner.borrow_mut().take_shutdown_progress(),
                                Some(Ok(AppServiceShutdownProgress::Ready))
                            ));
                        }
                        let mutation = if waiting {
                            let (home, command) = {
                                let owner = owner.borrow();
                                let graph = owner.test_services().graph().unwrap();
                                let home = graph.home();
                                let mut command = HomeCommand::new(home.home_revision().unwrap());
                                command
                                    .add(
                                        graph.state().settings().apply(
                                            graph.state().settings().revision(home).unwrap(),
                                            ApplySettings::new(vec![SettingUpdate::new(
                                                SettingKey::DeveloperInstructions,
                                                ExpectedSettingRevision::Absent,
                                                SettingValue::developer_instructions(
                                                    "waiting progress fixture",
                                                )
                                                .unwrap(),
                                            )])
                                            .unwrap(),
                                        ),
                                    )
                                    .unwrap();
                                (home.service_reference(), command)
                            };
                            let pause = faults.block_next(FaultPoint::BeforeCommit);
                            let work = cx
                                .background_executor()
                                .spawn(async move { home.execute(command) });
                            assert!(pause.wait_until_reached(Duration::from_secs(5)));
                            Some((pause, work))
                        } else {
                            None
                        };
                        let cancellation = ProjectionCancellationToken::new();
                        if cancelled || report_refusal {
                            cancellation.cancel();
                        }
                        let slot = Rc::new(RefCell::new(None));
                        let delivered = slot.clone();
                        let gui_thread = std::thread::current().id();
                        let weak = Rc::downgrade(&owner);
                        cx.update(|app| {
                            assert!(
                                RunningProcessOwner::drive_exit(
                                    &owner,
                                    request,
                                    cancellation,
                                    app,
                                    move |owner, request, result, _| {
                                        assert_eq!(std::thread::current().id(), gui_thread);
                                        assert!(!owner.borrow().test_services_on_worker());
                                        assert!(
                                            owner.borrow_mut().take_shutdown_progress().is_none()
                                        );
                                        assert!(owner.borrow().exit_requested());
                                        assert_eq!(
                                            owner.borrow().test_process().windows.window_ids(),
                                            vec![invoking]
                                        );
                                        assert!(
                                            delivered
                                                .borrow_mut()
                                                .replace((owner.clone(), request, result))
                                                .is_none()
                                        );
                                    },
                                )
                                .is_ok()
                            );
                            command.request_exit();
                        })
                        .unwrap();
                        drop(owner);
                        assert!(weak.upgrade().is_some());
                        if let Some((pause, work)) = mutation {
                            let deadline = Instant::now() + Duration::from_secs(5);
                            while {
                                let owner = weak.upgrade().unwrap();
                                let owner = owner.borrow();
                                owner.test_exit_waiting_passes() == 0
                                    || owner.test_services_on_worker()
                            } {
                                assert!(
                                    Instant::now() < deadline,
                                    "driver never delivered Waiting"
                                );
                                cx.background_executor()
                                    .timer(Duration::from_millis(1))
                                    .await;
                            }
                            assert!(slot.borrow().is_none());
                            {
                                let owner = weak.upgrade().unwrap();
                                assert!(!owner.borrow().test_services_on_worker());
                                assert!(owner.borrow_mut().take_shutdown_progress().is_none());
                                assert!(owner.borrow().exit_requested());
                            }
                            command.request_exit();
                            pause.release();
                            assert!(matches!(work.await, CommandOutcome::Committed { .. }));
                        }
                        wait(&slot, cx).await;
                        let (owner, mut request, result) = slot.borrow_mut().take().unwrap();
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                        if cancelled || report_refusal {
                            assert!(matches!(
                                result.unwrap(),
                                AppServiceShutdownProgress::Failed { reopened: true, .. }
                            ));
                        } else {
                            assert!(matches!(result.unwrap(), AppServiceShutdownProgress::Ready));
                            assert_eq!(owner.borrow().shutdown_status(), Some((invoking, ShutdownIntent::ApplicationExit, RunningShutdownStatus::WorkReady)));
                            assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                            let cancellation = ProjectionCancellationToken::new();
                            cancellation.cancel();
                            let delivered = slot.clone();
                            cx.update(|app| {
                                assert!(
                                    RunningProcessOwner::drive_exit(
                                        &owner,
                                        request,
                                        cancellation,
                                        app,
                                        move |owner, request, result, _| {
                                            assert!(
                                                delivered
                                                    .borrow_mut()
                                                    .replace((owner.clone(), request, result))
                                                    .is_none()
                                            );
                                        },
                                    )
                                    .is_ok()
                                )
                            })
                            .unwrap();
                            wait(&slot, cx).await;
                            let (_, returned, result) = slot.borrow_mut().take().unwrap();
                            request = returned;
                            assert!(matches!(
                                result.unwrap(),
                                AppServiceShutdownProgress::Failed { reopened: true, .. }
                            ));
                        }
                        assert!(owner.borrow().shutdown_status().is_none());
                        assert!(RunningProcessOwner::finish_exit(&owner, &request));
                        assert!(!owner.borrow().exit_requested());
                        let (request, error) = cx
                            .update(|app| {
                                RunningProcessOwner::drive_exit(
                                    &owner,
                                    request,
                                    ProjectionCancellationToken::new(),
                                    app,
                                    |_, _, _, _| panic!("stale request must not notify"),
                                )
                            })
                            .unwrap()
                            .err()
                            .unwrap();
                        assert!(matches!(error, ExitProgressError::Request(_)));
                        assert!(Rc::ptr_eq(&identity, &request.identity()));
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
