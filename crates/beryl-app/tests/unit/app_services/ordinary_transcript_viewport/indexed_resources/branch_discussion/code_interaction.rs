use super::*;
use beryl_home_store::test_faults::FaultBlock;
use std::sync::{Arc, Mutex};

struct Held(FaultBlock);
impl Drop for Held {
    fn drop(&mut self) {
        self.0.release();
    }
}

async fn ready_code(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &gpui::Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    ready(window, panel, cx, &["Intro"]).await;
    wait_until(
        window,
        cx,
        |cx| {
            panel
                .read_with(cx, |panel, _| {
                    panel.code_source_point_for_test(21).is_some()
                        && panel.code_source_point_for_test(25).is_some()
                        && panel.diagnostic_snapshot().code_panels[0].layout_state == "ready"
                })
                .unwrap()
        },
        "the Code origin must publish exact source geometry",
    )
    .await;
    with_native(window, cx, |native, app| {
        drag(
            native,
            app,
            panel.read(app).code_source_point_for_test(21).unwrap(),
            panel.read(app).code_source_point_for_test(25).unwrap(),
        );
        assert_eq!(panel.read(app).code_selection_for_test(), Some(21..25));
    })
    .unwrap();
}

#[test]
fn native_captured_branch_code_pending_escape_navigation_and_wheel_preserve_exact_origin() {
    require_fixture_ledger();
    let source =
        "Intro\n\n```text\nalpha beta\n".to_owned() + &"more source\n".repeat(60) + "```\n";
    run_mounted_with_prepared_home_retained(
        1,
        0,
        move |path| fixture::seed(path, &source, true),
        |owner, faults, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready_code(window, &panel, cx).await;
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
                let slot = Arc::new(Mutex::new(None));
                let worker_slot = slot.clone();
                window
                    .update(cx, |root, _, _| {
                        root.test_captured_branch_hooks(
                            Some(Box::new(move |_| {
                                *worker_slot.lock().unwrap() =
                                    Some(faults.block_next(FaultPoint::BeforeCommit))
                            })),
                            None,
                        )
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    key(native, app, "shift-f10");
                    assert!(
                        panel.read(app).quote_menu_open_for_test(),
                        "Code menu failed: {:?}; origin={:?}",
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
                                panel
                                    .transcript_menu_rows_for_test()
                                    .iter()
                                    .any(|row| row.0 == "Discuss in new branch" && row.1)
                            })
                            .unwrap()
                    },
                    "the current Code origin must qualify before invoking",
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
                    |_| {
                        slot.lock()
                            .unwrap()
                            .as_ref()
                            .is_some_and(|block: &FaultBlock| {
                                block.wait_until_reached(Duration::ZERO)
                            })
                    },
                    "Code branch creation must reach the held commit boundary",
                )
                .await;
                let held = Held(slot.lock().unwrap().take().unwrap());
                let original = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                let before = panel
                    .read_with(cx, |panel, _| {
                        (
                            panel.code_viewport_for_test().unwrap().2,
                            panel.code_horizontal_offset_for_test(),
                            panel.code_selection_for_test(),
                        )
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    key(native, app, "escape");
                    key(native, app, "pagedown");
                    key(native, app, "end");
                    let position = panel.read(app).code_viewport_for_test().unwrap().1.center();
                    native.dispatch_input_for_test(
                        gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position,
                            delta: gpui::ScrollDelta::Pixels(gpui::point(
                                gpui::px(-1000.),
                                gpui::px(-1000.),
                            )),
                            ..Default::default()
                        }),
                        app,
                    );
                    native.draw_and_present_for_test(app);
                    assert_eq!(
                        (
                            panel.read(app).code_viewport_for_test().unwrap().2,
                            panel.read(app).code_horizontal_offset_for_test(),
                            panel.read(app).code_selection_for_test()
                        ),
                        before
                    );
                })
                .unwrap();
                assert_eq!(
                    window
                        .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                        .unwrap(),
                    original
                );
                held.0.release();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, _| {
                                !root.test_running_thread_activation_pending()
                                    && root.test_captured_branch_state().is_none_or(|state| {
                                        !matches!(
                                            state.0,
                                            "Creating" | "Reconciling" | "Activating"
                                        )
                                    })
                            })
                            .unwrap()
                    },
                    "the held Code creation must finish without replacing its captured request",
                )
                .await;
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_captured_branch_code_pending_layout_cannot_open_from_cached_fragments() {
    require_fixture_ledger();
    let source = "Intro\n\n```text\nalpha beta ".to_owned() + &"x".repeat(1000) + "\n```\n";
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
                ready_code(window, &panel, cx).await;
                with_native(window, cx, |native, app| {
                    let position = panel.read(app).code_viewport_for_test().unwrap().1.center();
                    native.dispatch_input_for_test(
                        gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position,
                            delta: gpui::ScrollDelta::Pixels(gpui::point(
                                gpui::px(-500.),
                                gpui::px(0.),
                            )),
                            ..Default::default()
                        }),
                        app,
                    );
                    assert_eq!(
                        panel.read(app).diagnostic_snapshot().code_panels[0].layout_state,
                        "pending"
                    );
                    native.dispatch_input_for_test(
                        gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("shift-f10").unwrap(),
                            is_held: false,
                        }),
                        app,
                    );
                    assert!(!panel.read(app).quote_menu_open_for_test());
                    assert_eq!(panel.read(app).code_selection_for_test(), Some(21..25));
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
