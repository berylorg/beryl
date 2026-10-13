use super::*;
use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::BerylState;
use std::sync::Mutex;
use syndic_storage::SyndicStorage;

fn open(path: &Path, faults: FaultController) -> HomeOpenOutcome {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
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

fn run_pending_activation_exit(eof: bool) {
    let (path, first, target, _) = fixture::history_home();
    let HomeOpenOutcome::Ready {
        candidate,
        state: storage_state,
        syndic,
    } = fixture::open(&path)
    else {
        unreachable!()
    };
    let home = candidate.publish().unwrap();
    native_appearance::install_native_theme(&home, &storage_state);
    drop((storage_state, syndic));
    home.close().unwrap();
    let faults = FaultController::new();
    let opener_faults = faults.clone();
    let configuration = Configuration::new(
        path.clone(),
        Arc::new(move |path, _| open(path, opener_faults.clone())),
        true,
    )
    .unwrap();
    let (endpoint, client) = DiagnosticTargetServer::test_stdio_endpoint();
    let joined = client.clone();
    let unsuccessful = Rc::new(Cell::new(false));
    let startup_outcome = unsuccessful.clone();
    let outcome = Arc::new(Mutex::new((0usize, None)));
    let observed_outcome = outcome.clone();
    let exit_handoffs = Rc::new(Cell::new(0));
    let observed_exit_handoffs = exit_handoffs.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            app.set_global(crate::bootstrap::diagnostic::DiagnosticExitObservation(observed_exit_handoffs.clone()));
            launch(configuration.startup, startup_outcome, Some(endpoint), app);
            app.spawn(async move |cx| {
                let hello = send(&client, json!({"id":"hello","command":"handshake"}), cx).await;
                assert!(hello["error"].is_null());
                wait_state(
                    &client,
                    cx,
                    |state| {
                        state["stage"] == "running"
                            && state["transcript"]["publishedViewId"] == first.to_string()
                    },
                    "initial publication",
                )
                .await;
                let held = Arc::new(Mutex::new(None));
                let held_hook = held.clone();
                let watched_outcome = observed_outcome.clone();
                let observed_outcome = observed_outcome.clone();
                let window = cx.update(|app| {
                    let owner = app.global::<BootstrapLifetime>().running.clone().unwrap();
                    let window = owner.borrow().diagnostic_main_window(app).unwrap();
                    window
                        .update(app, |root, _, _| {
                            root.test_thread_confirmation_hooks(
                                Some(Box::new(move |_| {
                                    *held_hook.lock().unwrap() =
                                        Some(faults.block_next(FaultPoint::BeforeCommit));
                                })),
                                None,
                                None,
                            );
                            root.test_original_ordinary_claim_outcome_hook(Box::new(move |outcome| {
                                use crate::main_window::running_threads::activation::RunningThreadActivationOutcome;
                                let class = match outcome {
                                    RunningThreadActivationOutcome::Settled(commit) => {
                                        assert_eq!(commit.selection.thread_id(), target);
                                        "committed"
                                    },
                                    RunningThreadActivationOutcome::NotCommitted(_) => "not_committed",
                                    RunningThreadActivationOutcome::Pending(_) => "pending",
                                };
                                let mut recorded = observed_outcome.lock().unwrap();
                                recorded.0 += 1;
                                recorded.1 = Some(class);
                            }));
                        })
                        .unwrap();
                    window
                })
                .unwrap();
                let accepted = send(
                    &client,
                    json!({"id":"switch","command":"switch_thread","params":{"threadId":target}}),
                    cx,
                )
                .await;
                assert_eq!(accepted["result"]["status"], "opening", "{accepted}");
                let pending = wait_state(
                    &client,
                    cx,
                    |state| {
                        held.lock().unwrap().as_ref().is_some_and(
                            |block: &beryl_home_store::test_faults::FaultBlock| {
                                block.wait_until_reached(Duration::ZERO)
                            },
                        ) && state["activationPending"] == true
                    },
                    "ordinary commit must be held",
                )
                .await;
                assert!(pending["selectedThreadId"].is_null());
                assert_eq!(pending["transcript"]["publishedViewId"], first.to_string());
                assert_eq!(pending["pendingThreadId"], target.to_string());
                let competing = send(
                    &client,
                    json!({"id":"competing","command":"switch_thread","params":{"threadId":first}}),
                    cx,
                )
                .await;
                assert_eq!(competing["error"]["kind"], "activation_failed");
                assert_eq!(
                    state(&client, cx).await["pendingThreadId"],
                    target.to_string()
                );
                if eof {
                    client.close();
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while !client.terminal() || observed_exit_handoffs.get() == 0
                    {
                        assert!(
                            Instant::now() < deadline,
                            "EOF must enter the ordinary process exit path"
                        );
                        cx.background_executor()
                            .timer(Duration::from_millis(10))
                            .await;
                    }
                    assert_eq!(observed_exit_handoffs.get(), 1);
                    cx.update(|app| assert!(!window.read(app).unwrap().test_exit_command_enabled())).unwrap();
                    held.lock().unwrap().take().unwrap().release();
                    let deadline = Instant::now() + Duration::from_secs(20);
                    loop {
                        let settled = cx.update(|app| {
                            let root = window.read(app).unwrap();
                            root.test_exit_command_enabled()
                                && root.diagnostic_selected_thread(app) == Some(target)
                                && root.diagnostic_transcript_facts(app, 0)["transcript"]["publishedViewId"] == target.to_string()
                        }).unwrap();
                        if settled {
                            break;
                        }
                        assert!(Instant::now() < deadline, "original activation must settle before a fresh ordinary Exit");
                        cx.background_executor().timer(Duration::from_millis(10)).await;
                    }
                    assert_eq!(*watched_outcome.lock().unwrap(), (1, Some("committed")));
                    cx.update(|app| {
                        let owner = app.global::<BootstrapLifetime>().running.clone().unwrap();
                        assert!(owner.borrow().test_diagnostic_exit_idle());
                        window.read(app).unwrap().test_activate_exit_command();
                    }).unwrap();
                    let deadline = Instant::now() + Duration::from_secs(20);
                    loop {
                        cx.background_executor().timer(Duration::from_millis(100)).await;
                        assert!(Instant::now() < deadline, "the explicit fresh ordinary Exit must dispose the child");
                    }
                } else {
                    held.lock().unwrap().take().unwrap().release();
                    let settled = wait_state(
                        &client,
                        cx,
                        |state| {
                            state["activationPending"] == false
                                && state["selectedThreadId"] == target.to_string()
                                && state["transcript"]["publishedViewId"] == target.to_string()
                        },
                        "the original ordinary target must settle after the competing request was refused",
                    )
                    .await;
                    assert!(settled["pendingThreadId"].is_null());
                    assert!(settled["activationFailure"].is_null());
                    client.close();
                }
            })
            .detach();
        });
    joined.join();
    assert!(joined.terminal());
    assert_eq!(exit_handoffs.get(), 1);
    assert!(!unsuccessful.get());
    let HomeOpenOutcome::Ready {
        candidate,
        state,
        syndic,
    } = fixture::open(&path)
    else {
        unreachable!()
    };
    let home = candidate.publish().unwrap();
    let session = state.session().minimal_bootstrap(&home).unwrap().unwrap();
    assert_eq!(
        session.windows()[0].selected_thread().unwrap().thread_id(),
        match *outcome.lock().unwrap() {
            (1, Some("committed")) => target,
            (1, Some("not_committed")) => first,
            recorded => panic!("one ordinary claim must settle exactly: {recorded:?}"),
        }
    );
    drop((session, state, syndic));
    home.close().unwrap();
}

#[test]
fn native_diagnostic_stdio_pending_activation_rejects_supersession_and_settles_exact_target() {
    run_pending_activation_exit(false);
}

#[test]
fn native_diagnostic_stdio_eof_preserves_gated_activation_until_explicit_fresh_exit() {
    run_pending_activation_exit(true);
}
