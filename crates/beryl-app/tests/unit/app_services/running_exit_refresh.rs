#[test]
fn native_initial_exit_refresh_requires_confirmation_for_arriving_work() {
    exercise(true, false);
}

#[test]
fn native_initial_exit_refresh_admits_fresh_idle_evidence() {
    exercise(false, false);
}

#[test]
fn native_initial_exit_refresh_cancellation_returns_original_request() {
    exercise(true, true);
}

fn exercise(keep_work: bool, cancel: bool) {
    let directory = support::native_home();
    let input = input(directory.path(), |path, _| support::open(path));
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
                    let permit = running.services.process.execution_permit();
                    let owner = RunningProcessOwner::test_start_unmounted(running, app);
                    let command = owner.borrow().window_exit_command(invoking, app).unwrap();
                    command.request_exit();
                    app.spawn(async move |cx| {
                        let request = next_request(&owner, cx).await;
                        let identity = request.identity();
                        let token = ProjectionCancellationToken::new();
                        let slot = Rc::new(RefCell::new(None));
                        let delivered = slot.clone();
                        let gui_thread = std::thread::current().id();
                        let (collected, first_ready) = std::sync::mpsc::sync_channel(1);
                        let (release_first, hold_first) = std::sync::mpsc::sync_channel(1);
                        let (refreshing, refresh_ready) = std::sync::mpsc::sync_channel(1);
                        let (release_refresh, hold_refresh) = std::sync::mpsc::sync_channel(1);
                        assert!(
                            cx.update(|app| {
                                RunningProcessOwner::test_observe_and_classify_exit_work_with(
                                    &owner,
                                    request,
                                    token.clone(),
                                    app,
                                    move |owner, mut request, result, app| {
                                        assert_eq!(std::thread::current().id(), gui_thread);
                                        assert!(Rc::ptr_eq(&identity, &request.identity()));
                                        assert_eq!(
                                            owner
                                                .borrow()
                                                .resolve_exit_window(&mut request, app)
                                                .unwrap(),
                                            invoking
                                        );
                                        assert!(delivered.borrow().is_none());
                                        *delivered.borrow_mut() =
                                            Some((owner.clone(), request, result));
                                    },
                                    move |result| {
                                        assert!(!result.as_ref().unwrap().has_work());
                                        collected.send(()).unwrap();
                                        hold_first.recv_timeout(Duration::from_secs(10)).unwrap();
                                    },
                                    move || {
                                        refreshing.send(()).unwrap();
                                        hold_refresh.recv_timeout(Duration::from_secs(10)).unwrap();
                                    },
                                )
                            })
                            .unwrap()
                            .is_ok()
                        );
                        wait_signal(&first_ready, cx).await;
                        let mut work =
                            Some(crate::cas_projection::test_faults::retain_projection_work(
                                owner.borrow().test_services().graph().unwrap().cas(),
                                beryl_model::SyndicThreadId::from_bytes([233; 16]),
                            ));
                        if !keep_work {
                            drop(work.take());
                        }
                        let weak = Rc::downgrade(&owner);
                        drop(owner);
                        release_first.send(()).unwrap();
                        wait_signal(&refresh_ready, cx).await;
                        {
                            let owner = weak.upgrade().expect("refresh retains complete owner");
                            assert!(slot.borrow().is_none());
                            assert!(owner.borrow().shutdown_status().is_none());
                            assert!(owner.borrow().exit_requested());
                            command.request_exit();
                            permit.commit(|| ()).unwrap();
                            assert!(
                                cx.update(|app| RunningProcessOwner::observe_shutdown_work(
                                    &owner,
                                    ProjectionCancellationToken::new(),
                                    app,
                                    |_, _, _| panic!("duplicate observation must not complete"),
                                ))
                                .unwrap()
                                .is_err()
                            );
                        }
                        if cancel {
                            token.cancel();
                        }
                        release_refresh.send(()).unwrap();
                        wait(&slot, cx).await;
                        let (owner, request, result) = slot.borrow_mut().take().unwrap();
                        if cancel {
                            assert!(matches!(
                                result,
                                Err(ExitWorkError::Observation(AppServiceCloseError::Work(
                                    crate::cas_projection::ShutdownWorkError::Work(
                                        crate::cas_projection::ProcessWorkError::Cancelled
                                    )
                                )))
                            ));
                        } else if keep_work {
                            let ExitWorkClassification::ConfirmationRequired {
                                invoking: original,
                                observation,
                            } = result.unwrap()
                            else {
                                panic!("newly arrived work requires confirmation")
                            };
                            assert_eq!(original, invoking);
                            assert_eq!(observation.running_threads(), 1);
                        } else {
                            assert!(matches!(result, Ok(ExitWorkClassification::Admitted)));
                            assert!(permit.commit(|| ()).is_err());
                        }
                        drop(work);
                        let owner = if !keep_work && !cancel {
                            super::super::running_shutdown_progress::exercise(
                                owner,
                                invoking,
                                ShutdownIntent::ApplicationExit,
                                false,
                                cx,
                            )
                            .await
                        } else {
                            assert!(owner.borrow().shutdown_status().is_none());
                            permit.commit(|| ()).unwrap();
                            owner
                        };
                        assert!(RunningProcessOwner::finish_exit(&owner, &request));
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

async fn wait_signal(signal: &std::sync::mpsc::Receiver<()>, cx: &mut AsyncApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match signal.try_recv() {
            Ok(()) => return,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(error) => panic!("worker signal failed: {error}"),
        }
        assert!(Instant::now() < deadline, "worker did not reach checkpoint");
        cx.background_executor()
            .timer(Duration::from_millis(1))
            .await;
    }
}
