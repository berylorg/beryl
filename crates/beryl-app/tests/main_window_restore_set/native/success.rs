use super::*;
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::WindowsAndMessaging::{SendMessageW, WM_CLOSE},
};

#[test]
fn native_complete_set_publishes_in_order_and_releases_interaction_together() {
    let fixtures =
        home_support::worker(|| vec![fixture(Kind::Restored, 151), fixture(Kind::Threadless, 161)])
            .join()
            .unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let result = finished.clone();
    Application::new().run(move |app| {
        let (control, _) = windows_native::open(app, "restore-set-success");
        app.spawn(async move |cx| {
            for NativeFixture {
                fixture,
                lifetime,
                mut prepared,
                appearance,
                saved,
                ids,
                cancellation: _,
            } in fixtures
            {
                let handles = Arc::new(std::sync::Mutex::new(Vec::new()));
                let observed = handles.clone();
                prepared.test_before_publication(move |index, shell, app| {
                    assert_eq!(index, observed.lock().unwrap().len());
                    assert_gated(shell, true, app);
                    let raw = raw_handle(index, shell, app);
                    assert!(!windows_native::visible(raw));
                    for previous in observed.lock().unwrap().iter().copied() {
                        assert!(windows_native::visible(previous));
                        unsafe {
                            SendMessageW(
                                windows_native::hwnd(previous),
                                WM_CLOSE,
                                Some(WPARAM(0)),
                                Some(LPARAM(0)),
                            );
                        }
                        assert!(windows_native::alive(previous));
                    }
                    observed.lock().unwrap().push(raw);
                    Ok(())
                });
                let complete = Rc::new(RefCell::new(None));
                let delivered = complete.clone();
                let observer = Rc::new(());
                let owned_observer = observer.clone();
                cx.update(|app| {
                    let appearance = GpuiAppearanceWindowSet::new(
                        appearance,
                        NonZeroUsize::new(256).unwrap(),
                        app,
                    );
                    let cancellation = prepared.start(
                        appearance,
                        move |value, _| {
                            assert_eq!(Rc::strong_count(&owned_observer), 1);
                            assert!(delivered.borrow().is_none());
                            *delivered.borrow_mut() = Some(value);
                        },
                        app,
                    );
                    drop(cancellation);
                })
                .unwrap();
                drop(observer);
                let published = match wait_for(&complete, cx).await {
                    MainWindowNativeRestoreSetCompletion::Published(value) => value,
                    MainWindowNativeRestoreSetCompletion::Failed(value) => {
                        panic!("{}", value.error)
                    }
                };
                assert_eq!(published.window_ids(), ids);
                assert_eq!(published.shells().len(), ids.len());
                assert_eq!(handles.lock().unwrap().len(), ids.len());
                cx.update(|app| {
                    for shell in published.shells() {
                        assert_gated(shell, false, app);
                    }
                })
                .unwrap();
                assert!(
                    handles
                        .lock()
                        .unwrap()
                        .iter()
                        .all(|raw| windows_native::visible(*raw))
                );
                for raw in handles.lock().unwrap().iter().copied() {
                    unsafe {
                        SendMessageW(
                            windows_native::hwnd(raw),
                            WM_CLOSE,
                            Some(WPARAM(0)),
                            Some(LPARAM(0)),
                        );
                    }
                    assert!(windows_native::alive(raw));
                }
                cx.update(|app| {
                    for shell in published.shells() {
                        assert_gated(shell, false, app);
                    }
                })
                .unwrap();
                assert_eq!(fixture.process.main_window_occupancy(), ids.len());
                let (fixture, saved) = cx
                    .background_executor()
                    .spawn(async move {
                        assert_eq!(snapshot(&fixture), saved);
                        (fixture, saved)
                    })
                    .await;
                let disposed = Rc::new(RefCell::new(None));
                let delivered = disposed.clone();
                cx.update(|app| {
                    published.test_dispose(
                        move |value, _| {
                            *delivered.borrow_mut() = Some(value);
                        },
                        app,
                    )
                })
                .unwrap();
                let disposal = wait_for(&disposed, cx).await;
                assert!(disposal.retained.is_none(), "{}", disposal.error);
                assert!(disposal.diagnostics.is_empty());
                assert!(
                    handles
                        .lock()
                        .unwrap()
                        .iter()
                        .all(|raw| !windows_native::alive(*raw))
                );
                assert_eq!(fixture.process.main_window_occupancy(), 0);
                drop((disposal, lifetime));
                cx.background_executor()
                    .spawn(async move {
                        assert_eq!(snapshot(&fixture), saved);
                        cleanup(fixture);
                    })
                    .await;
            }
            result.store(true, Ordering::SeqCst);
            control
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        })
        .detach();
    });
    assert!(finished.load(Ordering::SeqCst));
}
