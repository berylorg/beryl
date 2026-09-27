use crate::running_owner::{RunningProcessOwner, RunningShutdownStatus, ShutdownIntent};
use beryl_model::WindowId;

#[test]
fn native_cancelled_progress_retains_owner_until_coherent_reopening() {
    super::running_idle_shutdown::run(ShutdownIntent::FinalWindowClose, false, Some(false));
}

#[test]
fn native_ready_progress_keeps_windows_and_requires_result_consumption() {
    super::running_idle_shutdown::run(ShutdownIntent::ApplicationExit, false, Some(true));
}

async fn settled(owner: &Rc<RefCell<RunningProcessOwner>>, cx: &mut AsyncApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !owner.borrow().test_shutdown_progress_settled() {
        assert!(
            Instant::now() < deadline,
            "shutdown progress did not settle"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert!(!owner.borrow().test_services_on_worker());
    cx.update(|app| {
        let window = owner.borrow().test_process().windows.shells()[0].window();
        let reopened = owner.borrow().shutdown_status().is_none();
        for _ in 0..2 {
            let result = RunningProcessOwner::release_shutdown_interaction_gate(owner, app);
            assert_eq!(result.is_ok(), reopened);
            assert_eq!(
                window
                    .read(app)
                    .unwrap()
                    .new_window_disabled_reason(app)
                    .as_deref()
                    == Some("Application Exit is waiting for active work and durable state."),
                !reopened
            );
        }
        assert!(owner.borrow().test_shutdown_progress_settled());
    })
    .unwrap();
    if owner.borrow().shutdown_status().is_none() {
        let invoking = owner.borrow().test_process().windows.window_ids()[0];
        let job = owner
            .borrow()
            .test_services()
            .prepare_shutdown_observation()
            .unwrap();
        let observation = cx
            .background_executor()
            .spawn(async move { job.collect(&ProjectionCancellationToken::new()).unwrap() })
            .await;
        assert!(matches!(
            cx.update(|app| owner.borrow_mut().try_begin_idle_shutdown(
                invoking,
                ShutdownIntent::ApplicationExit,
                &observation,
                app,
            ))
            .unwrap(),
            Err(crate::running_owner::IdleShutdownError::IntentBusy)
        ));
        assert!(
            cx.update(|app| RunningProcessOwner::begin_shutdown_confirmation(
                owner,
                invoking,
                ShutdownIntent::ApplicationExit,
                observation,
                app,
                |_, _| panic!("refused confirmation must not notify")
            ))
            .unwrap()
            .is_err()
        );
        assert!(owner.borrow().test_shutdown_progress_settled());
    }
}

pub(super) async fn exercise(
    owner: Rc<RefCell<RunningProcessOwner>>,
    invoking: WindowId,
    intent: ShutdownIntent,
    ready_first: bool,
    cx: &mut AsyncApp,
) -> Rc<RefCell<RunningProcessOwner>> {
    let window = owner.borrow().test_process().windows.shells()[0].window();
    let original_attempt = owner.borrow().test_services().graph().unwrap().shutdown;
    let cancellation = ProjectionCancellationToken::new();
    if !ready_first {
        cancellation.cancel();
    }
    let (release, wait) = std::sync::mpsc::sync_channel(1);
    let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started = entered.clone();
    let gui_thread = std::thread::current().id();
    let notifications = Rc::new(Cell::new(0));
    let delivered = notifications.clone();
    let callback_cancellation = cancellation.clone();
    cx.update(|app| {
        RunningProcessOwner::test_advance_shutdown_with(
            &owner,
            cancellation.clone(),
            app,
            move |owner, app| {
                assert_eq!(std::thread::current().id(), gui_thread);
                assert!(!owner.borrow().test_services_on_worker());
                assert!(owner.borrow().test_shutdown_progress_settled());
                window
                    .update(app, |root, _, _| {
                        assert_eq!(root.controller().unwrap().window_id(), invoking);
                    })
                    .unwrap();
                delivered.set(delivered.get() + 1);
                assert!(
                    RunningProcessOwner::advance_shutdown(
                        owner,
                        callback_cancellation.clone(),
                        app,
                        |_, _| panic!("refused progress must not notify"),
                    )
                    .is_err()
                );
                if ready_first {
                    let result = owner
                        .borrow_mut()
                        .take_shutdown_progress()
                        .unwrap()
                        .unwrap();
                    assert!(matches!(
                        result,
                        AppServiceShutdownProgress::Ready | AppServiceShutdownProgress::Waiting
                    ));
                    assert!(owner.borrow_mut().take_shutdown_progress().is_none());
                    RunningProcessOwner::advance_shutdown(
                        owner,
                        callback_cancellation,
                        app,
                        move |owner, _| {
                            assert_eq!(std::thread::current().id(), gui_thread);
                            assert!(!owner.borrow().test_services_on_worker());
                            assert!(owner.borrow().test_shutdown_progress_settled());
                            delivered.set(delivered.get() + 1);
                        },
                    )
                    .unwrap();
                }
            },
            move || {
                assert_ne!(std::thread::current().id(), gui_thread);
                started.store(true, Ordering::SeqCst);
                wait.recv_timeout(Duration::from_secs(10)).unwrap();
            },
        )
    })
    .unwrap()
    .unwrap();
    assert!(owner.borrow().test_services_on_worker());
    assert!(
        cx.update(|app| RunningProcessOwner::release_shutdown_interaction_gate(&owner, app))
            .unwrap()
            .is_err()
    );
    assert!(owner.borrow_mut().take_shutdown_progress().is_none());
    assert!(owner.borrow_mut().end_unadmitted_shutdown().is_err());
    assert_eq!(notifications.get(), 0);
    assert!(
        cx.update(|app| RunningProcessOwner::advance_shutdown(
            &owner,
            cancellation.clone(),
            app,
            |_, _| panic!("pending progress must not notify")
        ))
        .unwrap()
        .is_err()
    );
    let weak = Rc::downgrade(&owner);
    drop(owner);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !entered.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline);
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    // Only the detached continuation retained this native owner while the worker waited.
    let owner = weak
        .upgrade()
        .expect("worker continuation retains native custody");
    window
        .update(cx, |root, _, _| {
            assert_eq!(root.controller().unwrap().window_id(), invoking);
        })
        .unwrap();
    assert_eq!(
        owner.borrow().shutdown_status(),
        Some((invoking, intent, RunningShutdownStatus::Admitted))
    );
    release.send(()).unwrap();
    settled(&owner, cx).await;
    assert_eq!(notifications.get(), if ready_first { 2 } else { 1 });
    assert!(
        cx.update(|app| RunningProcessOwner::advance_shutdown(
            &owner,
            cancellation.clone(),
            app,
            |_, _| {}
        ))
        .unwrap()
        .is_err()
    );
    let mut result = owner
        .borrow_mut()
        .take_shutdown_progress()
        .unwrap()
        .unwrap();
    assert!(owner.borrow_mut().take_shutdown_progress().is_none());
    if ready_first {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match result {
                AppServiceShutdownProgress::Ready => break,
                AppServiceShutdownProgress::Waiting => {
                    assert!(Instant::now() < deadline);
                    cx.update(|app| {
                        RunningProcessOwner::advance_shutdown(
                            &owner,
                            cancellation.clone(),
                            app,
                            |_, _| {},
                        )
                    })
                    .unwrap()
                    .unwrap();
                    settled(&owner, cx).await;
                    result = owner
                        .borrow_mut()
                        .take_shutdown_progress()
                        .unwrap()
                        .unwrap();
                }
                other => panic!("unexpected idle progress: {other:?}"),
            }
        }
        assert_eq!(
            owner.borrow().test_services().graph().unwrap().shutdown,
            original_attempt
        );
        assert_eq!(
            owner.borrow().shutdown_status(),
            Some((invoking, intent, RunningShutdownStatus::Admitted))
        );
        assert!(owner.borrow_mut().end_unadmitted_shutdown().is_err());
        window.update(cx, |_, _, _| ()).unwrap();
        cancellation.cancel();
        cx.update(|app| {
            RunningProcessOwner::advance_shutdown(&owner, cancellation.clone(), app, |_, _| {})
        })
        .unwrap()
        .unwrap();
        settled(&owner, cx).await;
        result = owner
            .borrow_mut()
            .take_shutdown_progress()
            .unwrap()
            .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match result {
            AppServiceShutdownProgress::Failed { reopened: true, .. } => break,
            AppServiceShutdownProgress::Failed {
                reopened: false, ..
            } => {
                assert!(Instant::now() < deadline);
                assert!(owner.borrow().shutdown_status().is_some());
                cx.update(|app| {
                    RunningProcessOwner::advance_shutdown(
                        &owner,
                        cancellation.clone(),
                        app,
                        |_, _| {},
                    )
                })
                .unwrap()
                .unwrap();
                settled(&owner, cx).await;
                result = owner
                    .borrow_mut()
                    .take_shutdown_progress()
                    .unwrap()
                    .unwrap();
            }
            other => panic!("unexpected cancelled progress: {other:?}"),
        }
    }
    assert!(owner.borrow().shutdown_status().is_none());
    assert!(
        cx.update(|app| RunningProcessOwner::release_shutdown_interaction_gate(&owner, app))
            .unwrap()
            .is_err()
    );
    assert!(
        owner
            .borrow()
            .test_services()
            .graph()
            .unwrap()
            .shutdown
            .is_none()
    );
    owner
        .borrow()
        .test_services()
        .process
        .execution_permit()
        .commit(|| ())
        .unwrap();
    owner
        .borrow()
        .test_services()
        .graph()
        .unwrap()
        .restored_window_attempt()
        .unwrap()
        .validate_lifetime()
        .unwrap();
    drop(
        owner
            .borrow()
            .test_services()
            .windows
            .reserve_main_window(WindowId::from_bytes([236; 16]))
            .unwrap(),
    );
    window.update(cx, |_, _, _| ()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let job = owner
            .borrow()
            .test_services()
            .prepare_shutdown_observation()
            .unwrap();
        let observation = cx
            .background_executor()
            .spawn(async move { job.collect(&ProjectionCancellationToken::new()).unwrap() })
            .await;
        if cx
            .update(|app| {
                owner
                    .borrow_mut()
                    .try_begin_idle_shutdown(invoking, intent, &observation, app)
            })
            .unwrap()
            .is_ok()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "fresh explicit admission did not settle"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert_ne!(
        owner.borrow().test_services().graph().unwrap().shutdown,
        original_attempt
    );
    assert_eq!(
        owner.borrow().shutdown_status(),
        Some((invoking, intent, RunningShutdownStatus::Admitted))
    );
    cx.update(|app| RunningProcessOwner::install_shutdown_interaction_gate(&owner, app))
        .unwrap()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.update(|app| {
            RunningProcessOwner::advance_shutdown(&owner, cancellation.clone(), app, |_, _| {})
        })
        .unwrap()
        .unwrap();
        settled(&owner, cx).await;
        match owner
            .borrow_mut()
            .take_shutdown_progress()
            .unwrap()
            .unwrap()
        {
            AppServiceShutdownProgress::Failed { reopened: true, .. } => break,
            AppServiceShutdownProgress::Failed {
                reopened: false, ..
            } => {
                assert!(Instant::now() < deadline);
            }
            other => panic!("unexpected successor cancellation: {other:?}"),
        }
    }
    assert!(owner.borrow().shutdown_status().is_none());
    owner
}
