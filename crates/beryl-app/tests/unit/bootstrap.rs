use super::*;

#[cfg(feature = "test-faults")]
#[path = "bootstrap_creation_context.rs"]
mod creation_context;

#[test]
fn relative_home_is_refused_before_open() {
    let result = Configuration::new(
        PathBuf::from("relative"),
        Arc::new(|_, _| panic!("must not open")),
        false,
    );
    assert!(matches!(result, Err(BootstrapError::RelativeHome)));
}

#[test]
fn home_containing_token_temp_root_is_refused_before_open() {
    let home = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let result = Configuration::new(home, Arc::new(|_, _| panic!("must not open")), false);
    assert!(
        matches!(result, Err(BootstrapError::Inputs(detail)) if detail.contains("must not contain"))
    );
}

#[test]
fn production_configuration_keeps_exact_home_and_lazy_opener() {
    let home = PathBuf::from(r"C:\Beryl\ChosenHome");
    let configuration = Configuration::new(
        home.clone(),
        Arc::new(|_, _| panic!("must remain lazy")),
        true,
    )
    .unwrap();
    assert_eq!(configuration.startup.home, home);
    assert!(configuration.diagnostic_target);
    assert_ne!(configuration.startup.initial_window.as_bytes(), &[0; 16]);
    assert_eq!(configuration.startup.enrollment_slots.get(), 4);
}

#[test]
fn production_activation_uses_fresh_session_and_operation_identities() {
    let thread = beryl_model::SyndicThreadId::from_bytes([3; 16]);
    let (first, first_commit) = inputs::activation(thread).unwrap();
    let (second, second_commit) = inputs::activation(thread).unwrap();
    assert_eq!(first.thread_id(), thread);
    assert_ne!(first.session_id(), second.session_id());
    assert_ne!(first.operation_id(), second.operation_id());
    assert_ne!(first_commit, second_commit);
    assert_eq!(first.first_demands().len(), 2);
}

#[cfg(feature = "test-faults")]
mod native {
    mod native_appearance {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/native_shell_appearance.rs"
        ));
    }
    use super::*;
    use beryl_home_store::{HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion};
    use beryl_state::BerylState;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };
    use syndic_storage::SyndicStorage;

    #[derive(Clone, Copy)]
    enum Initial {
        Ready,
        Busy,
        Failed,
    }

    fn open(home: &Path) -> HomeOpenOutcome {
        let mut candidate =
            HomeOpenCandidate::open(HomeOpenOptions::new(home, HomeSchemaVersion::CURRENT))
                .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let syndic = SyndicStorage::register(&mut candidate).unwrap();
        let candidate = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap();
        HomeOpenOutcome::Ready {
            candidate,
            state,
            syndic,
        }
    }

    fn run_case(initial: Initial, gated_eof: bool, restored: bool) {
        let (directory, restored_selection) = if restored {
            let (directory, window, thread) = super::creation_context::selected_home();
            (directory, Some((window, thread)))
        } else {
            (tempfile::tempdir().unwrap(), None)
        };
        eprintln!("bootstrap native fixture: {}", directory.path().display());
        let HomeOpenOutcome::Ready {
            candidate,
            state,
            syndic,
        } = open(directory.path())
        else {
            unreachable!()
        };
        let store = candidate.publish().unwrap();
        native_appearance::install_native_theme(&store, &state);
        drop((state, syndic));
        store.close().unwrap();
        let expected = directory.path().to_owned();
        let opened = Arc::new(AtomicUsize::new(0));
        let calls = opened.clone();
        let configuration = Configuration::new(
            expected.clone(),
            Arc::new(move |home, _| {
                assert_eq!(home, expected);
                let call = calls.fetch_add(1, Ordering::SeqCst);
                match (initial, call) {
                    (Initial::Busy, _) => HomeOpenOutcome::Busy,
                    (Initial::Failed, 0) => HomeOpenOutcome::Failed {
                        detail: "fixture open failure".into(),
                        retained: None,
                    },
                    _ => open(home),
                }
            }),
            false,
        )
        .unwrap();
        let unsuccessful = Rc::new(Cell::new(false));
        let result = unsuccessful.clone();
        let (endpoint, shutdown, connected_input) =
            crate::diagnostic_child_target::DiagnosticTargetServer::test_endpoint();
        Application::new().with_quit_on_last_window_close(false).run(move |app| {
            assert!(app.windows().is_empty());
            launch(configuration.startup, result, Some(endpoint), app);
            app.spawn(async move |cx| {
                let deadline = Instant::now() + Duration::from_secs(20);
                let mut retried = false;
                loop {
                    let (running, surface) = cx.update(|app| (
                        app.global::<BootstrapLifetime>().running.is_some(),
                        app.windows().into_iter().find_map(|window| window.downcast::<crate::startup_surface::StartupSurface>()),
                    )).unwrap();
                    if let Some(surface) = surface {
                        if matches!(initial, Initial::Busy) {
                            surface.update(cx, |surface, _, cx| assert!(surface.request_retry(cx).is_none())).unwrap();
                            shutdown.request();
                            break;
                        }
                        if matches!(initial, Initial::Failed) && !retried {
                            surface.update(cx, |surface, _, cx| {
                                assert!(surface.request_retry(cx).is_some());
                                assert!(surface.request_retry(cx).is_none());
                            }).unwrap();
                            retried = true;
                        }
                    }
                    if running && surface.is_none() {
                        cx.update(|app| {
                            let windows = app.windows();
                            assert_eq!(windows.len(), 1);
                            let shell = windows[0].downcast::<crate::main_window::MainWindowShellRoot>().unwrap();
                            let root = shell.read(app).unwrap();
                            if let Some((window, thread)) = restored_selection {
                                assert_eq!(root.controller().unwrap().window_id(), window);
                                assert!(!root.controller().unwrap().is_threadless());
                                assert_eq!(root.diagnostic_selected_thread(app), Some(thread));
                            } else {
                                assert!(root.controller().unwrap().is_threadless());
                                assert!(root.new_window_disabled_reason(app).unwrap().contains("runtime"));
                            }
                            let request = crate::diagnostic_child_protocol::parse_request_frame(
                                br#"{"id":"state","command":"read_ui_state"}"#
                            ).unwrap().unwrap();
                            let response = super::super::diagnostic::respond(&request, app);
                            let value = serde_json::to_value(response).unwrap();
                            assert_eq!(value["result"]["stage"], "running");
                            assert_eq!(value["result"]["mainWindowIds"].as_array().unwrap().len(), 1);
                            assert_eq!(value["result"]["shellState"], if restored { "ready" } else { "backend_unavailable" });
                            assert_eq!(value["result"]["backendAvailability"], if restored { "unavailable" } else { "zero_runtime" });
                            assert!(value["result"]["backgroundWork"]["turnStreamPending"].is_null());
                            if gated_eof { app.global::<BootstrapLifetime>().commands.test_set_exit_gate(crate::startup_owner::RunningExitGate::SettingsReconciliation, true); }
                        }).unwrap();
                        shutdown.request();
                        if gated_eof {
                            cx.background_executor().timer(Duration::from_millis(30)).await;
                            cx.update(|app| {
                                let commands = &app.global::<BootstrapLifetime>().commands;
                                assert!(commands.test_process_exit_pending());
                                assert!(!commands.exit_requested());
                                assert_eq!(app.windows().len(), 1);
                                commands.test_set_exit_gate(crate::startup_owner::RunningExitGate::SettingsReconciliation, false);
                            }).unwrap();
                        }
                        break;
                    }
                    assert!(Instant::now() < deadline, "bootstrap lifecycle deadline");
                    cx.background_executor().timer(Duration::from_millis(5)).await;
                }
            }).detach();
        });
        drop(connected_input);
        assert_eq!(unsuccessful.get(), matches!(initial, Initial::Busy));
        assert_eq!(
            opened.load(Ordering::SeqCst),
            if matches!(initial, Initial::Failed) {
                2
            } else {
                1
            }
        );
        let candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        candidate.close().unwrap();
        directory.close().unwrap();
    }

    #[test]
    fn native_fresh_home_handoff_and_channel_loss_exit() {
        run_case(Initial::Ready, false, false);
    }
    #[test]
    fn native_busy_home_disallows_retry_and_channel_loss_exit() {
        run_case(Initial::Busy, false, false);
    }
    #[test]
    fn native_failed_home_retry_keeps_exact_home_before_running_exit() {
        run_case(Initial::Failed, false, false);
    }
    #[test]
    fn native_running_channel_loss_waits_for_existing_exit_gate() {
        run_case(Initial::Ready, true, false);
    }
    #[test]
    fn native_restored_selected_thread_uses_production_inputs_and_channel_loss_exit() {
        run_case(Initial::Ready, false, true);
    }
}
