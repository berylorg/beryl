use crate::main_window::{
    NOTICE_GENERAL_CAPACITY, NoticeConditionId, NoticeContent, NoticeDismissal, NoticeKind,
    NoticeRecord, NoticeVariant,
};
use crate::running_owner::{ExitAttemptCompletion, ExitAttemptError, ExitRoutingError};

#[test]
fn native_exit_consumer_handles_deferred_command_and_fresh_successor() {
    run_consumer(true, false);
}

#[test]
fn native_exit_consumer_handles_fresh_command_and_reentrant_wait() {
    run_consumer(false, false);
}

#[test]
fn native_exit_consumer_reports_scheduling_refusal_and_retains_custody() {
    run_consumer(false, true);
}

#[test]
fn native_exit_consumer_preserves_outcome_when_notice_capacity_is_full() {
    run_consumer_with_notices(false, false, true, false);
}

#[test]
fn native_exit_consumer_does_not_redirect_an_unavailable_invoking_window() {
    run_consumer_with_notices(false, false, false, true);
}

fn run_consumer(deferred: bool, refusal: bool) {
    run_consumer_with_notices(deferred, refusal, false, false);
}

fn run_consumer_with_notices(deferred: bool, refusal: bool, full: bool, missing: bool) {
    let directory = support::native_home();
    let input = input(directory.path(), |path, _| support::open(path));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            let producer = Rc::new(RefCell::new(None::<startup_owner::StartupCommands>));
            let supplied = producer.clone();
            let commands = startup_owner::start(
                input,
                move |result, app| {
                    let StartupCompletion::Running(running) = result else {
                        panic!("startup failed")
                    };
                    let invoking = running.windows.window_ids()[0];
                    let window = running.windows.shells()[0].window();
                    if full {
                        let ingress = window.update(app, |root, window, cx|
                            root.notice_ingress(window, cx)).unwrap();
                        for _ in 0..NOTICE_GENERAL_CAPACITY {
                            let _ = ingress.admit(NoticeRecord {
                                window_id: invoking,
                                condition: NoticeConditionId::new(),
                                revision: 1,
                                kind: NoticeKind::Error,
                                content: NoticeContent::new(NoticeVariant::Error,
                                    NoticeDismissal::Dismissible, "Existing error", "Existing detail"),
                            }, app);
                        }
                    }
                    let owner = RunningProcessOwner::test_start_unmounted(running, app);
                    let missing_command = missing.then(|| owner.borrow().test_window_command_unchecked(
                        beryl_model::WindowId::from_bytes([249; 16]),
                    ));
                    if let Some(command) = &missing_command {
                        assert!(command.disabled_reason().is_none());
                    }
                    let command = owner.borrow().window_exit_command(invoking, app).unwrap();
                    app.spawn(async move |cx| {
                        if refusal {
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
                        }
                        let first = Rc::new(RefCell::new(None));
                        let delivered = first.clone();
                        let second = Rc::new(RefCell::new(None));
                        let successor = second.clone();
                        let gui_thread = std::thread::current().id();
                        let weak = Rc::downgrade(&owner);
                        let cancellation = ProjectionCancellationToken::new();
                        cancellation.cancel();
                        cx.update(|app| {
                            if deferred {
                                producer.borrow().as_ref().unwrap().request_exit();
                            }
                            RunningProcessOwner::wait_for_exit_session(
                                &owner,
                                cancellation,
                                app,
                                move |owner, request, outcome, app| {
                                    assert_eq!(std::thread::current().id(), gui_thread);
                                    assert!(!owner.borrow().test_services_on_worker());
                                    assert_eq!(outcome.command_completed, !refusal);
                                    assert_eq!(owner.borrow().exit_requested(), refusal);
                                    let root = window.read(app).unwrap();
                                    assert_eq!(root.notice_diagnostics().retained_records,
                                        if full { NOTICE_GENERAL_CAPACITY } else { usize::from(!missing) });
                                    if missing {
                                        assert!(root.notice_projection().is_none());
                                    } else {
                                        let notice = root.notice_projection().unwrap();
                                        assert_eq!(notice.kind, NoticeKind::Error);
                                        assert_eq!(notice.report_count, 1);
                                        assert_eq!(notice.content.commands().count(), 0);
                                        assert_eq!(notice.content.dismissal, NoticeDismissal::Dismissible);
                                        assert_eq!(notice.content.title().as_str(),
                                            if full { "Existing error" } else { "Couldn't exit Beryl" });
                                        assert_eq!(root.notice_diagnostics().omitted, u64::from(full));
                                    }
                                    if refusal {
                                        assert!(matches!(
                                            outcome.result,
                                            Err(ExitAttemptError::Observation(
                                                ExitObservationError::Scheduling(_)
                                            ))
                                        ));
                                        assert!(matches!(
                                            owner.borrow().shutdown_status(),
                                            Some((window, ShutdownIntent::ApplicationExit,
                                                RunningShutdownStatus::Admitted)) if window == invoking
                                        ));
                                    } else {
                                        assert!(if missing { matches!(outcome.result,
                                            Err(ExitAttemptError::Observation(ExitObservationError::Request(_))))
                                        } else { matches!(
                                            outcome.result,
                                            Err(ExitAttemptError::Routing(ExitRoutingError::Work(
                                                ExitWorkError::Observation(_)
                                            )))
                                        ) });
                                        assert!(owner.borrow().shutdown_status().is_none());
                                        let previous = request.identity();
                                        RunningProcessOwner::wait_for_exit_session(
                                            owner,
                                            ProjectionCancellationToken::new(),
                                            app,
                                            move |owner, mut request, outcome, app| {
                                                assert_eq!(std::thread::current().id(), gui_thread);
                                                assert!(!Rc::ptr_eq(&previous, &request.identity()));
                                                assert!(!outcome.command_completed);
                                                let root = window.read(app).unwrap();
                                                assert_eq!(root.notice_diagnostics().retained_records,
                                                    if full { NOTICE_GENERAL_CAPACITY } else { usize::from(!missing) });
                                                assert_eq!(root.notice_diagnostics().omitted, u64::from(full));
                                                assert_eq!(root.notice_projection().map(|notice| notice.report_count),
                                                    if missing { None } else { Some(1) });
                                                assert!(matches!(outcome.result,
                                                    Ok(ExitAttemptCompletion::SessionReady)));
                                                assert_eq!(owner.borrow().resolve_exit_window(
                                                    &mut request, app).unwrap(), invoking);
                                                assert!(!owner.borrow().test_services_on_worker());
                                                assert!(owner.borrow_mut().take_shutdown_progress().is_none());
                                                assert!(successor.borrow_mut().replace(request).is_none());
                                            },
                                        ).unwrap();
                                    }
                                    assert!(delivered.borrow_mut().replace((owner.clone(), request)).is_none());
                                },
                            ).unwrap();
                            assert!(RunningProcessOwner::wait_for_exit_session(
                                &owner, ProjectionCancellationToken::new(), app,
                                |_, _, _, _| panic!("duplicate wait must not notify"),
                            ).is_err());
                            if !deferred {
                                missing_command.as_ref().unwrap_or(&command).request_exit();
                            }
                            command.request_exit();
                            command.request_exit();
                        }).unwrap();
                        drop(owner);
                        assert!(weak.upgrade().is_some());
                        wait(&first, cx).await;
                        let (owner, mut request) = first.borrow_mut().take().unwrap();
                        if !refusal {
                            cx.background_executor().timer(Duration::from_millis(75)).await;
                            assert!(second.borrow().is_none(), "duplicates queued a successor");
                            assert!(!owner.borrow().exit_requested());
                            command.request_exit();
                            wait(&second, cx).await;
                            request = second.borrow_mut().take().unwrap();
                        }
                        assert!(owner.borrow().exit_requested());
                        assert!(!RunningProcessOwner::finish_exit(&owner, &request));
                        if !refusal {
                            assert!(owner.borrow().require_shutdown_session_ready().is_ok());
                            assert!(cx.update(|app| RunningProcessOwner::release_shutdown_drafts(&owner, app)).unwrap().is_err());
                            let running = Rc::try_unwrap(owner).ok().unwrap().into_inner().test_into_process();
                            support::dispose_running(running, cx).await;
                            observed.set(true);
                            cx.update(|app| app.quit()).unwrap();
                            return;
                        }
                        let identity = request.identity();
                        let cancellation = ProjectionCancellationToken::new();
                        cancellation.cancel();
                        let cleaned = Rc::new(RefCell::new(None));
                        let delivered = cleaned.clone();
                        cx.update(|app| {
                            let completed = move |owner: &Rc<RefCell<RunningProcessOwner>>, request, result: Result<AppServiceShutdownProgress, crate::running_owner::ExitProgressError>, _: &mut gpui::App| {
                                    assert!(matches!(result.unwrap(),
                                        AppServiceShutdownProgress::Failed { reopened: true, .. }));
                                    assert!(owner.borrow().shutdown_status().is_none());
                                    assert!(RunningProcessOwner::finish_exit(owner, &request));
                                    *delivered.borrow_mut() = Some(request);
                                };
                            let scheduled = if refusal {
                                RunningProcessOwner::drive_exit(&owner, request, cancellation, app, completed)
                            } else {
                                RunningProcessOwner::recover_exit_drafts(&owner, request, app, completed)
                            };
                            assert!(scheduled.is_ok());
                        }).unwrap();
                        wait(&cleaned, cx).await;
                        assert!(Rc::ptr_eq(&identity, &cleaned.borrow_mut().take().unwrap().identity()));
                        assert!(!owner.borrow().exit_requested());
                        let running = Rc::try_unwrap(owner).ok().unwrap().into_inner().test_into_process();
                        support::dispose_running(running, cx).await;
                        observed.set(true);
                        cx.update(|app| app.quit()).unwrap();
                    }).detach();
                },
                app,
            );
            *supplied.borrow_mut() = Some(commands);
            support::watchdog(app);
        });
    assert!(finished.get());
    assert_reopens(&directory);
}
