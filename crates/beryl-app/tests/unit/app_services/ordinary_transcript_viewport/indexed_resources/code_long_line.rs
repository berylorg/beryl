use super::*;

#[path = "code_copy_selection.rs"]
mod code_copy_selection;
use crate::syndic_transcript::SyndicTranscriptPanel;
use gpui::{AsyncApp, Entity, PlatformInput, point, px};

#[path = "../../../../support/native_window_geometry.rs"]
pub(super) mod native_geometry;

const LONG_LINE_BYTES: u64 = 100_000;
const BODY_START: u64 = "```text\n".len() as u64;

#[path = "code_source_anchor.rs"]
mod code_source_anchor;

#[path = "code_resource_extent.rs"]
mod code_resource_extent;

#[test]
fn native_code_layout_traverses_saved_hundred_kilobyte_line_with_bounded_canonical_paint() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            let source = format!("```text\n{}\ny\n```", "x".repeat(LONG_LINE_BYTES as usize));
            seed_history_text(path, 1, Some(&source));
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .update(cx, |root, _, _| root.test_transcript_panel())
                    .unwrap();
                let mut saw_incomplete_geometry = false;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                let diagnostics = panel.diagnostic_snapshot();
                                let Some(code) = diagnostics.code_panels.first() else {
                                    return false;
                                };
                                assert_ne!(code.layout_state, "refused");
                                assert!(code.layout_checkpoints <= 64);
                                assert!(code.layout_fragments <= 8);
                                assert!(
                                    panel
                                        .code_layout_max_fragment_bytes_for_test()
                                        .is_none_or(|bytes| bytes <= 1024)
                                );
                                if code.layout_fragments > 0
                                    && code.complete_inline_extent.is_none()
                                {
                                    saw_incomplete_geometry = true;
                                }
                                code.layout_state == "ready"
                                    && code.logical_lines == 2
                                    && code.layout_fragments > 0
                                    && panel.painted_glyphs_for_test() > 50
                            })
                            .unwrap()
                    },
                    "saved long Code must publish canonical provider-backed geometry",
                )
                .await;
                assert!(
                    saw_incomplete_geometry,
                    "prefix paint must expose incomplete horizontal extent"
                );

                let initial = panel
                    .read_with(cx, |panel, _| {
                        let code = panel.diagnostic_snapshot().code_panels[0].clone();
                        assert_eq!(code.wrap_mode, "no_wrap");
                        assert_eq!(code.realized_visual_lines, 2);
                        assert_eq!(
                            code.realized_source_range.as_ref().unwrap().start,
                            BODY_START
                        );
                        assert_eq!(code.source_bytes, Some(LONG_LINE_BYTES + 3));
                        (
                            panel.code_presentation_for_test().unwrap().0,
                            code.key,
                            code.layout_epoch,
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                        )
                    })
                    .unwrap();
                let focus = with_native(window, cx, |native, app| {
                    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                    code_interaction::press(native, app, bounds.center());
                    code_interaction::release(native, app, bounds.center());
                    let focus = native.focused(app).expect("Code click owns keyboard focus");
                    native.dispatch_input_for_test(
                        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: bounds.center(),
                            delta: gpui::ScrollDelta::Pixels(point(px(-100_000.0), px(0.0))),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                    assert_eq!(
                        panel.read(app).code_horizontal_offset_for_test(),
                        Some(100_000.0)
                    );
                    assert_eq!(
                        panel.read(app).diagnostic_snapshot().code_panels[0].layout_state,
                        "pending"
                    );
                    native.draw_and_present_for_test(app);
                    assert!(
                        panel.read(app).painted_glyphs_for_test() > 50,
                        "pending horizontal demand retains coherent painted glyphs"
                    );
                    assert_eq!(native.focused(app), Some(focus.clone()));
                    focus
                })
                .unwrap();

                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                let code = &panel.diagnostic_snapshot().code_panels[0];
                                assert_ne!(code.layout_state, "refused");
                                assert!(code.layout_fragments <= 8);
                                assert!(code.layout_checkpoints <= 64);
                                assert!(panel.painted_glyphs_for_test() > 50);
                                code.layout_state == "ready"
                                    && panel
                                        .code_layout_line_source_range_for_test(0)
                                        .as_ref()
                                        .is_some_and(|range| {
                                            range.start > BODY_START + 8192
                                                && range.end <= BODY_START + LONG_LINE_BYTES
                                        })
                            })
                            .unwrap()
                    },
                    "distant horizontal target must resolve to later exact saved source",
                )
                .await;
                with_native(window, cx, |native, app| {
                    let panel = panel.read(app);
                    let code = &panel.diagnostic_snapshot().code_panels[0];
                    assert_eq!(panel.code_presentation_for_test().unwrap().0, initial.0);
                    assert_eq!(code.key, initial.1);
                    assert_eq!(code.layout_epoch, initial.2);
                    assert_eq!(
                        panel
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px,
                        initial.3
                    );
                    assert_eq!(panel.code_viewport_for_test().unwrap().2, 0);
                    assert!(!code.selected_for_wheel);
                    assert_eq!(native.focused(app), Some(focus.clone()));
                    assert_eq!(panel.code_horizontal_offset_for_test(), Some(100_000.0));
                    assert!(panel.code_layout_max_fragment_bytes_for_test().unwrap() <= 1024);
                })
                .unwrap();

                with_native(window, cx, |native, app| {
                    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                    native.dispatch_input_for_test(
                        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: bounds.center(),
                            delta: gpui::ScrollDelta::Pixels(point(px(-1_000_000_000.0), px(0.0))),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                    native.draw_and_present_for_test(app);
                    assert!(panel.read(app).painted_glyphs_for_test() > 50);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                let code = &panel.diagnostic_snapshot().code_panels[0];
                                assert_ne!(code.layout_state, "refused");
                                code.layout_state == "ready"
                                    && code.complete_inline_extent.is_some()
                                    && panel.painted_glyphs_for_test() > 50
                            })
                            .unwrap()
                    },
                    "terminal traversal must complete exact extent and publish its clamped target",
                )
                .await;
                with_native(window, cx, |native, app| {
                    let panel = panel.read(app);
                    let code = &panel.diagnostic_snapshot().code_panels[0];
                    let width = code.complete_inline_extent.unwrap();
                    assert!(width > 100_000.0);
                    assert_eq!(
                        panel.code_horizontal_offset_for_test(),
                        Some((width - (code.viewport_size.0 - 20.0)).max(0.0))
                    );
                    let source = panel.code_layout_line_source_range_for_test(0).unwrap();
                    assert!(source.start > BODY_START + LONG_LINE_BYTES - 8192);
                    assert_eq!(source.end, BODY_START + LONG_LINE_BYTES);
                    assert!(code.layout_fragments <= 8);
                    assert!(code.layout_checkpoints <= 64);
                    assert!(panel.code_layout_max_fragment_bytes_for_test().unwrap() <= 1024);
                    assert_eq!(panel.code_presentation_for_test().unwrap().0, initial.0);
                    assert_eq!(code.key, initial.1);
                    assert_eq!(code.layout_epoch, initial.2);
                    assert_eq!(
                        panel
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px,
                        initial.3
                    );
                    assert_eq!(native.focused(app), Some(focus.clone()));
                    assert!(!code.selected_for_wheel);
                    assert!(panel.painted_glyphs_for_test() > 50);
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
                assert!(drained, "ordinary source reads release before retirement");
                window
                    .update(cx, |root, native, app| {
                        root.retire_running_thread_views(native, app).unwrap()
                    })
                    .unwrap();
                panel.update(cx, |panel, _| panel.retire()).unwrap();
                panel
                    .read_with(cx, |panel, _| {
                        assert!(panel.diagnostic_snapshot().code_panels.is_empty());
                        assert!(panel.code_layout_max_fragment_bytes_for_test().is_none());
                    })
                    .unwrap();
                cx.update(|app| app.quit()).unwrap();
            })
        },
    );
}

pub(super) async fn wait_until(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut AsyncApp,
    ready: impl FnMut(&mut AsyncApp) -> bool,
    message: &str,
) {
    wait_until_with_visibility(window, cx, ready, message, false).await;
}

pub(super) async fn wait_until_unminimized(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut AsyncApp,
    ready: impl FnMut(&mut AsyncApp) -> bool,
    message: &str,
) {
    wait_until_with_visibility(window, cx, ready, message, false).await;
}

pub(super) async fn wait_until_with_visibility(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut AsyncApp,
    mut ready: impl FnMut(&mut AsyncApp) -> bool,
    message: &str,
    minimize: bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    if !minimize {
        native_geometry::restore(window, cx);
    }
    loop {
        with_native(window, cx, |native, app| {
            native.draw_and_present_for_test(app);
            if minimize {
                native.minimize_window();
            }
        })
        .unwrap();
        if ready(cx) {
            return;
        }
        if Instant::now() >= deadline {
            let panel = window
                .read_with(cx, |root, _| root.test_transcript_panel())
                .unwrap();
            let facts = panel
                .read_with(cx, |panel, _| {
                    let diagnostics = panel.diagnostic_snapshot();
                    diagnostics.code_panels.first().map(|code| {
                        (
                            code.layout_state,
                            code.layout_fragments,
                            code.layout_checkpoints,
                            panel.code_layout_line_source_range_for_test(0),
                            panel.code_horizontal_offset_for_test(),
                            panel.painted_glyphs_for_test(),
                            panel.code_copy_feedback_for_test(),
                            panel.code_copy_progress_for_test(),
                            panel.resource_demand().and_then(|demand| demand.indexed),
                            panel.code_layout_work_for_test(),
                        )
                    })
                })
                .unwrap();
            eprintln!("Code wait timeout: {message}; facts={facts:?}");
            panic!("{message}");
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
}

#[test]
fn native_code_layout_unicode_context_crosses_unaligned_display_edges_without_stalling() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            let source = "🍒".repeat(2048) + "xxx" + &"🍒".repeat(2049);
            seed_history_text(path, 1, Some(&format!("```text\n{source}\n```")));
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .update(cx, |root, _, _| root.test_transcript_panel())
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                let diagnostics = panel.diagnostic_snapshot();
                                let Some(code) = diagnostics.code_panels.first() else {
                                    return false;
                                };
                                assert_ne!(code.layout_state, "refused");
                                code.layout_state == "ready"
                                    && code.logical_lines == 1
                                    && code.layout_fragments > 0
                                    && panel.code_painted_body_glyphs_for_test() > 0
                            })
                            .unwrap()
                    },
                    "saved Unicode Code must publish its first canonical window",
                )
                .await;
                let key = panel
                    .read_with(cx, |panel, _| {
                        panel.diagnostic_snapshot().code_panels[0].key
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                    code_interaction::press(native, app, bounds.center());
                    code_interaction::release(native, app, bounds.center());
                    native.dispatch_input_for_test(
                        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: bounds.center(),
                            delta: gpui::ScrollDelta::Pixels(point(px(-20_000.0), px(0.0))),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                    native.draw_and_present_for_test(app);
                    assert!(panel.read(app).code_painted_body_glyphs_for_test() > 0);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                let code = &panel.diagnostic_snapshot().code_panels[0];
                                assert_ne!(code.layout_state, "refused");
                                assert!(code.layout_checkpoints <= 32);
                                assert!(code.layout_fragments <= 4);
                                assert!(
                                    panel
                                        .code_layout_max_fragment_bytes_for_test()
                                        .is_none_or(|bytes| bytes <= 1024)
                                );
                                code.layout_state == "ready"
                                    && code.complete_inline_extent.is_some()
                            })
                            .unwrap()
                    },
                    "unaligned Unicode context demand must complete through ordinary Display reads",
                )
                .await;
                panel
                    .read_with(cx, |panel, _| {
                        let code = &panel.diagnostic_snapshot().code_panels[0];
                        assert_eq!(code.key, key);
                        assert_eq!(code.logical_lines, 1);
                        assert_eq!(code.source_bytes, Some(16_392));
                        assert!(code.complete_inline_extent.unwrap() > 20_000.0);
                        assert_eq!(panel.code_horizontal_offset_for_test(), Some(20_000.0));
                        assert!(!code.selected_for_wheel);
                        assert!(panel.code_painted_body_glyphs_for_test() > 0);
                        assert!(panel.code_layout_max_fragment_bytes_for_test().unwrap() <= 1024);
                        let realized = code.realized_source_range.as_ref().unwrap();
                        assert!(realized.start > BODY_START);
                        assert!(realized.end <= BODY_START + 16_391);
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
                panel
                    .read_with(cx, |panel, _| {
                        assert!(panel.diagnostic_snapshot().code_panels.is_empty());
                        assert!(panel.code_layout_max_fragment_bytes_for_test().is_none());
                    })
                    .unwrap();
                cx.update(|app| app.quit()).unwrap();
            })
        },
    );
}

#[test]
fn native_code_layout_refusal_preserves_atomic_multiline_paint_and_recovers_on_new_target() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            let source = format!(
                "```text\n{}\n{}{}\n```",
                "x".repeat(3000),
                "x".repeat(500),
                "\t".repeat(160)
            );
            seed_history_text(path, 1, Some(&source));
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .update(cx, |root, _, _| root.test_transcript_panel())
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                let diagnostics = panel.diagnostic_snapshot();
                                let Some(code) = diagnostics.code_panels.first() else {
                                    return false;
                                };
                                assert_ne!(code.layout_state, "refused");
                                code.layout_state == "ready"
                                    && code.logical_lines == 2
                                    && panel.code_layout_painted_offsets_for_test()
                                        == vec![0.0, 0.0]
                                    && panel.painted_glyphs_for_test() > 50
                            })
                            .unwrap()
                    },
                    "two saved Code lines must publish one coherent origin window",
                )
                .await;
                let stop = with_native(window, cx, |native, app| {
                    panel.read(app).code_tab_stop_for_test(native)
                })
                .unwrap();
                let needs_resize = panel
                    .read_with(cx, |panel, _| {
                        panel.diagnostic_snapshot().code_panels[0].viewport_size.0 - 20.0
                            <= 5.0 * stop
                    })
                    .unwrap();
                if needs_resize {
                    with_native(window, cx, |native, _| {
                        native.resize(gpui::size(
                            px((8.0 * stop + 400.0).max(1200.0)),
                            native.viewport_size().height.max(px(600.0)),
                        ));
                    })
                    .unwrap();
                    wait_until(
                        window,
                        cx,
                        |cx| {
                            panel
                                .read_with(cx, |panel, _| {
                                    let code = &panel.diagnostic_snapshot().code_panels[0];
                                    code.viewport_size.0 - 20.0 > 5.0 * stop
                                        && code.layout_state == "ready"
                                        && panel.code_layout_painted_offsets_for_test()
                                            == vec![0.0, 0.0]
                                })
                                .unwrap()
                        },
                        "tab-capacity fixture must expose more than five exact tab advances",
                    )
                    .await;
                }
                let original = panel
                    .read_with(cx, |panel, _| {
                        let code = &panel.diagnostic_snapshot().code_panels[0];
                        (
                            panel.code_presentation_for_test().unwrap().0,
                            code.key,
                            code.layout_epoch,
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                        )
                    })
                    .unwrap();
                let (focus, refused_target) = with_native(window, cx, |native, app| {
                    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                    let prefix_end = panel
                        .read(app)
                        .code_layout_first_fragment_end_for_test(1)
                        .unwrap();
                    let refused_target = (prefix_end / stop).ceil() * stop + 8.0 * stop;
                    assert!(
                        refused_target > prefix_end && refused_target < prefix_end + 160.0 * stop
                    );
                    assert!(f32::from(bounds.size.width) - 20.0 > 5.0 * stop);
                    code_interaction::press(native, app, bounds.center());
                    code_interaction::release(native, app, bounds.center());
                    let focus = native.focused(app).expect("Code acquires keyboard focus");
                    native.dispatch_input_for_test(
                        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: bounds.center(),
                            delta: gpui::ScrollDelta::Pixels(point(px(-refused_target), px(0.0))),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                    native.draw_and_present_for_test(app);
                    (focus, refused_target)
                })
                .unwrap();
                wait_until(window, cx, |cx| panel.read_with(cx, |panel, _| {
                    let code = &panel.diagnostic_snapshot().code_panels[0];
                    assert_eq!(panel.code_layout_painted_offsets_for_test(), vec![0.0, 0.0],
                        "one prepared line cannot replace an older coherent multiline publication");
                    assert!(panel.painted_glyphs_for_test() > 50);
                    assert_eq!(panel.code_presentation_for_test().unwrap().0, original.0);
                    assert_eq!(code.key, original.1);
                    assert_eq!(code.layout_epoch, original.2);
                    assert_eq!(panel.measured_frame_for_test().unwrap().manual_scroll_total_px, original.3);
                    assert!(!code.selected_for_wheel);
                    code.layout_state == "refused"
                }).unwrap(), "tab geometry exceeding the fixed fragment ceiling must refuse locally").await;
                with_native(window, cx, |native, app| {
                    assert_eq!(native.focused(app), Some(focus.clone()));
                    assert_eq!(
                        panel.read(app).code_horizontal_offset_for_test(),
                        Some(refused_target)
                    );
                    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                    native.dispatch_input_for_test(
                        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: bounds.center(),
                            delta: gpui::ScrollDelta::Pixels(point(
                                px(refused_target - 1000.0),
                                px(0.0),
                            )),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                    native.draw_and_present_for_test(app);
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                let code = &panel.diagnostic_snapshot().code_panels[0];
                                let offsets = panel.code_layout_painted_offsets_for_test();
                                assert!(
                                    offsets == vec![0.0, 0.0] || offsets == vec![1000.0, 1000.0],
                                    "target recovery must publish both lines atomically"
                                );
                                assert!(panel.painted_glyphs_for_test() > 50);
                                code.layout_state == "ready" && offsets == vec![1000.0, 1000.0]
                            })
                            .unwrap()
                    },
                    "a valid successor target must recover from the refused request",
                )
                .await;
                with_native(window, cx, |native, app| {
                    let panel = panel.read(app);
                    let code = &panel.diagnostic_snapshot().code_panels[0];
                    assert_eq!(panel.code_horizontal_offset_for_test(), Some(1000.0));
                    assert_eq!(panel.code_presentation_for_test().unwrap().0, original.0);
                    assert_eq!(code.key, original.1);
                    assert_eq!(code.layout_epoch, original.2);
                    assert_eq!(
                        panel
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px,
                        original.3
                    );
                    assert_eq!(native.focused(app), Some(focus.clone()));
                    assert!(!code.selected_for_wheel);
                    assert!(code.layout_checkpoints <= 64);
                    assert!(code.layout_fragments <= 8);
                    assert!(panel.code_layout_max_fragment_bytes_for_test().unwrap() <= 1024);
                })
                .unwrap();
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

pub(super) async fn retire_code_fixture(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
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
    panel
        .read_with(cx, |panel, _| {
            assert!(panel.diagnostic_snapshot().code_panels.is_empty());
            assert!(panel.code_layout_painted_offsets_for_test().is_empty());
        })
        .unwrap();
    cx.update(|app| app.quit()).unwrap();
}

pub(super) fn with_native<R>(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut AsyncApp,
    run: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> R,
) -> gpui::Result<R> {
    cx.update(|app| {
        app.update_window(window.into(), |_, native, app| run(native, app))
            .unwrap()
    })
}
