#[test]
fn native_pending_exit_delivers_once_and_reentrant_successor_requires_fresh_activation() {
    run_delivery(true);
}

#[test]
fn native_delayed_exit_retains_owner_without_blocking_gui_or_execution() {
    run_delivery(false);
}

fn run_delivery(pending: bool) {
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
                    let producer = producer.borrow().as_ref().unwrap().clone();
                    let invoking = running.windows.window_ids()[0];
                    let main = running.windows.shells()[0].window();
                    let permit = running.services.process.execution_permit();
                    let owner = RunningProcessOwner::start(running, app);
                    if pending {
                        producer.request_exit();
                    }
                    let completion = Rc::new(RefCell::new(None));
                    let completed = completion.clone();
                    let callbacks = Rc::new(Cell::new(0));
                    let calls = callbacks.clone();
                    let callback_producer = producer.clone();
                    let gui_thread = std::thread::current().id();
                    RunningProcessOwner::wait_for_exit(&owner, app, move |owner, request, app| {
                        assert_eq!(std::thread::current().id(), gui_thread);
                        assert_eq!(calls.replace(1), 0);
                        assert!(owner.borrow().exit_requested());
                        assert!(owner.borrow().shutdown_status().is_none());
                        callback_producer.request_exit();
                        callback_producer.request_exit();
                        let request = Rc::new(request);
                        let stale = request.clone();
                        RunningProcessOwner::wait_for_exit(
                            owner,
                            app,
                            move |owner, successor, _| {
                                assert_eq!(std::thread::current().id(), gui_thread);
                                assert_eq!(calls.replace(2), 1);
                                assert!(!RunningProcessOwner::finish_exit(owner, &stale));
                                assert!(owner.borrow().exit_requested());
                                assert!(RunningProcessOwner::finish_exit(owner, &successor));
                                assert!(!owner.borrow().exit_requested());
                                *completed.borrow_mut() = Some(owner.clone());
                            },
                        )
                        .unwrap();
                        assert!(RunningProcessOwner::finish_exit(owner, &request));
                        assert!(!RunningProcessOwner::finish_exit(owner, &request));
                        assert!(!owner.borrow().exit_requested());
                    })
                    .unwrap();
                    assert!(
                        RunningProcessOwner::wait_for_exit(&owner, app, |_, _, _| panic!(
                            "overlapping wait must not notify"
                        ))
                        .is_err()
                    );
                    let weak = Rc::downgrade(&owner);
                    drop(owner);
                    app.spawn(async move |cx| {
                        let deadline = Instant::now() + Duration::from_secs(5);
                        if !pending {
                            cx.background_executor()
                                .timer(Duration::from_millis(40))
                                .await;
                            assert_eq!(callbacks.get(), 0);
                            {
                                let owner =
                                    weak.upgrade().expect("pending wait retains complete owner");
                                assert!(!owner.borrow().exit_requested());
                                assert!(owner.borrow().test_services().graph().is_some());
                            }
                            permit.commit(|| ()).unwrap();
                            main.update(cx, |root, _, _| {
                                assert_eq!(root.controller().unwrap().window_id(), invoking)
                            })
                            .unwrap();
                            producer.request_exit();
                        }
                        while callbacks.get() == 0 {
                            assert!(Instant::now() < deadline);
                            cx.background_executor()
                                .timer(Duration::from_millis(10))
                                .await;
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(40))
                            .await;
                        assert_eq!(callbacks.get(), 1);
                        permit.commit(|| ()).unwrap();
                        producer.request_exit();
                        while completion.borrow().is_none() {
                            assert!(Instant::now() < deadline);
                            cx.background_executor()
                                .timer(Duration::from_millis(10))
                                .await;
                        }
                        assert_eq!(callbacks.get(), 2);
                        let owner = completion.borrow_mut().take().unwrap();
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
            *supplied.borrow_mut() = Some(commands);
            support::watchdog(app);
        });
    assert!(finished.get());
    assert_reopens(&directory);
}
