#[test]
fn native_partial_preparation_retains_later_member_until_exact_release() {
    let first_home = support::native_home();
    let second_home = support::native_home();
    let first_input = input(first_home.path(), |path, _| support::open(path));
    let second_input = input(second_home.path(), |path, _| support::open(path));
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new().with_quit_on_last_window_close(false).run(move |app| {
        startup_owner::start(first_input, move |first, app| {
            let StartupCompletion::Running(first) = first else { panic!("first startup failed") };
            startup_owner::start(second_input, move |second, app| {
                let StartupCompletion::Running(second) = second else { panic!("second startup failed") };
                let invoking = first.windows.window_ids()[0];
                let first_window = first.windows.shells()[0].window();
                let second_window = second.windows.shells()[0].window();
                let owner = RunningProcessOwner::start(first, app);
                app.spawn(async move |cx| {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    loop {
                        let job = owner.borrow().test_services().prepare_shutdown_observation().unwrap();
                        let observation = cx.background_executor().spawn(async move {
                            job.collect(&ProjectionCancellationToken::new()).unwrap()
                        }).await;
                        if cx.update(|app| owner.borrow_mut().try_begin_idle_shutdown(invoking, ShutdownIntent::ApplicationExit, &observation, app)).unwrap().is_ok() {
                            break;
                        }
                        assert!(Instant::now() < deadline);
                        cx.background_executor().timer(Duration::from_millis(10)).await;
                    }
                    cx.update(|app| RunningProcessOwner::install_shutdown_interaction_gate(&owner, app)).unwrap().unwrap();
                    prepare_work(&owner, cx).await;
                    let cancelled = ProjectionCancellationToken::new();
                    cancelled.cancel();
                    cx.update(|app| {
                        first_window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(false, cx)).unwrap().unwrap();
                        second_window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(true, cx)).unwrap().unwrap();
                        RunningProcessOwner::test_add_shutdown_draft_window(&owner, second_window);
                        let first_error = RunningProcessOwner::advance_shutdown_drafts(&owner, app).unwrap_err();
                        first_window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(true, cx)).unwrap().unwrap();
                        assert_eq!(RunningProcessOwner::advance_shutdown_drafts(&owner, app).unwrap_err(), first_error);
                        second_window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(false, cx)).unwrap().unwrap();
                        assert!(RunningProcessOwner::release_shutdown_drafts(&owner, app).is_err(), "later member must have been prepared despite earlier failure");
                        assert!(RunningProcessOwner::advance_shutdown(&owner, cancelled.clone(), app, |_, _| panic!("partially released set cannot reopen")).is_err());
                        assert!(!owner.borrow().test_services_on_worker());
                        assert_eq!(owner.borrow().shutdown_status(), Some((invoking, ShutdownIntent::ApplicationExit, RunningShutdownStatus::WorkReady)));
                        second_window.update(app, |root, _, cx| root.set_shutdown_interaction_gated(true, cx)).unwrap().unwrap();
                        assert_eq!(RunningProcessOwner::release_shutdown_drafts(&owner, app).unwrap(), RunningShutdownDraftProgress::Released);
                        assert!(RunningProcessOwner::advance_shutdown_drafts(&owner, app).is_err());
                        assert!(RunningProcessOwner::release_shutdown_interaction_gate(&owner, app).is_err());
                    }).unwrap();
                    let deadline = Instant::now() + Duration::from_secs(5);
                    loop {
                        cx.update(|app| RunningProcessOwner::advance_shutdown(&owner, cancelled.clone(), app, |_, _| {})).unwrap().unwrap();
                        settled(&owner, cx).await;
                        match owner.borrow_mut().take_shutdown_progress().unwrap().unwrap() {
                            AppServiceShutdownProgress::Failed { reopened: true, .. } => break,
                            AppServiceShutdownProgress::Failed { reopened: false, .. } => assert!(Instant::now() < deadline),
                            other => panic!("unexpected recovery: {other:?}"),
                        }
                    }
                    assert!(owner.borrow().shutdown_status().is_none());
                    second_window.update(cx, |root, _, cx| root.set_shutdown_interaction_gated(false, cx)).unwrap().unwrap();
                    let first = Rc::try_unwrap(owner).ok().unwrap().into_inner().test_into_process();
                    support::dispose_running(first, cx).await;
                    support::dispose_running(second, cx).await;
                    observed.set(true);
                    cx.update(|app| app.quit()).unwrap();
                }).detach();
            }, app);
        }, app);
        support::watchdog(app);
    });
    assert!(finished.get());
    assert_reopens(&first_home);
    assert_reopens(&second_home);
}
