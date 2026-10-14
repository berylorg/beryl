use super::*;

#[test]
fn native_mounted_replacement_cancel_submit_then_branch_retry_context_and_disposal() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained_and_workers_and_deadline(
        1,
        0,
        9,
        Duration::from_secs(120),
        |path| {
            super::image_markers::fixture::seed(path);
            seed_other_thread(path);
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                leading(window, &panel, cx).await;
                ready(window, &panel, cx, &["before", "after"]).await;
                let parent_thread = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                let target = edit(window, &panel, cx).await;
                let resident = composer(window, cx).await;
                replacement_ready(window, &resident, &panel, true, cx).await;
                let input = resident
                    .read_with(cx, |resident, _| resident.gpui_input())
                    .unwrap();
                let before = input
                    .read_with(cx, |input, _| {
                        input
                            .export_restoration(Some(input.history_frontier()))
                            .unwrap()
                    })
                    .unwrap();
                assert_eq!(
                    input
                        .read_with(cx, |input, _| input
                            .surface()
                            .unwrap()
                            .realized_objects()
                            .len())
                        .unwrap(),
                    2
                );
                with_native(window, cx, |native, app| {
                    input.update(app, |input, _| input.focus(native));
                    key(native, app, "escape");
                })
                .unwrap();
                replacement_ready(window, &resident, &panel, false, cx).await;
                assert_eq!(
                    input
                        .read_with(cx, |input, _| input
                            .export_restoration(Some(input.history_frontier()))
                            .unwrap())
                        .unwrap(),
                    before
                );
                clear_editor(window, &resident, cx).await;
                leading(window, &panel, cx).await;
                assert_eq!(edit(window, &panel, cx).await, target);
                replacement_ready(window, &resident, &panel, true, cx).await;
                let identity = resident
                    .read_with(cx, |resident, _| resident.selection_identity())
                    .unwrap();
                let submitted = identity
                    .binding()
                    .candidate()
                    .draft_id()
                    .submitted_turn_id();
                let expected_parent = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    graph
                        .syndic()
                        .turn(
                            graph.home(),
                            target.turn_id,
                            super::branch_discussion::fixture::limit(),
                        )
                        .unwrap()
                        .unwrap()
                        .parent()
                };
                let mount = window
                    .read_with(cx, |root, _| {
                        root.controller().unwrap().composer_mount().unwrap().clone()
                    })
                    .unwrap();
                window
                    .update(cx, |_, native, app| {
                        mount.update(app, |mount, cx| {
                            mount.test_use_storage_only_submission();
                            mount
                                .test_begin_submission_start(identity, native, cx)
                                .unwrap();
                        })
                    })
                    .unwrap();
                wait_until(window, cx, |cx| mount.read_with(cx, |mount, app| {
                assert!(!matches!(mount.submission_status(), crate::main_window::MainWindowConversationComposerSubmissionStatus::Failed | crate::main_window::MainWindowConversationComposerSubmissionStatus::Unavailable | crate::main_window::MainWindowConversationComposerSubmissionStatus::NotCommitted));
                mount.submission_status() == crate::main_window::MainWindowConversationComposerSubmissionStatus::Idle && mount.contribution().is_some_and(|next| next.entity_id() != resident.entity_id() && next.read(app).selection_identity().claim() == identity.claim())
            }).unwrap(), "replacement submission must adopt its exact successor").await;
                {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    let turn = graph
                        .syndic()
                        .turn(
                            graph.home(),
                            submitted,
                            super::branch_discussion::fixture::limit(),
                        )
                        .unwrap()
                        .unwrap();
                    assert_eq!(turn.parent(), expected_parent);
                    assert_eq!(turn.origin_thread_id(), parent_thread);
                    assert_ne!(submitted, target.turn_id);
                    assert_eq!(
                        graph
                            .syndic()
                            .thread(
                                graph.home(),
                                parent_thread,
                                super::branch_discussion::fixture::limit()
                            )
                            .unwrap()
                            .unwrap()
                            .committed_tail(),
                        Some(submitted)
                    );
                }
                activate(window, OTHER_THREAD, cx).await;
                ready(window, &panel, cx, &["branch prefix"]).await;
                let (copied, _) = writer(&panel, cx);
                let untouched_tail = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    graph
                        .syndic()
                        .thread(
                            graph.home(),
                            OTHER_THREAD,
                            super::branch_discussion::fixture::limit(),
                        )
                        .unwrap()
                        .unwrap()
                        .committed_tail()
                };
                window
                    .update(cx, |root, _, _| {
                        root.test_thread_confirmation_hooks(
                            Some(Box::new(|cancellation| cancellation.cancel())),
                            None,
                            None,
                        )
                    })
                    .unwrap();
                super::branch_discussion::invoke(window, &panel, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, _| {
                                root.test_captured_branch_state()
                                    .is_some_and(|state| state.0 == "Retry")
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "ordinary activation failure must retain its exact created child",
                )
                .await;
                let (_, child, capture) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                assert_eq!(capture.text.as_str(), super::branch_discussion::SELECTED);
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(OTHER_THREAD)
                );
                window
                    .update(cx, |root, _, _| {
                        root.test_captured_branch_hooks(
                            Some(Box::new(|_| {
                                panic!("Retry must never create a second child")
                            })),
                            None,
                        )
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    key(native, app, "escape");
                    assert!(panel.read(app).focus_handle(app).is_focused(native));
                    assert!(
                        panel
                            .read(app)
                            .narrative_selection_ranges_for_test()
                            .is_some()
                    );
                    copy_both(
                        native,
                        app,
                        &copied,
                        super::branch_discussion::SELECTED,
                        "café 🌸 words",
                    );
                    key(native, app, "shift-f10");
                    assert!(
                        panel
                            .read(app)
                            .transcript_menu_rows_for_test()
                            .iter()
                            .any(|row| row.0 == "Retry" && row.1)
                    );
                    key(native, app, "end");
                    key(native, app, "enter");
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, app| {
                                root.coherent_viewed_thread(app) == Some(child)
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "Retry must activate the same child",
                )
                .await;
                assert!(
                    window
                        .read_with(cx, |root, _| root.test_captured_branch_state().is_none())
                        .unwrap()
                );
                {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    let thread = graph
                        .syndic()
                        .thread(
                            graph.home(),
                            child,
                            super::branch_discussion::fixture::limit(),
                        )
                        .unwrap()
                        .unwrap();
                    assert_eq!(thread.parent_thread_id(), Some(OTHER_THREAD));
                    assert_eq!(thread.committed_tail(), untouched_tail);
                    let context = graph
                        .syndic()
                        .context_envelope(
                            graph.home(),
                            thread.context_owner_id().unwrap(),
                            super::branch_discussion::fixture::limit(),
                        )
                        .unwrap()
                        .unwrap();
                    assert_eq!(context.envelope().text(), &capture.text);
                    assert_eq!(context.envelope().descriptor().source(), capture.source);
                    assert_eq!(
                        graph
                            .syndic()
                            .thread(
                                graph.home(),
                                OTHER_THREAD,
                                super::branch_discussion::fixture::limit()
                            )
                            .unwrap()
                            .unwrap()
                            .committed_tail(),
                        untouched_tail
                    );
                }
                context_copy(window, &panel, cx).await;
                {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    let head = graph
                        .syndic()
                        .transcript_view_head(
                            graph.home(),
                            child,
                            super::branch_discussion::fixture::limit(),
                        )
                        .unwrap()
                        .unwrap();
                    assert_eq!(
                        head.lifecycle(),
                        syndic_storage::ProjectionLifecycle::Current
                    );
                    assert_eq!(head.entry_count(), 2);
                    assert!(matches!(
                        graph
                            .syndic()
                            .transcript_build(
                                graph.home(),
                                child,
                                head.generation(),
                                super::branch_discussion::fixture::limit()
                            )
                            .unwrap()
                            .unwrap()
                            .phase(),
                        syndic_storage::TranscriptBuildPhase::Complete
                    ));
                    assert!(
                        graph
                            .syndic()
                            .history_summary(
                                graph.home(),
                                child,
                                super::branch_discussion::fixture::limit()
                            )
                            .unwrap()
                            .unwrap()
                            .complete()
                    );
                    assert_eq!(
                        window
                            .read_with(cx, |root, _| root.test_transcript_panel().entity_id())
                            .unwrap(),
                        panel.entity_id()
                    );
                }
                bounded(window, &panel, cx);
                window
                    .update(cx, |root, native, _| {
                        root.test_thread_navigation_focus(false).focus(native)
                    })
                    .unwrap();
                with_native(window, cx, |native, app| key(native, app, "enter")).unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, app| {
                                root.coherent_viewed_thread(app) == Some(OTHER_THREAD)
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "ordinary Back must return from the created discussion",
                )
                .await;
                ready(window, &panel, cx, &["branch prefix"]).await;
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .diagnostic_snapshot()
                            .resident_data
                            .synthetic_context_group_count)
                        .unwrap(),
                    0
                );
                dispose(window, &panel, cx).await;
            })
        },
    );
}
