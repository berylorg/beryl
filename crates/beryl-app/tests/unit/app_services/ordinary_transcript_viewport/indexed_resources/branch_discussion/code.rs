use super::*;
use std::{cell::RefCell, rc::Rc};

async fn wait_composer_bytes(
    window: WindowHandle<MainWindowShellRoot>,
    resident: &gpui::Entity<MainWindowConversationComposer>,
    bytes: u64,
    cx: &mut AsyncApp,
) {
    wait_until(
        window,
        cx,
        |cx| {
            cx.update(|app| {
                let composer = resident.read(app);
                assert!(
                    composer.last_error().is_none(),
                    "{:?}",
                    composer.last_error()
                );
                composer
                    .selection_identity()
                    .binding()
                    .logical_extent()
                    .logical_utf8_bytes()
                    == bytes
                    && composer.gpui_input().read(app).is_quiescent()
                    && composer.test_edit_position_proof_ready(app)
                    && !composer.test_has_active_flight()
            })
            .unwrap()
        },
        "the accepted Code Quote must settle its exact composer edit",
    )
    .await;
}

#[test]
fn native_captured_branch_code_selection_uses_canonical_range_and_preserves_retry_origin() {
    require_fixture_ledger();
    const CODE_SOURCE: &str = "Intro\n\n```text\nalpha beta\n```\n";
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| fixture::seed(path, CODE_SOURCE, true),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["Intro"]).await;
                wait_until(window, cx, |cx| panel.read_with(cx, |panel, _| panel.code_source_point_for_test(21).is_some() && panel.code_source_point_for_test(25).is_some()).unwrap(), "the short Code selection must be fully realized in canonical source coordinates").await;
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                let resident = composer(window, cx).await;
                wait_composer_bytes(window, &resident, 0, cx).await;
                let (head, summary) = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    (
                        graph
                            .syndic()
                            .transcript_view_head(graph.home(), parent, fixture::limit())
                            .unwrap()
                            .unwrap(),
                        graph
                            .syndic()
                            .history_summary(graph.home(), parent, fixture::limit())
                            .unwrap()
                            .unwrap(),
                    )
                };
                code_delivery_ready(window, &panel, &head, &summary, cx).await;
                with_native(window, cx, |native, app| {
                    drag(
                        native,
                        app,
                        panel.read(app).code_source_point_for_test(21).unwrap(),
                        panel.read(app).code_source_point_for_test(25).unwrap(),
                    );
                    assert_eq!(panel.read(app).code_selection_for_test(), Some(21..25));
                    assert!(
                        panel.read(app).code_discussion_capture_ready_for_test(),
                        "{:?}",
                        panel.read(app).code_discussion_facts_for_test()
                    );
                    let position = panel.read(app).code_source_point_for_test(23).unwrap();
                    super::super::narrative_native::pointer(
                        native,
                        app,
                        gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                            position,
                            button: gpui::MouseButton::Right,
                            click_count: 1,
                            ..Default::default()
                        }),
                    );
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
                    "native right-click must qualify the exact Code source",
                )
                .await;
                with_native(window, cx, |native, app| {
                    assert_eq!(
                        panel.read(app).transcript_menu_rows_for_test()[0].0,
                        "Quote"
                    );
                    key(native, app, "enter");
                })
                .unwrap();
                wait_composer_bytes(window, &resident, 4, cx).await;
                assert_eq!(copy_all(window, &resident, cx).await, "beta");
                with_native(window, cx, |native, app| {
                    drag(
                        native,
                        app,
                        panel.read(app).code_source_point_for_test(21).unwrap(),
                        panel.read(app).code_source_point_for_test(25).unwrap(),
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
                    "Code keyboard menu must use the same qualified capture",
                )
                .await;
                window
                    .update(cx, |root, _, _| {
                        root.test_thread_confirmation_hooks(
                            Some(Box::new(|cancellation| cancellation.cancel())),
                            None,
                            None,
                        )
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
                            .read_with(cx, |root, _| {
                                root.test_captured_branch_state()
                                    .is_some_and(|state| state.0 == "Retry")
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "Code branch must preserve its origin on proven ordinary activation failure",
                )
                .await;
                let (_, created, captured) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                assert_eq!(captured.text.as_str(), "beta");
                assert_eq!(captured.source.range().start(), 21);
                assert_eq!(captured.source.range().end(), 25);
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.code_selection_for_test() == Some(21..25)
                            })
                            .unwrap()
                    },
                    "the same canonical Code selection must survive failed activation",
                )
                .await;
                let copied = Rc::new(RefCell::new(Vec::new()));
                let (head, summary) = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    (
                        graph
                            .syndic()
                            .transcript_view_head(graph.home(), parent, fixture::limit())
                            .unwrap()
                            .unwrap(),
                        graph
                            .syndic()
                            .history_summary(graph.home(), parent, fixture::limit())
                            .unwrap()
                            .unwrap(),
                    )
                };
                code_delivery_ready(window, &panel, &head, &summary, cx).await;
                let sink = copied.clone();
                panel
                    .update(cx, |panel, _| {
                        panel.code_set_clipboard_writer_for_test(move |item, _, _| {
                            sink.borrow_mut().push(item.clone());
                            Ok(())
                        })
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    key(native, app, "escape");
                    key(native, app, "ctrl-c");
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |_| !copied.borrow().is_empty(),
                    "Code focus and selection must still reach the real checked Copy path",
                )
                .await;
                assert_eq!(copied.borrow().len(), 1);
                assert_eq!(copied.borrow()[0].text().as_deref(), Some("beta"));
                with_native(window, cx, |native, app| {
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
                    "Code origin must expose same-discussion Retry",
                )
                .await;
                window
                    .update(cx, |root, _, _| {
                        root.test_captured_branch_hooks(
                            Some(Box::new(|_| {
                                panic!("Code Retry must not create a second discussion")
                            })),
                            None,
                        )
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
                                root.coherent_viewed_thread(app) == Some(created)
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "Code Retry must activate the same exact created discussion",
                )
                .await;
                {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    let child = graph
                        .syndic()
                        .thread(graph.home(), created, fixture::limit())
                        .unwrap()
                        .unwrap();
                    let context = graph
                        .syndic()
                        .context_envelope(
                            graph.home(),
                            child.context_owner_id().unwrap(),
                            fixture::limit(),
                        )
                        .unwrap()
                        .unwrap();
                    assert_eq!(context.envelope().descriptor().source(), captured.source);
                    assert_eq!(context.envelope().text(), &captured.text);
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
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
