use super::*;
use crate::{
    startup_owner::{self, StartupCompletion, StartupConfiguration, StartupHomeOpen},
    startup_surface::StartupSurface,
};
use gpui::{Application, AsyncApp, WindowHandle};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

fn input(
    path: &std::path::Path,
    open: impl Fn(&std::path::Path, CommandCancellation) -> StartupHomeOpen + Send + Sync + 'static,
) -> StartupConfiguration {
    StartupConfiguration {
        home: path.to_owned(),
        open: Arc::new(open),
        services: configuration(),
        windows: window_services::inputs(),
        enrollment_slots: NonZeroUsize::new(1).unwrap(),
        settlement_slots: NonZeroUsize::new(1).unwrap(),
        initial_window: beryl_model::WindowId::from_bytes([230; 16]),
        initial_placement: window_services::placement(),
        native_hook: None,
        service_hook: None,
    }
}

mod support {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/startup_owner_support.rs"
    ));
}

mod running_owner {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_owner.rs"
    ));
}

mod running_confirmation {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_confirmation.rs"
    ));
}

mod running_idle_shutdown {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_idle_shutdown.rs"
    ));
}

mod running_shutdown_progress {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_shutdown_progress.rs"
    ));
}

mod running_initial_observation {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_initial_observation.rs"
    ));
}

#[test]
fn native_success_hands_off_the_complete_graph_and_late_exit_with_auxiliary_custody() {
    let directory = support::native_home();
    let opened = Arc::new(AtomicUsize::new(0));
    let count = opened.clone();
    let input = input(directory.path(), move |path, _| {
        if count.fetch_add(1, Ordering::SeqCst) == 0 {
            StartupHomeOpen::Failed {
                detail: "first open failed".to_owned(),
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
                    let StartupCompletion::Running(mut running) = result else {
                        panic!("expected complete native startup")
                    };
                    assert_eq!(running.windows.window_ids().len(), 1);
                    assert!(running.services.graph().is_some());
                    assert!(!running.commands.exit_requested());
                    let mut surface = running
                        .startup_surface
                        .take()
                        .expect("same Retry surface transfers with success");
                    surface
                        .window
                        .update(app, |surface, _, cx| surface.request_exit(cx))
                        .unwrap();
                    app.spawn(async move |cx| {
                        surface.close(cx).await.unwrap();
                        cx.background_executor()
                            .timer(Duration::from_millis(10))
                            .await;
                        assert!(
                            running.commands.exit_requested(),
                            "deferred Exit survives native surface removal"
                        );
                        let request = running.commands.next_exit().await;
                        assert!(running.commands.finish_exit(&request));
                        assert!(!running.commands.exit_requested());
                        support::dispose_running(running, cx).await;
                        observed.set(true);
                        cx.update(|app| app.quit()).unwrap();
                    })
                    .detach();
                },
                app,
            );
            app.spawn(async move |cx| {
                let window = surface(cx).await;
                window
                    .update(cx, |surface, _, cx| {
                        assert!(surface.request_retry(cx).is_some())
                    })
                    .unwrap();
            })
            .detach();
            support::watchdog(app);
        });
    assert!(finished.get());
    assert_eq!(opened.load(Ordering::SeqCst), 2);
    assert_reopens(&directory);
}

async fn surface(cx: &mut AsyncApp) -> WindowHandle<StartupSurface> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let found = cx
            .update(|app| {
                app.windows()
                    .into_iter()
                    .find_map(|window| window.downcast::<StartupSurface>())
            })
            .unwrap();
        if let Some(window) = found {
            return window;
        }
        assert!(Instant::now() < deadline, "startup surface deadline");
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

#[test]
fn retry_is_serialized_for_the_same_home_and_exit_joins_opening() {
    let directory = tempfile::tempdir().unwrap();
    let expected = directory.path().to_owned();
    let calls = Arc::new(AtomicUsize::new(0));
    let opening_calls = calls.clone();
    let (entered, entered_rx) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let release_rx = std::sync::Mutex::new(release_rx);
    let input = input(directory.path(), move |path, cancellation| {
        assert_eq!(path, expected);
        let call = opening_calls.fetch_add(1, Ordering::SeqCst);
        if call == 1 {
            entered.send(()).unwrap();
            release_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
            assert!(cancellation.is_cancelled());
        }
        StartupHomeOpen::Failed {
            detail: "synthetic open failure".to_owned(),
            retained: None,
        }
    });
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            let completed = observed.clone();
            let commands = startup_owner::start(
                input,
                move |result, app| {
                    assert!(matches!(
                        result,
                        StartupCompletion::Exit { unsuccessful: true }
                    ));
                    assert!(
                        app.windows().is_empty(),
                        "native surface disposal precedes quit"
                    );
                    completed.set(true);
                    app.quit();
                },
                app,
            );
            app.spawn(async move |cx| {
                let window = surface(cx).await;
                window
                    .update(cx, |surface, _, cx| {
                        assert!(surface.request_retry(cx).is_some());
                        assert!(surface.request_retry(cx).is_none());
                    })
                    .unwrap();
                cx.background_executor()
                    .spawn(async move { entered_rx.recv_timeout(Duration::from_secs(10)).unwrap() })
                    .await;
                commands.request_exit();
                cx.background_executor()
                    .timer(Duration::from_millis(30))
                    .await;
                assert!(!observed.get(), "Exit waits for the owned open operation");
                assert_eq!(calls.load(Ordering::SeqCst), 2);
                release.send(()).unwrap();
            })
            .detach();
        });
    assert!(finished.get());
}

#[test]
fn busy_home_has_no_retry_and_exits_after_owned_surface_disposal() {
    let directory = tempfile::tempdir().unwrap();
    let input = input(directory.path(), |_, _| StartupHomeOpen::Busy);
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                input,
                move |result, app| {
                    assert!(matches!(
                        result,
                        StartupCompletion::Exit { unsuccessful: true }
                    ));
                    assert!(app.windows().is_empty());
                    observed.set(true);
                    app.quit();
                },
                app,
            );
            app.spawn(async move |cx| {
                let window = surface(cx).await;
                window
                    .update(cx, |surface, _, cx| {
                        assert!(surface.request_retry(cx).is_none());
                        surface.request_exit(cx);
                    })
                    .unwrap();
            })
            .detach();
        });
    assert!(finished.get());
}

#[test]
fn exit_during_native_validation_waits_for_the_worker_and_exact_window_disposal() {
    let directory = support::native_home();
    let mut input = input(directory.path(), |path, _| support::open(path));
    let (entered, entered_rx) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let release_rx = Arc::new(std::sync::Mutex::new(release_rx));
    input.native_hook = Some(Arc::new(move |native| {
        let entered = entered.clone();
        let release = release_rx.clone();
        native.test_before_final_validation(move || {
            entered.send(()).unwrap();
            release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        });
    }));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            let done = observed.clone();
            let commands = startup_owner::start(
                input,
                move |result, app| {
                    assert!(matches!(
                        result,
                        StartupCompletion::Exit { unsuccessful: true }
                    ));
                    assert!(app.windows().is_empty());
                    done.set(true);
                    app.quit();
                },
                app,
            );
            app.spawn(async move |cx| {
                cx.background_executor()
                    .spawn(async move { entered_rx.recv_timeout(Duration::from_secs(10)).unwrap() })
                    .await;
                commands.request_exit();
                cx.background_executor()
                    .timer(Duration::from_millis(30))
                    .await;
                assert!(!observed.get());
                release.send(()).unwrap();
            })
            .detach();
            support::watchdog(app);
        });
    assert!(finished.get());
    assert_reopens(&directory);
}

#[test]
fn unproven_service_retirement_blocks_retry_and_cannot_report_orderly_exit() {
    let directory = support::native_home();
    let mut input = input(directory.path(), |path, _| support::open(path));
    input.service_hook = Some(Arc::new(|owner| owner.test_fail_shutdown_completion()));
    input.native_hook = Some(Arc::new(|native| {
        native.test_before_publication(|_, _, _| Err("injected publication failure".to_owned()))
    }));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                input,
                |_, _| panic!("blocked cleanup cannot complete startup or ordinary Exit"),
                app,
            );
            app.spawn(async move |cx| {
                let window = surface(cx).await;
                window
                    .update(cx, |surface, _, cx| {
                        assert!(surface.request_retry(cx).is_none());
                        surface.request_exit(cx);
                    })
                    .unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                window
                    .update(cx, |surface, _, cx| {
                        assert!(surface.request_retry(cx).is_none())
                    })
                    .unwrap();
                observed.set(true);
                // Test teardown deliberately stops the executor; production retains this blocked owner.
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
            support::watchdog(app);
        });
    assert!(finished.get());
    assert_reopens(&directory);
}

#[test]
fn native_failure_retries_with_fresh_services_and_the_original_session_member() {
    let directory = support::native_home();
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = attempts.clone();
    let mut input = input(directory.path(), |path, _| support::open(path));
    let id = input.initial_window;
    input.native_hook = Some(Arc::new(move |native| {
        if count.fetch_add(1, Ordering::SeqCst) == 0 {
            native.test_before_publication(|_, _, _| Err("first publication refused".to_owned()));
        }
    }));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            startup_owner::start(
                input,
                move |result, app| {
                    let StartupCompletion::Running(mut running) = result else {
                        panic!("fresh startup expected")
                    };
                    assert_eq!(running.windows.window_ids(), &[id]);
                    app.spawn(async move |cx| {
                        running
                            .startup_surface
                            .as_mut()
                            .unwrap()
                            .close(cx)
                            .await
                            .unwrap();
                        running.startup_surface = None;
                        support::dispose_running(running, cx).await;
                        observed.set(true);
                        cx.update(|app| app.quit()).unwrap();
                    })
                    .detach();
                },
                app,
            );
            app.spawn(async move |cx| {
                let window = surface(cx).await;
                window
                    .update(cx, |surface, _, cx| {
                        assert!(surface.request_retry(cx).is_some())
                    })
                    .unwrap();
            })
            .detach();
            support::watchdog(app);
        });
    assert!(finished.get());
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_reopens(&directory);
}
