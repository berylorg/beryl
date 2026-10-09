use super::*;
use crate::running_owner::RecoveryPreparationFailure;

#[test]
fn native_original_ordinary_prior_record_collision_retains_gate_and_refuses_publication() {
    run_mounted_with_prepared_home_retained(2, 0, fixture::prepare_home, |owner, faults, cx| {
        Box::pin(async move {
            let original_windows = windows(&owner);
            let invoking = original_windows[0];
            let prior = composer(invoking, cx).await;
            let original_identity = cx
                .update(|app| prior.read(app).selection_identity())
                .unwrap();
            let (original_input, original_selection) = cx
                .update(|app| {
                    let input = prior.read(app).gpui_input();
                    let selection = input.read(app).surface().unwrap().selection();
                    (input, selection)
                })
                .unwrap();
            let native_handle = native(invoking, cx).await;
            let original = snapshot(&owner);
            let record = original
                .windows()
                .iter()
                .find(|row| row.window_id() == original_identity.window_id())
                .unwrap()
                .clone();
            let target = SyndicThreadId::from_bytes([242; 16]);
            wait_for_unviewed_catalog_source(&owner, target, cx).await;
            let (home, state, storage) = {
                let retained = owner.borrow();
                let graph = retained.test_services().graph().unwrap();
                (
                    graph.home().service_reference(),
                    graph.state().clone(),
                    graph.syndic().clone(),
                )
            };
            let prepare_home = home.clone();
            let prepare_state = state.clone();
            let prepared = cx
                .background_executor()
                .spawn(async move {
                    let RunningThreadActivationPreparation::Prepared(prepared) =
                        RunningThreadActivation::prepare(
                            &prepare_home,
                            &prepare_state,
                            &storage,
                            record.window_id(),
                            record.selected_thread(),
                            record.remembered_target().unwrap(),
                            target,
                        )
                        .unwrap()
                    else {
                        panic!("collision target was not prepared")
                    };
                    Box::new(prepared)
                })
                .await;
            let claim_write = faults.block_next_in_scope(
                FaultPoint::BeforeCommit,
                FaultScope::of::<beryl_state::ReplaceWindowClaim>(),
            );
            let hook_reached = Arc::new(AtomicBool::new(false));
            let hook_entered = Arc::new(AtomicBool::new(false));
            let entered = hook_entered.clone();
            let reached = hook_reached.clone();
            let fault_home = home.clone();
            let hook_faults = faults.clone();
            invoking
                .update(cx, |root, window, app| {
                    root.test_thread_confirmation_hooks(
                        Some(Box::new(move |_| {
                            entered.store(true, Ordering::Release);
                            let original = state
                                .session()
                                .capture_window_removal(&fault_home, original_identity.window_id())
                                .unwrap();
                            assert_eq!(
                                original.window().selected_thread(),
                                Some(original_identity.claim())
                            );
                            let mut command = HomeCommand::new(fault_home.home_revision().unwrap());
                            command
                                .add(
                                    state.session().update_placement(
                                        state.session().revision(&fault_home).unwrap(),
                                        beryl_state::UpdateWindowPlacement::new(
                                            original.header().revision(),
                                            original_identity.window_id(),
                                            original.window().revision(),
                                            beryl_model::WindowPlacement::new(
                                                beryl_model::WindowBounds::new(40, 40, 900, 700)
                                                    .unwrap(),
                                                beryl_model::WindowDisplayState::Normal,
                                                None,
                                                None,
                                            ),
                                        ),
                                    ),
                                )
                                .unwrap();
                            assert!(matches!(
                                fault_home.execute(command),
                                CommandOutcome::Committed {
                                    later_failure: None,
                                    ..
                                }
                            ));
                            let changed = state
                                .session()
                                .capture_window_removal(&fault_home, original_identity.window_id())
                                .unwrap();
                            assert_eq!(
                                changed.window().selected_thread(),
                                original.window().selected_thread()
                            );
                            assert!(changed.window().revision() > original.window().revision());
                            assert_ne!(changed.window().placement(), original.window().placement());
                            let mut restore = HomeCommand::new(fault_home.home_revision().unwrap());
                            restore
                                .add(state.session().update_placement(
                                    state.session().revision(&fault_home).unwrap(),
                                    beryl_state::UpdateWindowPlacement::new(
                                        changed.header().revision(),
                                        original_identity.window_id(),
                                        changed.window().revision(),
                                        original.window().placement().clone(),
                                    ),
                                ))
                                .unwrap();
                            assert!(matches!(
                                fault_home.execute(restore),
                                CommandOutcome::Committed {
                                    later_failure: None,
                                    ..
                                }
                            ));
                            let restored = state
                                .session()
                                .capture_window_removal(&fault_home, original_identity.window_id())
                                .unwrap();
                            assert_eq!(
                                restored.window().placement(),
                                original.window().placement()
                            );
                            assert_eq!(
                                restored.window().selected_thread(),
                                original.window().selected_thread()
                            );
                            assert_eq!(
                                restored.window().remembered_target(),
                                original.window().remembered_target()
                            );
                            assert_eq!(restored.claim(), original.claim());
                            assert!(restored.window().revision() > changed.window().revision());
                            assert!(changed.header().revision() > original.header().revision());
                            assert!(restored.header().revision() > changed.header().revision());
                            assert_eq!(
                                restored
                                    .header()
                                    .windows()
                                    .iter()
                                    .map(|reference| reference.window_id())
                                    .collect::<Vec<_>>(),
                                original
                                    .header()
                                    .windows()
                                    .iter()
                                    .map(|reference| reference.window_id())
                                    .collect::<Vec<_>>()
                            );
                            assert_eq!(
                                restored
                                    .header()
                                    .windows()
                                    .iter()
                                    .filter(|reference| {
                                        reference.window_id() != original_identity.window_id()
                                    })
                                    .collect::<Vec<_>>(),
                                original
                                    .header()
                                    .windows()
                                    .iter()
                                    .filter(|reference| {
                                        reference.window_id() != original_identity.window_id()
                                    })
                                    .collect::<Vec<_>>()
                            );
                            outcome_evidence::fail_original_confirmed_read(
                                &fault_home,
                                &hook_faults,
                                &reached,
                            );
                        })),
                        None,
                        None,
                    );
                    root.test_begin_original_ordinary_selection(*prepared, window, app)
                        .unwrap();
                })
                .unwrap();
            drop(home);
            let deadline = std::time::Instant::now() + Duration::from_secs(12);
            loop {
                let settled = {
                    let retained = owner.borrow();
                    assert!(!matches!(
                        retained.automatic_recovery_outcome().as_deref(),
                        Some(
                            InterruptedExitRecoveryOutcome::Completed
                                | InterruptedExitRecoveryOutcome::Cancelled
                        )
                    ));
                    let failure = retained.automatic_recovery_failure();
                    let collision = match failure.as_deref().and_then(Option::as_ref) {
                        Some(RecoveryPreparationFailure::Candidate(error)) => {
                            assert_eq!(
                                error.to_string(),
                                "activation audit does not equal the exact joined target"
                            );
                            true
                        }
                        None => false,
                        Some(_) => panic!("original activation refused outside its claim audit"),
                    };
                    collision
                        && !retained.test_services_on_worker()
                        && retained.test_services().graph().is_none()
                };
                if settled {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    let activation = invoking
                        .read_with(cx, |root, _| {
                            (
                                root.test_thread_confirmation_diagnostics(),
                                root.test_running_activation_failure(),
                            )
                        })
                        .unwrap();
                    panic!(
                        "original ordinary source collision did not settle: hook_entered={} hook_reached={} activation={activation:?} recovery_stage={}",
                        hook_entered.load(Ordering::Acquire),
                        hook_reached.load(Ordering::Acquire),
                        owner.borrow().test_thread_creation_recovery_stage()
                    );
                }
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
            }
            assert!(hook_reached.load(Ordering::Acquire));
            assert!(!claim_write.wait_until_reached(Duration::from_millis(1)));
            claim_write.release();
            assert!(!owner.borrow().test_services_on_worker());
            assert!(owner.borrow().test_services().graph().is_none());
            assert!(owner.borrow().test_services().running_selection_pending());
            assert!(
                owner
                    .borrow()
                    .test_services()
                    .test_recovery_process_is_fenced()
            );
            assert_eq!(windows(&owner), original_windows);
            let request = owner
                .borrow()
                .test_running_home_recovery_identity()
                .unwrap();
            invoking
                .update(cx, |root, _, app| {
                    assert!(!root.test_exit_command_enabled());
                    let mount = root.controller().unwrap().composer_mount().unwrap();
                    assert_eq!(mount.read(app).contribution(), Some(prior.clone()));
                    let resident = prior.read(app);
                    assert_eq!(resident.gpui_input(), original_input);
                    let retained = resident.selection_identity();
                    assert_eq!(retained.window_id(), original_identity.window_id());
                    assert_eq!(retained.claim(), original_identity.claim());
                    assert_eq!(
                        retained.binding().home_id(),
                        original_identity.binding().home_id()
                    );
                    let retained = retained.binding().candidate();
                    let original = original_identity.binding().candidate();
                    assert_eq!(retained.draft_id(), original.draft_id());
                    assert_eq!(retained.session_id(), original.session_id());
                    assert_eq!(
                        retained.candidate_generation(),
                        original.candidate_generation()
                    );
                    assert_eq!(retained.root(), original.root());
                    assert_eq!(retained.logical_extent(), original.logical_extent());
                    assert_eq!(
                        original_input.read(app).surface().unwrap().selection(),
                        original_selection
                    );
                    assert!(
                        root.test_running_transcript_snapshot(app)
                            .records
                            .is_empty()
                    );
                })
                .unwrap();
            assert!(unsafe {
                PostMessageW(Some(native_handle), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok()
            });
            cx.background_executor()
                .timer(Duration::from_millis(20))
                .await;
            assert!(Rc::ptr_eq(
                &request,
                &owner
                    .borrow()
                    .test_running_home_recovery_identity()
                    .unwrap()
            ));
            assert!(!owner.borrow().exit_requested());
            assert!(owner.borrow().shutdown_status().is_none());
            assert_eq!(native(invoking, cx).await, native_handle);
            assert!(unsafe { IsWindow(Some(native_handle)).as_bool() });
            owner.borrow().cancel_automatic_recovery();
            wait_until(
                cx,
                || {
                    matches!(
                        owner.borrow().automatic_recovery_outcome().as_deref(),
                        Some(InterruptedExitRecoveryOutcome::Cancelled)
                    )
                },
                "original colliding recovery cancellation did not settle",
            )
            .await;
            assert!(!owner.borrow().test_services_on_worker());
            assert!(owner.borrow().test_services().graph().is_none());
            cx.update(|app| app.quit()).unwrap();
        })
    });
}
