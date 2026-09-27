use crate::running_owner::{IdleShutdownError, RunningProcessOwner, ShutdownIntent};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Idle,
    Work,
    CancelBefore,
    CancelAfter,
}

#[test]
fn native_initial_idle_observation_retains_owner_and_allows_reentrant_successor() {
    run(Outcome::Idle);
}

#[test]
fn native_initial_observation_reports_process_work_without_fencing() {
    run(Outcome::Work);
}

#[test]
fn native_initial_observation_cancellation_before_collection_is_not_idle() {
    run(Outcome::CancelBefore);
}

#[test]
fn native_initial_observation_cancellation_after_collection_is_not_idle() {
    run(Outcome::CancelAfter);
}

fn run(outcome: Outcome) {
    let directory = support::native_home();
    let input = input(directory.path(), |path, _| support::open(path));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new().with_quit_on_last_window_close(false).run(move |app| {
        startup_owner::start(input, move |result, app| {
            let StartupCompletion::Running(running) = result else { panic!("startup failed") };
            let invoking = running.windows.window_ids()[0];
            let main = running.windows.shells()[0].window();
            let permit = running.services.process.execution_permit();
            let owner = RunningProcessOwner::start(running, app);
            app.spawn(async move |cx| {
                let job = owner.borrow().test_services().prepare_shutdown_observation().unwrap();
                let idle = cx.background_executor().spawn(async move {
                    job.collect(&ProjectionCancellationToken::new()).unwrap()
                }).await;
                let work = (outcome == Outcome::Work).then(|| {
                    crate::cas_projection::test_faults::retain_projection_work(
                        owner.borrow().test_services().graph().unwrap().cas(),
                        beryl_model::SyndicThreadId::from_bytes([236; 16]),
                    )
                });
                let cancellation = ProjectionCancellationToken::new();
                let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let before_entered = entered.clone();
                let after_entered = entered.clone();
                let (before_release, before_wait) = std::sync::mpsc::sync_channel(1);
                let (after_release, after_wait) = std::sync::mpsc::sync_channel(1);
                let completion = Rc::new(RefCell::new(None));
                let completed = completion.clone();
                let callbacks = Rc::new(Cell::new(0));
                let calls = callbacks.clone();
                let gui_thread = std::thread::current().id();
                cx.update(|app| RunningProcessOwner::test_observe_shutdown_work_with(
                    &owner, cancellation.clone(), app,
                    move |owner, result, app| {
                        assert_eq!(std::thread::current().id(), gui_thread);
                        calls.set(calls.get() + 1);
                        assert_eq!(calls.get(), 1);
                        assert!(owner.borrow().shutdown_status().is_none());
                        main.update(app, |root, _, _| assert_eq!(root.controller().unwrap().window_id(), invoking)).unwrap();
                        match outcome {
                            Outcome::Idle | Outcome::Work => {
                                let result = result.unwrap();
                                assert_eq!(result.has_work(), outcome == Outcome::Work);
                                assert_eq!(result.running_threads(), if outcome == Outcome::Work { 1 } else { 0 });
                            }
                            _ => assert!(matches!(result, Err(AppServiceCloseError::Work(
                                crate::cas_projection::ShutdownWorkError::Work(crate::cas_projection::ProcessWorkError::Cancelled)
                            )))),
                        }
                        RunningProcessOwner::observe_shutdown_work(owner, ProjectionCancellationToken::new(), app,
                            move |owner, result, _| {
                                assert_eq!(std::thread::current().id(), gui_thread);
                                calls.set(calls.get() + 1);
                                assert_eq!(calls.get(), 2);
                                assert_eq!(result.unwrap().has_work(), outcome == Outcome::Work);
                                *completed.borrow_mut() = Some(owner.clone());
                            }).unwrap();
                    },
                    move || {
                        assert_ne!(std::thread::current().id(), gui_thread);
                        if outcome != Outcome::CancelAfter {
                            before_entered.store(true, Ordering::Release);
                            before_wait.recv_timeout(Duration::from_secs(10)).unwrap();
                        }
                    },
                    move |result| {
                        assert_ne!(std::thread::current().id(), gui_thread);
                        if outcome == Outcome::CancelAfter {
                            assert!(!result.as_ref().unwrap().has_work());
                            after_entered.store(true, Ordering::Release);
                            after_wait.recv_timeout(Duration::from_secs(10)).unwrap();
                        }
                    },
                )).unwrap().unwrap();
                cx.update(|app| {
                    assert!(RunningProcessOwner::observe_shutdown_work(&owner, ProjectionCancellationToken::new(), app,
                        |_, _, _| panic!("refused observation must not notify")).is_err());
                    assert!(matches!(owner.borrow_mut().try_begin_idle_shutdown(invoking, ShutdownIntent::FinalWindowClose, &idle, app),
                        Err(IdleShutdownError::IntentBusy)));
                    assert!(RunningProcessOwner::begin_shutdown_confirmation(&owner, invoking, ShutdownIntent::ApplicationExit, idle, app, |_, _| panic!("refused confirmation must not notify")).is_err());
                }).unwrap();
                let weak = Rc::downgrade(&owner);
                drop(owner);
                let deadline = Instant::now() + Duration::from_secs(5);
                while !entered.load(Ordering::Acquire) {
                    assert!(Instant::now() < deadline);
                    cx.background_executor().timer(Duration::from_millis(10)).await;
                }
                assert_eq!(callbacks.get(), 0);
                {
                    let owner = weak.upgrade().expect("worker continuation retains running owner");
                    let owner = owner.borrow();
                    assert!(owner.test_services().graph().unwrap().shutdown.is_none());
                    permit.commit(|| ()).unwrap();
                    drop(owner.test_services().windows.reserve_main_window(beryl_model::WindowId::from_bytes([235; 16])).unwrap());
                    main.update(cx, |root, _, _| assert_eq!(root.controller().unwrap().window_id(), invoking)).unwrap();
                }
                if matches!(outcome, Outcome::CancelBefore | Outcome::CancelAfter) { cancellation.cancel(); }
                if outcome == Outcome::CancelAfter { after_release.send(()).unwrap(); }
                else { before_release.send(()).unwrap(); }
                while completion.borrow().is_none() {
                    assert!(Instant::now() < deadline);
                    cx.background_executor().timer(Duration::from_millis(10)).await;
                }
                assert_eq!(callbacks.get(), 2);
                let owner = completion.borrow_mut().take().unwrap();
                permit.commit(|| ()).unwrap();
                drop(work);
                let running = Rc::try_unwrap(owner).ok().unwrap().into_inner().test_into_process();
                support::dispose_running(running, cx).await;
                observed.set(true);
                cx.update(|app| app.quit()).unwrap();
            }).detach();
        }, app);
        support::watchdog(app);
    });
    assert!(finished.get());
    assert_reopens(&directory);
}
