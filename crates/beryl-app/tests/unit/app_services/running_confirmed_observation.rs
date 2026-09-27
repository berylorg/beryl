use crate::running_owner::{ConfirmedShutdownAdmission, RunningShutdownStatus};

pub(super) async fn exercise(
    owner: Rc<RefCell<RunningProcessOwner>>,
    context: crate::running_owner::ShutdownConfirmationContext,
    choice: Choice,
    cx: &mut AsyncApp,
) -> Rc<RefCell<RunningProcessOwner>> {
    let invoking = context.invoking();
    let intent = context.intent();
    let main = owner.borrow().test_process().windows.shells()[0].window();
    let permit = owner.borrow().test_services().process.execution_permit();
    owner
        .borrow_mut()
        .begin_confirmed_shutdown(context)
        .unwrap();
    let reservation = (choice == Choice::WorkerAdmission).then(|| permit.reserve().unwrap());
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
    cx.update(|app| {
        RunningProcessOwner::test_observe_confirmed_shutdown_with(
            &owner,
            cancellation.clone(),
            app,
            move |owner, result, app| {
                assert_eq!(std::thread::current().id(), gui_thread);
                calls.set(calls.get() + 1);
                assert_eq!(calls.get(), 1);
                assert_eq!(
                    owner.borrow().shutdown_status(),
                    Some((invoking, intent, RunningShutdownStatus::AwaitingObservation))
                );
                main.update(app, |root, _, _| {
                    assert_eq!(root.controller().unwrap().window_id(), invoking);
                })
                .unwrap();
                if choice == Choice::WorkerAdmission {
                    assert!(
                        result.is_err(),
                        "live execution reservation must refuse admission"
                    );
                    drop(reservation);
                    // Reenter scheduling directly from completion, without an owner borrow.
                    RunningProcessOwner::observe_confirmed_shutdown(
                        owner,
                        ProjectionCancellationToken::new(),
                        app,
                        move |owner, result, _| {
                            calls.set(calls.get() + 1);
                            assert_eq!(calls.get(), 2);
                            *completed.borrow_mut() = Some((owner.clone(), result));
                        },
                    )
                    .unwrap();
                } else {
                    assert!(matches!(result, Ok(ConfirmedShutdownAdmission::Cancelled)));
                    owner.borrow_mut().end_unadmitted_shutdown().unwrap();
                    assert!(owner.borrow().shutdown_status().is_none());
                    *completed.borrow_mut() = Some((owner.clone(), result));
                }
            },
            move || {
                assert_ne!(std::thread::current().id(), gui_thread);
                if choice != Choice::WorkerCancelAfter {
                    before_entered.store(true, Ordering::Release);
                    before_wait.recv_timeout(Duration::from_secs(10)).unwrap();
                }
            },
            move |result| {
                assert_ne!(std::thread::current().id(), gui_thread);
                if choice == Choice::WorkerCancelAfter {
                    let _ = result.test_duplicate_success();
                    after_entered.store(true, Ordering::Release);
                    after_wait.recv_timeout(Duration::from_secs(10)).unwrap();
                }
            },
        )
    })
    .unwrap()
    .unwrap();
    assert!(
        cx.update(|app| RunningProcessOwner::observe_confirmed_shutdown(
            &owner,
            ProjectionCancellationToken::new(),
            app,
            |_, _, _| panic!("rejected duplicate cannot receive completion"),
        ))
        .unwrap()
        .is_err()
    );
    assert!(owner.borrow_mut().end_unadmitted_shutdown().is_err());
    let weak = Rc::downgrade(&owner);
    drop(owner);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "worker did not enter");
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    {
        let owner = weak
            .upgrade()
            .expect("worker continuation retains complete owner");
        assert_eq!(
            owner.borrow().shutdown_status().unwrap().2,
            RunningShutdownStatus::Observing
        );
        assert!(owner.borrow_mut().end_unadmitted_shutdown().is_err());
        permit.commit(|| ()).unwrap();
        assert!(completion.borrow().is_none());
        assert_eq!(callbacks.get(), 0);
        main.update(cx, |root, _, _| {
            assert_eq!(root.controller().unwrap().window_id(), invoking);
        })
        .unwrap();
    }
    if choice != Choice::WorkerAdmission {
        cancellation.cancel();
    }
    if choice == Choice::WorkerCancelAfter {
        after_release.send(()).unwrap();
    } else {
        before_release.send(()).unwrap();
    }
    let (owner, mut result) = loop {
        if let Some(result) = completion.borrow_mut().take() {
            break result;
        }
        assert!(Instant::now() < deadline, "GUI completion did not arrive");
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    };
    if choice == Choice::WorkerAdmission {
        assert_eq!(callbacks.get(), 2);
        // A fresh bounded read may refuse on concurrent service revisions.
        while result.is_err() {
            assert!(
                Instant::now() < deadline,
                "fresh admission refused: {result:?}"
            );
            permit.commit(|| ()).unwrap();
            let completed = completion.clone();
            cx.update(|app| {
                RunningProcessOwner::observe_confirmed_shutdown(
                    &owner,
                    ProjectionCancellationToken::new(),
                    app,
                    move |owner, result, _| *completed.borrow_mut() = Some((owner.clone(), result)),
                )
            })
            .unwrap()
            .unwrap();
            result = loop {
                if let Some((returned, result)) = completion.borrow_mut().take() {
                    assert!(Rc::ptr_eq(&returned, &owner));
                    break result;
                }
                assert!(
                    Instant::now() < deadline,
                    "fresh observation did not settle"
                );
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
            };
        }
        assert!(matches!(result, Ok(ConfirmedShutdownAdmission::Admitted)));
        assert_eq!(
            owner.borrow().shutdown_status(),
            Some((invoking, intent, RunningShutdownStatus::Admitted))
        );
        assert!(owner.borrow_mut().end_unadmitted_shutdown().is_err());
    } else {
        assert_eq!(callbacks.get(), 1);
        permit.commit(|| ()).unwrap();
        drop(
            owner
                .borrow()
                .test_services()
                .windows
                .reserve_main_window(beryl_model::WindowId::from_bytes([232; 16]))
                .unwrap(),
        );
    }
    owner
}
