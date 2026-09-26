use super::*;

#[derive(Clone, Copy, Debug)]
enum FailureCase {
    LastConstruction,
    StaleValidation,
    CancelAfterDesktop,
    CancelFinalValidation,
    LaterPublication,
    ReplacementConstruction,
    ReplacementPublication,
}

#[test]
fn native_failed_set_drains_members_and_preserves_every_sealed_record() {
    let fixtures = home_support::worker(|| {
        [
            FailureCase::LastConstruction,
            FailureCase::StaleValidation,
            FailureCase::CancelAfterDesktop,
            FailureCase::CancelFinalValidation,
            FailureCase::LaterPublication,
            FailureCase::ReplacementConstruction,
            FailureCase::ReplacementPublication,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, case)| {
            let kind = if matches!(
                case,
                FailureCase::ReplacementConstruction | FailureCase::ReplacementPublication
            ) {
                Kind::Replacement
            } else {
                Kind::Restored
            };
            (case, fixture(kind, 171 + index as u8 * 3))
        })
        .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let result = finished.clone();
    Application::new().run(move |app| {
        let (control, _) = windows_native::open(app, "restore-set-failures");
        app.spawn(async move |cx| {
            for (
                case,
                NativeFixture {
                    fixture,
                    lifetime,
                    mut prepared,
                    appearance,
                    saved,
                    ids,
                    cancellation,
                },
            ) in fixtures
            {
                let handles = Arc::new(std::sync::Mutex::new(Vec::new()));
                let observed = handles.clone();
                let after_desktop_cancellation = cancellation.clone();
                prepared.test_after_desktop(move |index, shell, app| {
                    let raw = raw_handle(index, shell, app);
                    assert!(!windows_native::visible(raw));
                    observed.lock().unwrap().push(raw);
                    if matches!(case, FailureCase::CancelAfterDesktop) {
                        after_desktop_cancellation.cancel();
                    }
                });
                match case {
                    FailureCase::LastConstruction | FailureCase::ReplacementConstruction => {
                        let fail_at = ids.len() - 1;
                        prepared.test_before_construction(move |index, host| {
                            if index == fail_at {
                                host.test_fail_startup_construction_at(
                                    MainWindowStartupConstructionFault::ComposerSetup,
                                );
                            }
                        });
                    }
                    FailureCase::StaleValidation => {
                        let store = fixture.store.clone();
                        let state = fixture.state.clone();
                        let id = ids[0];
                        prepared.test_before_final_validation(move || {
                            change_saved_placement(&store, &state, id);
                        });
                    }
                    FailureCase::CancelFinalValidation => {
                        prepared.test_before_final_validation(move || cancellation.cancel());
                    }
                    FailureCase::LaterPublication | FailureCase::ReplacementPublication => {
                        let observed = handles.clone();
                        let fail_at = ids.len() - 1;
                        prepared.test_before_publication(move |index, shell, app| {
                            assert_gated(shell, true, app);
                            if index == fail_at {
                                for raw in observed.lock().unwrap().iter().take(index) {
                                    assert!(windows_native::visible(*raw));
                                }
                                Err("injected later publication failure".to_owned())
                            } else {
                                Ok(())
                            }
                        });
                    }
                    FailureCase::CancelAfterDesktop => {}
                }
                let complete = Rc::new(RefCell::new(None));
                let delivered = complete.clone();
                cx.update(|app| {
                    let appearance = GpuiAppearanceWindowSet::new(
                        appearance,
                        NonZeroUsize::new(256).unwrap(),
                        app,
                    );
                    drop(prepared.start(
                        appearance,
                        move |value, _| {
                            *delivered.borrow_mut() = Some(value);
                        },
                        app,
                    ));
                })
                .unwrap();
                let failure = match wait_for(&complete, cx).await {
                    MainWindowNativeRestoreSetCompletion::Failed(value) => value,
                    MainWindowNativeRestoreSetCompletion::Published(_) => {
                        panic!("{case:?} published")
                    }
                };
                assert!(!failure.error.is_empty());
                assert!(failure.retained.is_none(), "{case:?}: {}", failure.error);
                assert!(failure.diagnostics.is_empty(), "{case:?}");
                assert!(
                    handles
                        .lock()
                        .unwrap()
                        .iter()
                        .all(|raw| !windows_native::alive(*raw))
                );
                assert_eq!(fixture.process.main_window_occupancy(), 0, "{case:?}");
                drop((failure, lifetime));
                cx.background_executor()
                    .spawn(async move {
                        let after = snapshot(&fixture);
                        match case {
                            FailureCase::ReplacementConstruction => {
                                assert!(after.windows().is_empty())
                            }
                            FailureCase::StaleValidation => {
                                assert_ne!(after, saved);
                                assert_eq!(
                                    after
                                        .windows()
                                        .iter()
                                        .map(|w| w.window_id())
                                        .collect::<Vec<_>>(),
                                    ids
                                );
                            }
                            _ => assert_eq!(after, saved, "{case:?}"),
                        }
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
