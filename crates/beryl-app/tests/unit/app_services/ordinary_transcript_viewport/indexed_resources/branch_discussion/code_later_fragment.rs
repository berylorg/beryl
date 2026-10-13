use super::*;

#[test]
fn native_captured_branch_code_later_horizontal_fragment_captures_exact_authenticated_bytes() {
    require_fixture_ledger();
    let source = "Intro\n\n```text\n".to_owned() + &"x".repeat(9000) + "beta tail\n```\n";
    run_mounted_with_prepared_home_retained(
        1,
        0,
        move |path| fixture::seed(path, &source, true),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["Intro"]).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.diagnostic_snapshot().code_panels.first().is_some_and(
                                    |code| {
                                        code.complete_inline_extent.is_some()
                                            && code.layout_state == "ready"
                                    },
                                )
                            })
                            .unwrap()
                    },
                    "the long Code resource must establish its ordinary scroll extent",
                )
                .await;
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let position = panel.read(app).code_viewport_for_test().unwrap().1.center();
                    native.dispatch_input_for_test(
                        gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position,
                            delta: gpui::ScrollDelta::Pixels(gpui::point(
                                gpui::px(-1_000_000.),
                                gpui::px(0.),
                            )),
                            ..Default::default()
                        }),
                        app,
                    );
                })
                .unwrap();
                wait_until(window, cx, |cx| {
                    !window.read_with(cx, |root, _| root.test_pending_resource_read()).unwrap()
                        && panel.read_with(cx, |panel, _| panel.code_commands_current_for_test() && panel.code_source_point_for_test(9015).is_some() && panel.code_source_point_for_test(9019).is_some() && panel.diagnostic_snapshot().code_panels[0].layout_state == "ready").unwrap()
                }, "a later horizontal fragment beyond the initial source page must settle its delivery and become genuinely realized").await;
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
                        panel.read(app).code_source_point_for_test(9015).unwrap(),
                        panel.read(app).code_source_point_for_test(9019).unwrap(),
                    );
                    assert_eq!(panel.read(app).code_selection_for_test(), Some(9015..9019));
                    assert!(
                        panel.read(app).diagnostic_snapshot().code_panels[0].layout_charged_bytes
                            <= 8 * 1024 * 1024
                    );
                    key(native, app, "shift-f10");
                    assert!(
                        panel.read(app).quote_menu_open_for_test(),
                        "later Code menu failed: {:?}; origin={:?}",
                        panel.read(app).code_discussion_facts_for_test(),
                        panel.read(app).code_discussion_origin_for_test(native)
                    );
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                let rows = panel.transcript_menu_rows_for_test();
                                rows.iter()
                                    .any(|row| row.0 == "Discuss in new branch" && row.1)
                            })
                            .unwrap()
                    },
                    "the current later raw fragment must qualify without an offscreen branch read",
                )
                .await;
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
                                root.coherent_viewed_thread(app)
                                    .is_some_and(|thread| thread != parent)
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "the exact later-fragment discussion must complete ordinary activation",
                )
                .await;
                let child = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    let child = graph
                        .syndic()
                        .thread(graph.home(), child, fixture::limit())
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
                    assert_eq!(context.envelope().text().as_str(), "beta");
                    assert_eq!(
                        context.envelope().descriptor().source().range().start(),
                        9015
                    );
                    assert_eq!(context.envelope().descriptor().source().range().end(), 9019);
                }
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_captured_branch_code_oversize_placeholder_has_no_rendered_raw_source_authority() {
    require_fixture_ledger();
    let source = "Intro\n\n```text\na".to_owned() + &"\u{301}".repeat(600) + "\n```\n";
    run_mounted_with_prepared_home_retained(
        1,
        0,
        move |path| fixture::seed(path, &source, true),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["Intro"]).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                !panel.code_source_atoms_for_test().is_empty()
                            })
                            .unwrap()
                    },
                    "the oversize Code source must publish its bounded placeholder",
                )
                .await;
                with_native(window, cx, |native, app| {
                    let (source, bounds) = panel.read(app).code_source_atoms_for_test()[0].clone();
                    drag(
                        native,
                        app,
                        bounds.origin
                            + gpui::point(bounds.size.width * 0.25, bounds.size.height * 0.5),
                        bounds.origin
                            + gpui::point(bounds.size.width * 0.75, bounds.size.height * 0.5),
                    );
                    assert_eq!(panel.read(app).code_selection_for_test(), Some(source));
                    key(native, app, "shift-f10");
                    assert!(!panel.read(app).quote_menu_open_for_test());
                })
                .unwrap();
                assert!(
                    window
                        .read_with(cx, |root, _| root.test_captured_branch_state().is_none())
                        .unwrap()
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_captured_branch_code_separator_after_oversize_line_has_no_raw_line_authority() {
    require_fixture_ledger();
    let source = "Intro\n\n```text\na".to_owned() + &"\u{301}".repeat(600) + "\nbeta\n```\n";
    run_mounted_with_prepared_home_retained(
        1,
        0,
        move |path| fixture::seed(path, &source, true),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["Intro"]).await;
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
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
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                !panel.code_source_atoms_for_test().is_empty()
                                    && panel.code_source_point_for_test(1217).is_some()
                            })
                            .unwrap()
                    },
                    "the placeholder and adjacent raw line must both be genuinely realized",
                )
                .await;
                with_native(window, cx, |native, app| {
                    let (range, bounds) = panel.read(app).code_source_atoms_for_test()[0].clone();
                    assert_eq!(range, 15..1216);
                    drag(
                        native,
                        app,
                        bounds.origin
                            + gpui::point(bounds.size.width * 0.75, bounds.size.height * 0.5),
                        panel.read(app).code_source_point_for_test(1217).unwrap(),
                    );
                    assert_eq!(panel.read(app).code_selection_for_test(), Some(1216..1217));
                    assert!(!panel.read(app).code_discussion_capture_ready_for_test());
                    key(native, app, "shift-f10");
                    assert!(!panel.read(app).quote_menu_open_for_test());
                })
                .unwrap();
                assert!(
                    window
                        .read_with(cx, |root, _| root.test_captured_branch_state().is_none())
                        .unwrap()
                );
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
