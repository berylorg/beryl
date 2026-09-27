use crate::running_owner::{RunningProcessOwner, StartupCleanup};

mod exit_delivery {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_exit_delivery.rs"
    ));
}

#[test]
fn native_running_owner_without_auxiliary_is_settled() {
    run(false, false);
}

#[test]
fn native_running_owner_retains_cleanup_and_deferred_exit() {
    run(true, false);
}

#[test]
fn native_running_owner_retains_failed_cleanup_without_rolling_back_windows() {
    run(true, true);
}

fn run(with_surface: bool, lose_surface: bool) {
    let directory = support::native_home();
    let opened = Arc::new(AtomicUsize::new(0));
    let input = input(directory.path(), move |path, _| {
        if opened.fetch_add(1, Ordering::SeqCst) == 0 && with_surface {
            StartupHomeOpen::Failed {
                detail: "initial open failure".into(),
                retained: None,
            }
        } else {
            support::open(path)
        }
    });
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                input,
                move |result, app| {
                    let StartupCompletion::Running(running) = result else {
                        panic!("expected native startup success")
                    };
                    let ids = running.windows.window_ids().to_vec();
                    let main = running.windows.shells()[0].window();
                    let auxiliary = running
                        .startup_surface
                        .as_ref()
                        .map(|surface| surface.window);
                    if let Some(window) = auxiliary {
                        window
                            .update(app, |surface, window, cx| {
                                surface.request_exit(cx);
                                if lose_surface {
                                    window.remove_window();
                                }
                            })
                            .unwrap();
                    }
                    app.spawn(async move |cx| {
                        if lose_surface {
                            cx.background_executor()
                                .timer(Duration::from_millis(10))
                                .await;
                            assert!(auxiliary.unwrap().update(cx, |_, _, _| ()).is_err());
                        }
                        let owner = cx
                            .update(|app| {
                                let owner = RunningProcessOwner::start(running, app);
                                if with_surface {
                                    assert_eq!(
                                        owner.borrow().startup_cleanup(),
                                        &StartupCleanup::Pending
                                    );
                                    let weak = Rc::downgrade(&owner);
                                    drop(owner);
                                    weak.upgrade().expect("cleanup task retains complete owner")
                                } else {
                                    assert_eq!(
                                        owner.borrow().startup_cleanup(),
                                        &StartupCleanup::Settled
                                    );
                                    owner
                                }
                            })
                            .unwrap();
                        while owner.borrow().startup_cleanup() == &StartupCleanup::Pending {
                            cx.background_executor()
                                .timer(Duration::from_millis(10))
                                .await;
                        }
                        {
                            let owner = owner.borrow();
                            assert_eq!(owner.test_process().windows.window_ids(), ids);
                            assert_eq!(owner.test_process().windows.shells()[0].window(), main);
                            assert!(owner.test_services().graph().is_some());
                            assert_eq!(owner.exit_requested(), with_surface);
                            match owner.startup_cleanup() {
                                StartupCleanup::Failed(detail) => {
                                    assert!(lose_surface);
                                    assert!(!detail.is_empty());
                                    assert_eq!(
                                        owner
                                            .test_process()
                                            .startup_surface
                                            .as_ref()
                                            .unwrap()
                                            .window,
                                        auxiliary.unwrap()
                                    );
                                }
                                StartupCleanup::Settled => {
                                    assert!(!lose_surface);
                                    assert!(owner.test_process().startup_surface.is_none());
                                }
                                StartupCleanup::Pending => unreachable!(),
                            }
                        }
                        main.update(cx, |root, _, _| assert!(!root.startup_interaction_gated()))
                            .unwrap();
                        if let Some(auxiliary) = auxiliary {
                            assert!(auxiliary.update(cx, |_, _, _| ()).is_err());
                        }
                        let mut running = Rc::try_unwrap(owner)
                            .ok()
                            .expect("cleanup task released owner")
                            .into_inner()
                            .test_into_process();
                        if with_surface {
                            let request = running.commands.next_exit().await;
                            assert!(running.commands.finish_exit(&request));
                            assert!(!running.commands.exit_requested());
                        }
                        // The failure fixture removed this native window before ownership transfer.
                        drop(running.startup_surface.take());
                        support::dispose_running(running, cx).await;
                        observed.set(true);
                        cx.update(|app| app.quit()).unwrap();
                    })
                    .detach();
                },
                app,
            );
            if with_surface {
                app.spawn(async move |cx| {
                    surface(cx)
                        .await
                        .update(cx, |surface, _, cx| {
                            assert!(surface.request_retry(cx).is_some());
                        })
                        .unwrap();
                })
                .detach();
            }
            support::watchdog(app);
        });
    assert!(finished.get());
    assert_reopens(&directory);
}
