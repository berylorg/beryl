use super::*;

const CODE_LINE_BYTES: u64 = ("let payload = 1;".len() * 16 + 1) as u64;

#[test]
fn native_code_panel_mounts_bounded_lines_and_routes_selected_wheel_to_inner_viewport() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            seed_history_text(
                path,
                1,
                Some(&format!(
                    "```rust\n{}\n```",
                    ("let payload = 1;".repeat(16) + "\n").repeat(2000)
                )),
            )
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .update(cx, |root, _, _| root.test_transcript_panel())
                    .unwrap();
                let mut mounted = None;
                let mut facts = None;
                for frame in 0..300 {
                    cx.update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            native.draw_and_present_for_test(app)
                        })
                        .unwrap()
                    })
                    .unwrap();
                    if frame == 0 {
                        window
                            .update(cx, |_, native, _| native.minimize_window())
                            .unwrap();
                    }
                    facts = panel
                        .read_with(cx, |panel, _| {
                            let snapshot = panel.snapshot();
                            (
                                snapshot.records.len(),
                                snapshot.resources.metadata.len(),
                                panel
                                    .code_viewport_for_test()
                                    .map(|(_, _, top, selected)| (top, selected)),
                                panel.resource_demand().map(|demand| {
                                    (demand.indexed.is_some(), demand.metadata.is_some())
                                }),
                                panel.diagnostic_snapshot().resident_data.resource_bytes,
                                panel
                                    .code_viewport_for_test()
                                    .and_then(|facts| {
                                        panel.code_foreground_response_for_test(&facts.0)
                                    })
                                    .map(|response| match &response.value {
                                        IndexedResourceValue::Pending => ("pending", 0, false),
                                        IndexedResourceValue::Range(value) => {
                                            ("range", value.items.len(), value.request_complete)
                                        }
                                        _ => ("other", 0, false),
                                    }),
                            )
                        })
                        .ok();
                    mounted = panel
                        .read_with(cx, |panel, _| {
                            let facts = panel.code_viewport_for_test()?;
                            let response = panel.code_foreground_response_for_test(&facts.0)?;
                            let IndexedResourceValue::Range(range) = &response.value else {
                                return None;
                            };
                            (!range.items.is_empty() && panel.painted_glyphs_for_test() > 50)
                                .then_some((
                                    facts,
                                    range.items.len(),
                                    response.root.as_ref().unwrap().code_lines,
                                ))
                        })
                        .unwrap();
                    if mounted.is_some() {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                eprintln!("bounded Code mount facts: {facts:?}");
                let ((resource, bounds, top, selected), realized, lines) =
                    mounted.expect("automatically mounted indexed Code panel");
                assert!(realized <= 16);
                assert!(lines >= 2000);
                assert_eq!(top, 0);
                assert!(!selected);
                assert!(bounds.size.height > gpui::px(0.0));
                let diagnostic_key = panel
                    .read_with(cx, |panel, _| {
                        let diagnostics = panel.diagnostic_snapshot();
                        assert_eq!(diagnostics.code_panels.len(), 1);
                        let code = &diagnostics.code_panels[0];
                        assert_eq!(code.source_continuity, "initial");
                        assert_eq!(code.logical_lines, lines);
                        assert_eq!(code.realized_visual_lines, realized);
                        assert_eq!(code.overscan, 2);
                        assert_eq!(code.wrap_mode, "no_wrap");
                        assert_eq!(code.viewport_size.1, 240.0);
                        assert!(!code.selected_for_wheel);
                        assert!(code.anchor_present);
                        assert_eq!(code.anchor_visual_offset, Some(0.0));
                        let range = code.realized_source_range.as_ref().unwrap();
                        assert_eq!(range.start, "```rust\n".len() as u64);
                        assert!(range.end < range.start + code.source_bytes.unwrap());
                        code.key.expect("opaque Code diagnostic key")
                    })
                    .unwrap();
                let before = panel
                    .read_with(cx, |panel, _| {
                        assert!(panel.painted_glyphs_for_test() > 50);
                        panel
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px
                    })
                    .unwrap();
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        code_horizontal::route_wheel(&panel, native, app, false);
                    })
                    .unwrap();
                })
                .unwrap();
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        native.minimize_window();
                        native.dispatch_input_for_test(
                            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                                button: gpui::MouseButton::Left,
                                position: bounds.center(),
                                modifiers: Default::default(),
                                click_count: 1,
                                first_mouse: false,
                            }),
                            app,
                        );
                        native.draw_and_present_for_test(app);
                        native.dispatch_input_for_test(
                            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position: bounds.center(),
                                delta: gpui::ScrollDelta::Pixels(gpui::point(
                                    gpui::px(0.0),
                                    gpui::px(-90.5),
                                )),
                                modifiers: Default::default(),
                                touch_phase: gpui::TouchPhase::Moved,
                            }),
                            app,
                        );
                        native.draw_and_present_for_test(app);
                    })
                    .unwrap();
                })
                .unwrap();
                panel
                    .read_with(cx, |panel, _| {
                        let (_, _, top, selected) = panel.code_viewport_for_test().unwrap();
                        assert_eq!(top, 4);
                        assert!(selected);
                        assert_eq!(
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            before
                        );
                    })
                    .unwrap();
                let mut refreshed = false;
                for _ in 0..150 {
                    cx.update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            native.draw_and_present_for_test(app)
                        })
                        .unwrap()
                    })
                    .unwrap();
                    refreshed = panel.read_with(cx, |panel, _| panel.code_foreground_response_for_test(&resource).is_some_and(|response| {
                    matches!(&response.value, IndexedResourceValue::Range(value) if matches!(value.cursor.request, index::Request::Code { lines } if lines.start == 2) && value.items.len() <= 16)
                })).unwrap();
                    if refreshed {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                assert!(
                    refreshed,
                    "inner scroll must replace its exact indexed demand"
                );
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        native.draw_and_present_for_test(app)
                    })
                    .unwrap()
                })
                .unwrap();
                panel
                    .read_with(cx, |panel, _| {
                        let diagnostics = panel.diagnostic_snapshot();
                        let code = &diagnostics.code_panels[0];
                        assert_eq!(code.key, Some(diagnostic_key));
                        assert!(code.selected_for_wheel);
                        assert_eq!(code.anchor_visual_offset, Some(-10.5));
                        assert!(code.realized_visual_lines <= 16);
                        assert!(code.realized_source_range.as_ref().unwrap().start > 0);
                    })
                    .unwrap();
                code_anchor::scroll_beyond_resident_lines(window, &panel, CODE_LINE_BYTES, cx)
                    .await;
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        code_horizontal::route_wheel(&panel, native, app, true);
                    })
                    .unwrap();
                })
                .unwrap();
                let original = window
                    .read_with(cx, |root, _| root.test_appearance())
                    .unwrap();
                let original_presentation = panel
                    .read_with(cx, |panel, _| panel.code_presentation_for_test().unwrap())
                    .unwrap();
                let prepared = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    native_appearance::system_font_appearance_with_code_size(graph.state(), 48)
                };
                let large = std::sync::Arc::new(crate::theme_runtime::AppearanceGeneration::new(
                    original.number().checked_next().unwrap(),
                    prepared,
                    crate::theme_runtime::AppearancePublication::Durable,
                ));
                for (appearance, large_font) in [(large, true), (original, false)] {
                    cx.update(|app| {
                        MainWindowShellRoot::test_apply_appearance(window, appearance, app);
                    })
                    .unwrap();
                    let mut settled = false;
                    for _ in 0..150 {
                        cx.update(|app| {
                            app.update_window(window.into(), |_, native, app| {
                                native.draw_and_present_for_test(app)
                            })
                            .unwrap();
                        })
                        .unwrap();
                        settled = panel
                            .read_with(cx, |panel, _| {
                                panel.measured_frame_for_test().is_some()
                                    && panel.code_presentation_for_test().is_some_and(
                                        |(_, height, _)| {
                                            if large_font {
                                                height > 20.0
                                            } else {
                                                height == original_presentation.1
                                            }
                                        },
                                    )
                            })
                            .unwrap();
                        if settled {
                            break;
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await;
                    }
                    assert!(settled, "mounted Code accepts the prepared font metric");
                    panel
                        .read_with(cx, |panel, _| {
                            let presentation = panel.code_presentation_for_test().unwrap();
                            assert_eq!(presentation.0, original_presentation.0);
                            assert_eq!(presentation.2, original_presentation.2);
                            let (_, _, top, selected) = panel.code_viewport_for_test().unwrap();
                            assert_eq!(top, 4);
                            assert!(selected);
                            assert_eq!(
                                panel
                                    .measured_frame_for_test()
                                    .unwrap()
                                    .manual_scroll_total_px,
                                before
                            );
                        })
                        .unwrap();
                    for (key, expected_top) in [("down", 5), ("up", 4)] {
                        cx.update(|app| {
                            app.update_window(window.into(), |_, native, app| {
                                native.dispatch_input_for_test(
                                    gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                                        keystroke: gpui::Keystroke::parse(key).unwrap(),
                                        is_held: false,
                                    }),
                                    app,
                                );
                                native.draw_and_present_for_test(app);
                            })
                            .unwrap();
                        })
                        .unwrap();
                        assert_eq!(
                            panel
                                .read_with(cx, |panel, _| panel.code_viewport_for_test().unwrap().2)
                                .unwrap(),
                            expected_top,
                            "Code retains keyboard focus after appearance application"
                        );
                    }
                }
                for (key, expected_top) in [
                    ("down", 5),
                    ("up", 4),
                    ("pageup", 0),
                    ("pagedown", 12),
                    ("home", 0),
                    ("end", lines.saturating_sub(12)),
                    ("home", 0),
                    ("up", 0),
                ] {
                    cx.update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            native.dispatch_input_for_test(
                                gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                                    keystroke: gpui::Keystroke::parse(key).unwrap(),
                                    is_held: false,
                                }),
                                app,
                            );
                            native.draw_and_present_for_test(app);
                        })
                        .unwrap();
                    })
                    .unwrap();
                    panel
                        .read_with(cx, |panel, _| {
                            let (_, _, top, selected) = panel.code_viewport_for_test().unwrap();
                            assert_eq!(top, expected_top, "Code key {key}");
                            assert!(selected, "keyboard scrolling preserves wheel ownership");
                            assert_eq!(
                                panel
                                    .measured_frame_for_test()
                                    .unwrap()
                                    .manual_scroll_total_px,
                                before,
                                "Code key {key} must not scroll the outer viewport"
                            );
                        })
                        .unwrap();
                }
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        native.dispatch_input_for_test(
                            gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                                keystroke: gpui::Keystroke::parse("escape").unwrap(),
                                is_held: false,
                            }),
                            app,
                        );
                        native.draw_and_present_for_test(app);
                        native.dispatch_input_for_test(
                            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position: bounds.center(),
                                delta: gpui::ScrollDelta::Pixels(gpui::point(
                                    gpui::px(0.0),
                                    gpui::px(1000.0),
                                )),
                                modifiers: Default::default(),
                                touch_phase: gpui::TouchPhase::Moved,
                            }),
                            app,
                        );
                        native.draw_and_present_for_test(app);
                    })
                    .unwrap();
                })
                .unwrap();
                let outside = panel
                    .read_with(cx, |panel, _| {
                        let (_, _, top, selected) = panel.code_viewport_for_test().unwrap();
                        assert_eq!(top, 0);
                        assert!(selected, "Escape and a scroll edge retain wheel ownership");
                        assert_eq!(
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            before,
                            "selected edge scrolling must not reach the outer viewport"
                        );
                        let outer = panel.viewport_bounds_for_test().unwrap();
                        let position = outer.origin
                            + gpui::point(
                                outer.size.width - gpui::px(2.0),
                                outer.size.height - gpui::px(2.0),
                            );
                        assert!(!bounds.contains(&position));
                        position
                    })
                    .unwrap();
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        native.dispatch_input_for_test(
                            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                                button: gpui::MouseButton::Left,
                                position: outside,
                                modifiers: Default::default(),
                                click_count: 1,
                                first_mouse: false,
                            }),
                            app,
                        );
                        native.draw_and_present_for_test(app);
                    })
                    .unwrap();
                })
                .unwrap();
                assert!(
                    panel
                        .read_with(cx, |panel, _| !panel.code_viewport_for_test().unwrap().3)
                        .unwrap(),
                    "outer primary press releases wheel ownership"
                );
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        code_interaction::hover_without_acquiring(&panel, native, app);
                    })
                    .unwrap();
                })
                .unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        native.draw_and_present_for_test(app);
                        code_interaction::scrollbar_acquisition_drag_and_focus(&panel, native, app);
                    })
                    .unwrap();
                })
                .unwrap();
                window
                    .update(cx, |root, native, app| {
                        root.set_ordinary_close_interaction_gated(true, app);
                        root.suspend_running_thread_reads(native, app);
                    })
                    .unwrap();
                let mut drained = false;
                for _ in 0..100 {
                    drained = window
                        .update(cx, |root, _, _| {
                            root.release_suspended_running_thread_sources()
                        })
                        .unwrap();
                    if drained {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                assert!(drained);
                window
                    .update(cx, |root, native, app| {
                        root.retire_running_thread_views(native, app).unwrap()
                    })
                    .unwrap();
                panel.update(cx, |panel, _| panel.retire()).unwrap();
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .diagnostic_snapshot()
                            .code_panels
                            .is_empty())
                        .unwrap()
                );
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        native.draw_and_present_for_test(app)
                    })
                    .unwrap();
                    app.quit();
                })
                .unwrap();
            })
        },
    );
}
