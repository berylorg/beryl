use super::*;
use crate::running_owner::{InterruptedExitRecoveryOutcome, RecoveryPreparationFailure};

fn stage(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    Rc<RefCell<RunningProcessOwner>>,
    gpui::WindowHandle<MainWindowShellRoot>,
    FaultController,
    Arc<crate::app_services::runtime_setup::RuntimeSetupFlight>,
    beryl_state::SessionWindowRecord,
) {
    let (directory, owner, window, faults) = mounted(cx);
    let _picker = open(window, cx);
    let process = owner.borrow_mut().test_take_services();
    let (process, flight, record) = std::thread::spawn(move || {
        let (_, _, record) = commit_onboarding(&process, None);
        let flight = process.graph().unwrap().runtime_setup().test_first_flight();
        (process, flight, record)
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    window
        .update(cx, |root, window, cx| {
            root.test_attach_runtime_setup_flight(flight.clone(), window, cx)
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| root.test_runtime_setup_state().2)
                .unwrap()
        },
        "original first mount was not staged",
    );
    (directory, owner, window, faults, flight, record)
}

#[gpui::test]
fn failed_home_waits_staged_widget_and_worker_before_original_first_capture(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, faults, flight, record) = stage(cx);
    let original_window = window.window_id();
    let retained = window
        .read_with(cx, |root, app| {
            root.test_hold_runtime_setup_mount_worker(app)
        })
        .unwrap();
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        recovery_support::fail(&process, &faults);
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    cx.update(|app| RunningProcessOwner::test_observe_running_home_failure(&owner, app));
    cx.run_until_parked();
    assert!(
        owner
            .borrow()
            .test_running_home_recovery_identity()
            .is_none()
    );
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_state().2)
            .unwrap()
    );
    assert!(flight.cancellation().is_cancelled());
    retained();
    wait(
        cx,
        |cx| {
            cx.update(|app| RunningProcessOwner::test_observe_running_home_failure(&owner, app));
            match owner.borrow().automatic_recovery_outcome().as_deref() {
                Some(InterruptedExitRecoveryOutcome::Completed) => true,
                Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => panic!(
                    "staged first recovery unavailable: {error}; {}",
                    recovery_stage(&owner)
                ),
                Some(InterruptedExitRecoveryOutcome::Cancelled) => {
                    panic!("original staged recovery cancelled")
                }
                _ => false,
            }
        },
        "original staged first capture did not recover after exact resource release",
    );
    assert_eq!(window.window_id(), original_window);
    assert_eq!(cx.windows().len(), 1);
    assert!(
        window
            .read_with(cx, |root, app| {
                !root.test_runtime_setup_state().2
                    && !root.controller().unwrap().is_threadless()
                    && root.test_first_conversation_transcript_claim(
                        record.selected_thread().unwrap(),
                        app,
                    )
            })
            .unwrap()
    );
    dispose_recovered(owner, window, cx);
    assert_reopens(&directory);
}

#[gpui::test]
fn staged_first_cancellation_retires_real_widget_before_window_disposal(cx: &mut TestAppContext) {
    let (directory, owner, window, _, flight, _) = stage(cx);
    let retained = window
        .read_with(cx, |root, app| {
            root.test_hold_runtime_setup_mount_worker(app)
        })
        .unwrap();
    window
        .update(cx, |root, window, cx| {
            root.retire_notices(window, cx);
            assert!(!root.retire_setup_first_mount(window, cx).unwrap());
        })
        .unwrap();
    cx.run_until_parked();
    assert!(flight.cancellation().is_cancelled());
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_state().2)
            .unwrap()
    );
    retained();
    wait(
        cx,
        |cx| {
            window
                .update(cx, |root, window, cx| {
                    root.retire_setup_first_mount(window, cx).unwrap()
                })
                .unwrap()
        },
        "cancelled original widget and worker did not release",
    );
    assert!(
        !window
            .read_with(cx, |root, _| root.test_runtime_setup_state().2)
            .unwrap()
    );
    close_empty(owner, window, cx);
    assert_reopens(&directory);
}

#[gpui::test]
fn terminal_original_reconciliation_disables_mutation_and_keeps_first_custody(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, faults) = mounted(cx);
    let picker = open(window, cx);
    let mut process = owner.borrow_mut().test_take_services();
    let original_faults = faults.clone();
    let (process, flight) = std::thread::spawn(move || {
        process
            .graph_mut()
            .unwrap()
            .handoff
            .as_mut()
            .unwrap()
            .shutdown()
            .unwrap();
        let graph = process.graph().unwrap();
        original_faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let (_, _, record) = commit_onboarding(&process, None);
        let home = graph.home();
        let state = graph.state();
        let snapshot = state.session().minimal_bootstrap(home).unwrap().unwrap();
        let mut changed = HomeCommand::new(home.home_revision().unwrap());
        changed
            .add(state.session().update_placement(
                state.session().revision(home).unwrap(),
                beryl_state::UpdateWindowPlacement::new(
                    snapshot.header().revision(),
                    record.window_id(),
                    record.revision(),
                    beryl_model::WindowPlacement::new(
                        beryl_model::WindowBounds::new(20, 20, 840, 620).unwrap(),
                        beryl_model::WindowDisplayState::Normal,
                        None,
                        None,
                    ),
                ),
            ))
            .unwrap();
        assert!(matches!(
            home.execute(changed),
            beryl_home_store::CommandOutcome::Committed { .. }
        ));
        let flight = graph.runtime_setup().test_first_flight();
        (process, flight)
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    window
        .update(cx, |root, window, cx| {
            root.test_attach_runtime_setup_flight(flight.clone(), window, cx)
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| root.test_runtime_setup_state().1)
                .unwrap()
        },
        "original reconciliation did not become terminal unavailable",
    );
    assert!(cx.update(|app| {
        !picker
            .read(app)
            .command_state(&PickerCommand::AddRuntime)
            .unwrap()
            .can_dispatch()
    }));
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(PickerCommand::AddRuntime, cx)
    });
    assert!(!cx.has_pending_prompt());
    assert!(owner.borrow().test_services().running_selection_pending());
    let outcome = flight
        .take_reconciliation_outcome()
        .expect("retained original terminal reconciliation");
    assert!(matches!(
        &outcome,
        crate::runtime_admission::AdmissionReconciliationOutcome::Unavailable { .. }
    ));
    flight.retain_reconciliation_outcome(outcome);
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        recovery_support::fail(&process, &faults);
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    cx.update(|app| RunningProcessOwner::test_observe_running_home_failure(&owner, app));
    let mut last_diagnostic = String::new();
    let mut diagnostics = 0;
    wait(
        cx,
        |cx| {
            cx.update(|app| RunningProcessOwner::test_observe_running_home_failure(&owner, app));
            let (outcome, failure, retained_terminal) = {
                let retained = owner.borrow();
                let outcome = match retained.automatic_recovery_outcome().as_deref() {
                    None => "not started".to_owned(),
                    Some(InterruptedExitRecoveryOutcome::Running) => "running".to_owned(),
                    Some(InterruptedExitRecoveryOutcome::Completed) => "completed".to_owned(),
                    Some(InterruptedExitRecoveryOutcome::Cancelled) => "cancelled".to_owned(),
                    Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => {
                        format!("unavailable: {error}")
                    }
                };
                let failure = retained.automatic_recovery_failure();
                let (failure, retained_terminal) = match failure.as_deref().and_then(Option::as_ref)
                {
                    Some(RecoveryPreparationFailure::Candidate(error)) => {
                        let detail = error.to_string();
                        let terminal = detail.contains(
                            "first conversation admission retains terminal unavailable custody",
                        );
                        (detail, terminal)
                    }
                    Some(RecoveryPreparationFailure::Services(_)) => {
                        ("service preparation failure".to_owned(), false)
                    }
                    Some(RecoveryPreparationFailure::Resume(_)) => {
                        ("resume failure".to_owned(), false)
                    }
                    Some(RecoveryPreparationFailure::ResumeReconciliation(_)) => {
                        ("resume reconciliation failure".to_owned(), false)
                    }
                    None => ("none".to_owned(), false),
                };
                (outcome, failure, retained_terminal)
            };
            let (threadless, retrying) = window
                .read_with(cx, |root, _| {
                    (
                        root.controller().unwrap().is_threadless(),
                        root.test_home_recovery_retrying(),
                    )
                })
                .unwrap();
            let diagnostic = format!(
                "outcome={outcome}, failure={failure}, threadless={threadless}, retrying={retrying}; {}",
                recovery_stage(&owner)
            );
            if diagnostic != last_diagnostic && diagnostics < 8 {
                eprintln!("terminal original recovery: {diagnostic}");
                diagnostics += 1;
                last_diagnostic = diagnostic;
            }
            assert_ne!(
                outcome, "completed",
                "terminal original custody must refuse editor publication"
            );
            retained_terminal && outcome == "running" && threadless && retrying
        },
        "same home recovery did not retain terminal original reconciliation failure while retrying",
    );
    assert!(flight.take_reconciliation_outcome().is_none());
    owner.borrow().cancel_automatic_recovery();
    wait(
        cx,
        |_| {
            matches!(
                owner.borrow().automatic_recovery_outcome().as_deref(),
                Some(InterruptedExitRecoveryOutcome::Cancelled)
            )
        },
        "terminal original custody cancellation did not settle",
    );
    dispose_refused(owner, window, cx);
    assert_reopens(&directory);
}

#[gpui::test]
fn staged_first_selection_lease_refuses_close_and_exit_until_same_window_publication(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _, _, record) = stage(cx);
    let prepared = owner
        .borrow()
        .test_services()
        .prepare_shutdown_observation()
        .unwrap();
    let observation = std::thread::spawn(move || {
        prepared
            .collect(&crate::cas_projection::ProjectionCancellationToken::new())
            .unwrap()
    })
    .join()
    .unwrap();
    assert!(!observation.has_work());
    cx.update(|app| {
        for intent in [
            crate::running_owner::ShutdownIntent::FinalWindowClose,
            crate::running_owner::ShutdownIntent::ApplicationExit,
        ] {
            assert!(
                owner
                    .borrow_mut()
                    .try_begin_idle_shutdown(record.window_id(), intent, &observation, app)
                    .is_err()
            );
            assert!(owner.borrow().test_services().running_selection_pending());
            assert!(owner.borrow().shutdown_status().is_none());
            assert!(
                window
                    .read(app)
                    .unwrap()
                    .controller()
                    .unwrap()
                    .is_threadless()
            );
        }
    });
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, app| {
                    root.controller().unwrap().composer_mount().is_some()
                        && root.test_first_conversation_transcript_claim(
                            record.selected_thread().unwrap(),
                            app,
                        )
                })
                .unwrap()
        },
        "first conversation did not publish after refused lifecycle overlap",
    );
    assert!(!owner.borrow().test_services().running_selection_pending());
    let close = owner
        .borrow()
        .test_services()
        .admit_ordinary_close(&[record.window_id()])
        .expect("ordinary Close admits after coherent first publication");
    drop(close);
    dispose_recovered(owner, window, cx);
    assert_reopens(&directory);
}
