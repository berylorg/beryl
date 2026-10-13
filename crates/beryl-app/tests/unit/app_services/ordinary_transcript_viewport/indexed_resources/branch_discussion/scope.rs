use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand};

#[test]
fn native_captured_branch_collision_closes_only_exact_capture_and_preserves_evidence() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| fixture::seed(path, SOURCE, true),
        |owner, faults, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["branch prefix"]).await;
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                let (home, storage) = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    (graph.home().service_reference(), graph.syndic().clone())
                };
                let creation_faults = faults.clone();
                window.update(cx, |root, _, _| root.test_captured_branch_hooks(
                Some(Box::new(move |_| creation_faults.fail_next(FaultPoint::AfterCommitBeforePersist))),
                Some(Box::new(move |created| {
                    let mut batch = syndic_storage::test_faults::FixtureBatch::new();
                    batch.delete(syndic_storage::test_faults::FixtureDelete::DiscussionHandoffGate(created.thread_id)).unwrap();
                    let mut command = HomeCommand::new(home.home_revision().unwrap());
                    command.add(storage.fixture_contribution(storage.revision(&home).unwrap(), batch)).unwrap();
                    assert!(matches!(home.execute(command), CommandOutcome::Committed { later_failure: None, .. }));
                })),
            )).unwrap();
                let (copied, _) = writer(&panel, cx);
                invoke(window, &panel, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, _| {
                                root.test_captured_branch_state()
                                    .is_some_and(|state| state.0 == "Unavailable")
                            })
                            .unwrap()
                    },
                    "creation Collision must publish terminal Unavailable",
                )
                .await;
                let (_, unavailable_child, original) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                with_native(window, cx, |native, app| {
                    assert_eq!(
                        panel.read(app).transcript_menu_rows_for_test()[1].0,
                        "Unavailable"
                    );
                    assert!(!panel.read(app).transcript_menu_rows_for_test()[1].1);
                    key(native, app, "end");
                    key(native, app, "enter");
                    key(native, app, "escape");
                    copy_both(native, app, &copied, SELECTED, "café 🌸 words");
                    let (id, _, _) = record(panel.read(app), "branch prefix");
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &id, 0),
                        point_at(panel.read(app), &id, "branch prefix".len()),
                    );
                    key(native, app, "shift-f10");
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel
                                    .transcript_menu_rows_for_test()
                                    .iter()
                                    .any(|row| row.0 == "Discuss in new branch" && row.1)
                            })
                            .unwrap()
                    },
                    "a distinct healthy source capture must remain branch-eligible after collision",
                )
                .await;
                {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    assert_eq!(
                        graph.home().health().state(),
                        beryl_home_store::HomeHealthState::Healthy
                    );
                }
                with_native(window, cx, |native, app| {
                    key(native, app, "end");
                    key(native, app, "enter");
                })
                .unwrap();
                wait_until(window, cx, |cx| window.read_with(cx, |root, app| {
                let state = root.test_captured_branch_state();
                assert!(!state.as_ref().is_some_and(|state| state.0 == "NotCommitted"), "healthy unrelated creation failed: state={state:?}, error={:?}", root.test_captured_branch_error());
                !root.test_running_thread_activation_pending() && (state.as_ref().is_some_and(|state| state.0 == "Retry") || root.coherent_viewed_thread(app).is_some_and(|child| child != parent && child != unavailable_child))
            }).unwrap(), "unrelated creation must be proven before ordinary activation or same-discussion Retry").await;
                let unrelated_created = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap().1)
                    .unwrap();
                if let Some((stage, created, captured)) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state())
                    .unwrap()
                {
                    assert_eq!(stage, "Retry");
                    assert_ne!(created, unavailable_child);
                    {
                        let owner = owner.borrow();
                        let graph = owner.test_services().graph().unwrap();
                        let thread = graph
                            .syndic()
                            .thread(graph.home(), created, fixture::limit())
                            .unwrap()
                            .unwrap();
                        let context = graph
                            .syndic()
                            .context_envelope(
                                graph.home(),
                                thread.context_owner_id().unwrap(),
                                fixture::limit(),
                            )
                            .unwrap()
                            .unwrap();
                        assert_eq!(context.envelope().text(), &captured.text);
                        assert_eq!(context.envelope().descriptor().source(), captured.source);
                        assert!(
                            graph
                                .state()
                                .catalog()
                                .row(
                                    graph.home(),
                                    created,
                                    beryl_state::CatalogPointReadLimit::schema_maximum()
                                )
                                .unwrap()
                                .is_some()
                        );
                    }
                    window.update(cx, |root, _, _| root.test_captured_branch_hooks(Some(Box::new(|_| panic!("ordinary Retry must not execute a second discussion creation"))), None)).unwrap();
                    with_native(window, cx, |native, app| {
                        key(native, app, "escape");
                        key(native, app, "shift-f10");
                    })
                    .unwrap();
                    wait_until(
                        window,
                        cx,
                        |cx| {
                            panel
                                .read_with(cx, |panel, _| {
                                    panel
                                        .transcript_menu_rows_for_test()
                                        .iter()
                                        .any(|row| row.0 == "Retry" && row.1)
                                })
                                .unwrap()
                        },
                        "the unrelated created discussion must expose its ordinary Retry",
                    )
                    .await;
                    let attempts = window
                        .read_with(cx, |root, _| {
                            root.test_captured_branch_activation_attempts().unwrap()
                        })
                        .unwrap();
                    with_native(window, cx, |native, app| {
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
                                    root.coherent_viewed_thread(app) == Some(parent)
                                        && root.test_captured_branch_activation_attempts() == Some(attempts + 1)
                                        && root.test_captured_branch_state().is_some_and(|state| state.0 == "Retry" && state.1 == created && state.2 == captured)
                                        && !root.test_running_thread_activation_pending()
                                })
                                .unwrap()
                        },
                        "ordinary Retry must retain that same discussion while collision custody forbids coherent election",
                    )
                    .await;
                    with_native(window, cx, |native, app| {
                        key(native, app, "escape");
                        copy_both(native, app, &copied, "branch prefix", "branch prefix");
                    })
                    .unwrap();
                }
                let owner = owner.borrow();
                let graph = owner.test_services().graph().unwrap();
                assert_eq!(graph.home().pending_reconciliations().len(), 1);
                let activated = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                assert_eq!(activated, parent);
                assert!(
                    graph
                        .state()
                        .catalog()
                        .row(
                            graph.home(),
                            unrelated_created,
                            beryl_state::CatalogPointReadLimit::schema_maximum()
                        )
                        .unwrap()
                        .is_some()
                );
                let unavailable = graph
                    .syndic()
                    .thread(graph.home(), unavailable_child, fixture::limit())
                    .unwrap()
                    .unwrap();
                let context = graph
                    .syndic()
                    .context_envelope(
                        graph.home(),
                        unavailable.context_owner_id().unwrap(),
                        fixture::limit(),
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(context.envelope().descriptor().source(), original.source);
                assert_eq!(context.envelope().text(), &original.text);
                drop(owner);
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
