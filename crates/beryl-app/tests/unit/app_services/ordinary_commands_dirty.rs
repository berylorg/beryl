use super::*;

#[test]
fn native_mounted_dirty_flush_failure_recovers_exact_resident_before_fresh_close() {
    run_dirty_failure(false);
}

#[test]
fn native_mounted_toolbar_exit_dirty_flush_failure_recovers_exact_resident_before_fresh_close() {
    run_dirty_failure(true);
}

fn run_dirty_failure(toolbar_exit: bool) {
    run_mounted_with_faults(
        2,
        0,
        move |owner, faults, cx| {
            Box::pin(async move {
                let invoking = windows(&owner)[0];
                let native = native(invoking, cx).await;
                let resident = composer(invoking, cx).await;
                let entity = resident.entity_id();
                let original = snapshot(&owner);
                let thread = original.windows()[0].selected_thread().unwrap().thread_id();
                let generation = owner.borrow().test_ordinary_command_status().3;
                let text = "ordinary dirty resident";
                let ready_deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let ready = cx
                        .update(|app| {
                            let composer = resident.read(app);
                            let input = composer.gpui_input();
                            let input = input.read(app);
                            input.is_enabled()
                                && input.is_quiescent()
                                && input.is_surface_current_and_interactive()
                        })
                        .unwrap();
                    if ready {
                        break;
                    }
                    assert!(
                        Instant::now() < ready_deadline,
                        "dirty editor did not become interactive"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                invoking
                    .update(cx, |_, window, app| {
                        resident.read(app).gpui_input().update(app, |input, cx| {
                            input.focus(window);
                            assert!(
                                input.surface().unwrap().platform_selection().is_some(),
                                "dirty edit has no admitted platform selection"
                            );
                            input.replace_and_mark_text_in_range(None, text, None, window, cx);
                        });
                    })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let ready = cx
                        .update(|app| {
                            let composer = resident.read(app);
                            assert!(
                                composer.last_error().is_none(),
                                "dirty edit failed: {:?}",
                                composer.last_error()
                            );
                            composer
                                .selection_identity()
                                .binding()
                                .logical_extent()
                                .logical_utf8_bytes()
                                == text.len() as u64
                                && composer.gpui_input().read(app).is_quiescent()
                        })
                        .unwrap();
                    if ready {
                        break;
                    }
                    if Instant::now() >= deadline {
                        let state = cx
                            .update(|app| {
                                let composer = resident.read(app);
                                let input = composer.gpui_input();
                                let input = input.read(app);
                                (
                                    composer.selection_identity().binding().logical_extent(),
                                    composer.last_error().map(str::to_owned),
                                    composer
                                        .last_mutation_admission_failure()
                                        .map(|failure| format!("{failure:?}")),
                                    composer.mutation_feedback(),
                                    composer.test_has_active_flight(),
                                    composer.test_widget_released(),
                                    composer.test_has_pending_realizer(),
                                    input.is_quiescent(),
                                    input.is_enabled(),
                                    input.is_surface_current_and_interactive(),
                                    composer.realization_diagnostics(app),
                                )
                            })
                            .unwrap();
                        panic!("dirty edit did not settle: {state:?}");
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                assert_eq!(copy_all(invoking, &resident, cx).await, text);
                let before = cx
                    .update(|app| {
                        resident
                            .read(app)
                            .gpui_input()
                            .read(app)
                            .surface()
                            .unwrap()
                            .selection()
                    })
                    .unwrap();
                let dirty_binding = cx
                    .update(|app| resident.read(app).selection_identity().binding())
                    .unwrap();
                let dirty_history = capture_resident_history(&owner, thread, dirty_binding);
                {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    let current = graph
                        .syndic()
                        .current_draft_piece_text_demand(
                            graph.home(),
                            thread,
                            syndic_storage::DraftPieceTextDemandV1::Forward(0),
                            4096,
                        )
                        .unwrap()
                        .unwrap();
                    assert!(
                        !graph
                            .syndic()
                            .draft_editor_candidate_is_saved(
                                graph.home(),
                                dirty_binding.candidate(),
                                current.selector()
                            )
                            .unwrap()
                    );
                }
                let removal_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let removed = removal_started.clone();
                owner
                    .borrow_mut()
                    .test_before_next_ordinary_session_removal(move |_| {
                        removed.store(true, Ordering::SeqCst)
                    });
                owner
                    .borrow_mut()
                    .test_before_next_ordinary_draft_prepare(move |_| {
                        faults.fail_next(FaultPoint::BeforeCommit)
                    });
                if toolbar_exit {
                    activate_exit(invoking, cx);
                } else {
                    post_close(native);
                }
                wait_until(
                    cx,
                    || {
                        let status = owner.borrow().test_ordinary_command_status();
                        status.3 != generation && !status.1 && !status.2
                    },
                    "dirty-flush automatic recovery",
                )
                .await;
                assert!(!removal_started.load(Ordering::SeqCst));
                assert!(unsafe { IsWindow(Some(native)).as_bool() });
                assert_eq!(composer(invoking, cx).await.entity_id(), entity);
                assert_eq!(
                    cx.update(|app| resident
                        .read(app)
                        .gpui_input()
                        .read(app)
                        .surface()
                        .unwrap()
                        .selection())
                        .unwrap(),
                    before
                );
                assert_eq!(
                    snapshot(&owner)
                        .windows()
                        .iter()
                        .map(|record| record.window_id())
                        .collect::<Vec<_>>(),
                    original
                        .windows()
                        .iter()
                        .map(|record| record.window_id())
                        .collect::<Vec<_>>()
                );
                assert_eq!(copy_all(invoking, &resident, cx).await, text);
                let binding = cx
                    .update(|app| resident.read(app).selection_identity().binding())
                    .unwrap();
                assert_eq!(binding.root(), dirty_binding.root());
                let history = binding.history();
                let previous = dirty_binding.history();
                assert_eq!(history.root(), previous.root());
                assert_eq!(
                    history.candidate_generation(),
                    previous.candidate_generation()
                );
                assert_eq!(history.frontier_revision(), previous.frontier_revision());
                assert_eq!(history.byte_budget(), previous.byte_budget());
                assert_eq!(
                    history.retention_policy_revision(),
                    previous.retention_policy_revision()
                );
                assert_eq!(history.availability(), previous.availability());
                let saved_history = capture_resident_history(&owner, thread, binding);
                assert_eq!(saved_history.journal_head(), dirty_history.journal_head());
                assert_eq!(saved_history.undo_head(), dirty_history.undo_head());
                assert_eq!(saved_history.redo_head(), dirty_history.redo_head());
                assert_eq!(
                    saved_history.oldest_eligible(),
                    dirty_history.oldest_eligible()
                );
                assert_eq!(
                    saved_history.cumulative_encoded_bytes(),
                    dirty_history.cumulative_encoded_bytes()
                );
                assert_eq!(
                    saved_history.retained_encoded_bytes(),
                    dirty_history.retained_encoded_bytes()
                );
                {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    let current = graph
                        .syndic()
                        .current_draft_piece_text_demand(
                            graph.home(),
                            thread,
                            syndic_storage::DraftPieceTextDemandV1::Forward(0),
                            4096,
                        )
                        .unwrap()
                        .unwrap();
                    assert_eq!(current.value().bytes(), text.as_bytes());
                    assert_eq!(binding.history(), current.selector().history());
                    assert!(
                        graph
                            .syndic()
                            .draft_editor_candidate_is_saved(
                                graph.home(),
                                binding.candidate(),
                                current.selector()
                            )
                            .unwrap()
                    );
                }
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                assert!(!owner.borrow().exit_requested());
                post_close(native);
                wait_until(
                    cx,
                    || {
                        owner.borrow().test_ordinary_command_status().0 == 1
                            && !owner.borrow().exit_requested()
                    },
                    "fresh dirty-flush close",
                )
                .await;
                assert!(removal_started.load(Ordering::SeqCst));
                assert!(!unsafe { IsWindow(Some(native)).as_bool() });
                activate_exit(windows(&owner)[0], cx);
            })
        },
        true,
        1,
    );
}
