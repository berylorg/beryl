use super::*;
use beryl_home_store::test_faults::FaultPoint;

#[test]
fn native_set_destroys_windows_and_retains_each_retirement_when_shared_storage_fails() {
    let NativeFixture {
        fixture,
        lifetime,
        mut prepared,
        appearance,
        saved,
        ids,
        cancellation: _,
    } = home_support::worker(|| fixture(Kind::Restored, 201))
        .join()
        .unwrap();
    let first = ids[0];
    let faults = fixture.faults.clone();
    prepared.test_before_retirement(move |id, retirement| {
        if id == first {
            assert!(matches!(
                retirement,
                MainWindowStartupRetirement::Restored(_)
            ));
            faults.fail_next(FaultPoint::BeforeCommit);
        }
    });
    prepared.test_before_publication(|index, _, _| {
        if index == 1 {
            Err("stop after visible first member".to_owned())
        } else {
            Ok(())
        }
    });
    let finished = Arc::new(AtomicBool::new(false));
    let result = finished.clone();
    Application::new().run(move |app| {
        let (control, _) = windows_native::open(app, "restore-set-retained");
        let handles = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed = handles.clone();
        prepared.test_after_desktop(move |index, shell, app| {
            observed.lock().unwrap().push(raw_handle(index, shell, app));
        });
        let complete = Rc::new(RefCell::new(None));
        let delivered = complete.clone();
        let appearance =
            GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(256).unwrap(), app);
        drop(prepared.start(
            appearance,
            move |value, _| {
                *delivered.borrow_mut() = Some(value);
            },
            app,
        ));
        app.spawn(async move |cx| {
            let failure = match wait_for(&complete, cx).await {
                MainWindowNativeRestoreSetCompletion::Failed(value) => value,
                MainWindowNativeRestoreSetCompletion::Published(_) => {
                    panic!("unexpected publication")
                }
            };
            assert_eq!(
                failure
                    .diagnostics
                    .iter()
                    .map(|item| item.window_id)
                    .collect::<Vec<_>>(),
                ids
            );
            assert!(
                failure
                    .diagnostics
                    .iter()
                    .all(|item| item.error.contains("store is Failed"))
            );
            let retained = failure
                .retained
                .as_ref()
                .expect("original failed retirement retained");
            assert_eq!(retained.window_ids(), ids);
            assert_eq!(retained.members().len(), ids.len());
            for (member, expected) in retained.members().iter().zip(&ids) {
                assert_eq!(member.window_id, *expected);
                assert!(matches!(
                    &member.custody,
                    MainWindowNativeRetainedCustody::Restored(_)
                ));
            }
            assert_eq!(retained.shells().count(), 0);
            assert!(
                handles
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|raw| !windows_native::alive(*raw))
            );
            assert_eq!(fixture.process.main_window_occupancy(), ids.len());
            drop((failure, lifetime));
            assert_eq!(fixture.process.main_window_occupancy(), 0);
            cx.background_executor()
                .spawn(async move {
                    let Fixture {
                        directory,
                        store,
                        state,
                        storage,
                        faults,
                        process,
                        service,
                        ..
                    } = fixture;
                    drop((state, storage, faults, process, service));
                    let store = Arc::try_unwrap(store)
                        .unwrap_or_else(|_| panic!("test owner still holds home"));
                    let recovered = store.recover_same_home().unwrap();
                    let state = BerylState::reacquire_candidate(&recovered).unwrap();
                    let store = recovered.publish().unwrap();
                    assert_eq!(
                        state.session().minimal_bootstrap(&store).unwrap().unwrap(),
                        saved
                    );
                    drop(state);
                    store.close().unwrap();
                    directory.close().unwrap();
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
fn native_release_failure_regates_members_and_retains_unreleased_editor() {
    let NativeFixture {
        fixture,
        lifetime,
        mut prepared,
        appearance,
        saved,
        ids,
        cancellation: _,
    } = home_support::worker(|| fixture(Kind::Restored, 211))
        .join()
        .unwrap();
    prepared.test_before_interaction_release(|shells, app| {
        shells
            .last()
            .unwrap()
            .window()
            .update(app, |root, _, cx| {
                let mount = root.controller().unwrap().composer_mount().unwrap();
                let composer = mount.read(cx).contribution().unwrap();
                composer.update(cx, |composer, cx| {
                    composer.test_set_terminal_error("late startup failure".to_owned(), cx);
                });
            })
            .unwrap();
    });
    let finished = Arc::new(AtomicBool::new(false));
    let result = finished.clone();
    Application::new().run(move |app| {
        let (control, _) = windows_native::open(app, "restore-set-release-failure");
        let handles = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed = handles.clone();
        prepared.test_after_desktop(move |index, shell, app| {
            observed.lock().unwrap().push(raw_handle(index, shell, app));
        });
        let complete = Rc::new(RefCell::new(None));
        let delivered = complete.clone();
        let appearance =
            GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(256).unwrap(), app);
        drop(prepared.start(
            appearance,
            move |value, _| {
                *delivered.borrow_mut() = Some(value);
            },
            app,
        ));
        app.spawn(async move |cx| {
            let failure = match wait_for(&complete, cx).await {
                MainWindowNativeRestoreSetCompletion::Failed(value) => value,
                MainWindowNativeRestoreSetCompletion::Published(_) => {
                    panic!("unexpected publication")
                }
            };
            let retained = failure
                .retained
                .as_ref()
                .expect("editor failure retains exact native owner");
            assert_eq!(retained.members().len(), 1);
            assert_eq!(retained.members()[0].window_id, ids[1]);
            assert_eq!(retained.shells().count(), 1);
            assert!(!windows_native::alive(handles.lock().unwrap()[0]));
            assert!(windows_native::alive(handles.lock().unwrap()[1]));
            assert_eq!(fixture.process.main_window_occupancy(), 1);
            cx.update(|app| {
                for shell in retained.shells() {
                    assert_gated(shell, true, app);
                    shell
                        .window()
                        .update(app, |_, window, _| window.remove_window())
                        .unwrap();
                }
            })
            .unwrap();
            drop((failure, lifetime));
            windows_native::pump(cx).await;
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
