use super::*;

#[test]
fn native_cancellation_wakes_a_set_waiting_for_readiness() {
    let NativeFixture {
        fixture,
        lifetime,
        mut prepared,
        appearance,
        saved,
        cancellation: _,
        ids: _,
    } = home_support::worker(|| fixture(Kind::Restored, 231))
        .join()
        .unwrap();
    let waiting = Arc::new(AtomicBool::new(false));
    let observed = waiting.clone();
    prepared.test_hold_readiness(move || observed.store(true, Ordering::SeqCst));
    let finished = Arc::new(AtomicBool::new(false));
    let result = finished.clone();
    Application::new().run(move |app| {
        let (control, _) = windows_native::open(app, "restore-set-readiness-cancel");
        let complete = Rc::new(RefCell::new(None));
        let delivered = complete.clone();
        let appearance =
            GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(256).unwrap(), app);
        let cancellation = prepared.start(
            appearance,
            move |value, _| {
                *delivered.borrow_mut() = Some(value);
            },
            app,
        );
        app.spawn(async move |cx| {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !waiting.load(Ordering::SeqCst) {
                assert!(
                    Instant::now() < deadline,
                    "startup did not reach readiness wait"
                );
                windows_native::pump(cx).await;
            }
            assert!(complete.borrow().is_none());
            cancellation.cancel();
            let failure = match wait_for(&complete, cx).await {
                MainWindowNativeRestoreSetCompletion::Failed(value) => value,
                MainWindowNativeRestoreSetCompletion::Published(_) => {
                    panic!("waiting set published")
                }
            };
            assert!(failure.error.contains("cancelled"));
            assert!(failure.retained.is_none(), "{}", failure.error);
            assert!(failure.diagnostics.is_empty());
            assert_eq!(fixture.process.main_window_occupancy(), 0);
            drop((failure, lifetime));
            cx.background_executor()
                .spawn(async move {
                    assert_eq!(snapshot(&fixture), saved);
                    cleanup(fixture);
                })
                .await;
            result.store(true, Ordering::SeqCst);
            control
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        })
        .detach();
    });
    assert!(finished.load(Ordering::SeqCst));
}

#[test]
fn native_cancel_waits_for_last_desktop_worker_before_draining_the_set() {
    let NativeFixture {
        fixture,
        lifetime,
        mut prepared,
        appearance,
        saved,
        ids,
        cancellation: _,
    } = home_support::worker(|| fixture(Kind::Restored, 221))
        .join()
        .unwrap();
    let (release, hold) = std::sync::mpsc::channel();
    let mut hold = Some(hold);
    let entered = Arc::new(AtomicBool::new(false));
    let signaled = entered.clone();
    let handles = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = handles.clone();
    let last = ids.len() - 1;
    prepared.test_before_desktop(move |index, shell, app| {
        observed.lock().unwrap().push(raw_handle(index, shell, app));
        if index == last {
            shell.test_hold_desktop_worker(hold.take().unwrap());
            signaled.store(true, Ordering::SeqCst);
        }
    });
    let finished = Arc::new(AtomicBool::new(false));
    let result = finished.clone();
    Application::new().run(move |app| {
        let (control, _) = windows_native::open(app, "restore-set-cancel-flight");
        let complete = Rc::new(RefCell::new(None));
        let delivered = complete.clone();
        let appearance =
            GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(256).unwrap(), app);
        let cancellation = prepared.start(
            appearance,
            move |value, _| {
                *delivered.borrow_mut() = Some(value);
            },
            app,
        );
        app.spawn(async move |cx| {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !entered.load(Ordering::SeqCst) {
                assert!(
                    Instant::now() < deadline,
                    "last desktop operation was not reached"
                );
                windows_native::pump(cx).await;
            }
            windows_native::pump(cx).await;
            cancellation.cancel();
            windows_native::pump(cx).await;
            assert!(
                complete.borrow().is_none(),
                "cancel must join worker custody"
            );
            assert_eq!(fixture.process.main_window_occupancy(), ids.len());
            assert!(
                handles
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|raw| windows_native::alive(*raw) && !windows_native::visible(*raw))
            );
            release.send(()).unwrap();
            let failure = match wait_for(&complete, cx).await {
                MainWindowNativeRestoreSetCompletion::Failed(value) => value,
                MainWindowNativeRestoreSetCompletion::Published(_) => {
                    panic!("cancelled startup published")
                }
            };
            assert!(failure.retained.is_none(), "{}", failure.error);
            assert!(failure.diagnostics.is_empty());
            assert!(
                handles
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|raw| !windows_native::alive(*raw))
            );
            assert_eq!(fixture.process.main_window_occupancy(), 0);
            drop((failure, lifetime));
            cx.background_executor()
                .spawn(async move {
                    assert_eq!(snapshot(&fixture), saved);
                    cleanup(fixture);
                })
                .await;
            result.store(true, Ordering::SeqCst);
            control
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        })
        .detach();
    });
    assert!(finished.load(Ordering::SeqCst));
}
