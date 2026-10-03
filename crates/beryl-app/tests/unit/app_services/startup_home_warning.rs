use crate::main_window::{MainWindowNoticeWidgetEvent, MainWindowShellRoot, NoticeKind};
use beryl_home_store::HomeDurabilityTier;

fn qualify_successful_open(tier: HomeDurabilityTier, fail_first: bool) {
    let directory = support::native_home();
    let calls = Arc::new(AtomicUsize::new(0));
    let opens = calls.clone();
    let input = input(directory.path(), move |path, _| {
        if opens.fetch_add(1, Ordering::SeqCst) == 0 && fail_first {
            return StartupHomeOpen::Failed {
                detail: "synthetic first open failure".into(),
                retained: None,
            };
        }
        support::open_with_options(
            HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)
                .with_durability_tier_for_tests(tier),
        )
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
                        panic!("successful open must publish main windows")
                    };
                    assert_eq!(running.windows.window_ids().len(), 1);
                    let window = app
                        .windows()
                        .into_iter()
                        .find_map(|window| window.downcast::<MainWindowShellRoot>())
                        .expect("published main window");
                    let is_best_effort = tier == HomeDurabilityTier::BestEffort;
                    window
                        .update(app, |root, window, cx| {
                            assert_eq!(root.notice_projection().is_some(), is_best_effort);
                            assert_eq!(root.test_home_warning_timer().is_some(), is_best_effort);
                            if is_best_effort {
                                let projection = root.notice_projection().unwrap();
                                assert_eq!(projection.kind, NoticeKind::Warning);
                                assert!(
                                    projection
                                        .content
                                        .detail()
                                        .as_str()
                                        .contains("exclusive ownership")
                                );
                                let visible = projection.token.clone();
                                let ingress = root.notice_ingress(window, cx);
                                cx.defer(move |app| {
                                    ingress
                                        .dispatch(
                                            MainWindowNoticeWidgetEvent::Dismiss(visible),
                                            app,
                                        )
                                        .unwrap();
                                });
                            }
                        })
                        .unwrap();
                    let surface = running.startup_surface.take();
                    app.spawn(async move |cx| {
                        if let Some(mut surface) = surface {
                            surface.close(cx).await.unwrap();
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(10))
                            .await;
                        window
                            .update(cx, |root, window, cx| {
                                assert!(root.notice_projection().is_none());
                                root.test_repeat_home_warning_trigger(window, cx);
                                assert!(root.notice_projection().is_none());
                            })
                            .unwrap();
                        support::dispose_running(running, cx).await;
                        observed.set(true);
                        cx.update(|app| app.quit()).unwrap();
                    })
                    .detach();
                },
                app,
            );
            if fail_first {
                app.spawn(async move |cx| {
                    let startup = surface(cx).await;
                    cx.update(|app| {
                        assert!(
                            app.windows()
                                .iter()
                                .all(|window| window.downcast::<MainWindowShellRoot>().is_none()),
                            "unsuccessful opening never publishes a warning shell"
                        );
                    })
                    .unwrap();
                    startup
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
    assert_eq!(calls.load(Ordering::SeqCst), if fail_first { 2 } else { 1 });
    assert_reopens(&directory);
}

#[test]
fn native_successful_best_effort_home_open_publishes_one_warning_after_failed_open_retry() {
    qualify_successful_open(HomeDurabilityTier::BestEffort, true);
}

#[test]
fn native_successful_local_ntfs_home_open_publishes_no_warning() {
    qualify_successful_open(HomeDurabilityTier::Full, false);
}
