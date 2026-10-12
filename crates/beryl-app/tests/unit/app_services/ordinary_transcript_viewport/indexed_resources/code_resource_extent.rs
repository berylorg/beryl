use super::*;
use std::sync::Arc;

const LONG_ORDINAL: u64 = 64;
const LONG_SOURCE_START: u64 = BODY_START + LONG_ORDINAL * 65;

#[test]
fn native_code_complete_extent_includes_long_unseen_line_and_remeasures_font() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            let short = "x".repeat(64) + "\n";
            let source =
                short.repeat(64) + &"x".repeat(LONG_LINE_BYTES as usize) + "\n" + &short.repeat(63);
            seed_history_text(path, 1, Some(&format!("```text\n{source}```")));
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                let mut incomplete = false;
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
                                incomplete |= code.complete_inline_extent.is_none();
                                code.logical_lines == 128
                                    && code.layout_state == "ready"
                                    && panel.code_painted_body_glyphs_for_test() > 0
                            })
                            .unwrap()
                    },
                    "foreground must paint before whole-source extent completes",
                )
                .await;
                assert!(incomplete);
                let (identity, key, outer) = panel
                    .read_with(cx, |panel, _| {
                        (
                            panel.code_presentation_for_test().unwrap().0,
                            panel.diagnostic_snapshot().code_panels[0].key,
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                        )
                    })
                    .unwrap();
                let target = 100_000.375;
                let focus = with_native(window, cx, |native, app| {
                    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                    code_interaction::press(native, app, bounds.center());
                    code_interaction::release(native, app, bounds.center());
                    native.dispatch_input_for_test(
                        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: bounds.center(),
                            delta: gpui::ScrollDelta::Pixels(point(px(-target), px(0.0))),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                    assert_eq!(
                        panel.read(app).code_horizontal_offset_for_test(),
                        Some(target)
                    );
                    native.focused(app).unwrap()
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                let code = &panel.diagnostic_snapshot().code_panels[0];
                                assert_eq!(panel.code_horizontal_offset_for_test(), Some(target));
                                code.layout_state == "ready"
                                    && code.complete_inline_extent.is_some()
                            })
                            .unwrap()
                    },
                    "unseen longest line must authorize exact resource-wide extent",
                )
                .await;
                panel
                    .read_with(cx, |panel, _| {
                        assert!(
                            panel.diagnostic_snapshot().code_panels[0]
                                .complete_inline_extent
                                .unwrap()
                                > target
                        );
                        assert_eq!(panel.code_viewport_for_test().unwrap().2, 0);
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                    let height = panel.read(app).code_presentation_for_test().unwrap().1;
                    native.dispatch_input_for_test(
                        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: bounds.center(),
                            delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-64.0 * height))),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                })
                .unwrap();
                wait_until(window, cx, |cx| panel.read_with(cx, |panel, _| {
                panel.code_viewport_for_test().unwrap().2 == LONG_ORDINAL
                    && panel.diagnostic_snapshot().code_panels[0].layout_state == "ready"
                    && panel.code_complete_line_width_for_test(LONG_ORDINAL).is_some()
                    && panel.code_layout_line_source_range_for_test(LONG_ORDINAL).is_some_and(|range| {
                        range.start > LONG_SOURCE_START + 8192 && range.end <= LONG_SOURCE_START + LONG_LINE_BYTES
                    })
                    && panel.code_painted_body_glyphs_for_test() > 0
            }).unwrap(), "foreground traversal must realize exact later source on the previously unseen line").await;
                let (original_width, source_anchor, painted_anchor, epoch) = panel
                    .read_with(cx, |panel, _| {
                        let code = &panel.diagnostic_snapshot().code_panels[0];
                        assert_eq!(
                            code.complete_inline_extent,
                            panel.code_complete_line_width_for_test(LONG_ORDINAL)
                        );
                        assert_extent_owner(panel, identity, key, outer);
                        (
                            code.complete_inline_extent.unwrap(),
                            panel.code_inline_anchor_for_test().unwrap(),
                            panel
                                .code_inline_anchor_painted_position_for_test()
                                .unwrap(),
                            code.layout_epoch,
                        )
                    })
                    .unwrap();
                let original = window
                    .read_with(cx, |root, _| root.test_appearance())
                    .unwrap();
                let prepared = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    native_appearance::system_font_appearance_with_code_size(graph.state(), 48)
                };
                let large = Arc::new(crate::theme_runtime::AppearanceGeneration::new(
                    original.number().checked_next().unwrap(),
                    prepared,
                    crate::theme_runtime::AppearancePublication::Durable,
                ));
                cx.update(|app| MainWindowShellRoot::test_apply_appearance(window, large, app))
                    .unwrap();
                let mut new_epoch_pending = false;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                let code = &panel.diagnostic_snapshot().code_panels[0];
                                if code.layout_epoch != epoch
                                    && code.complete_inline_extent.is_none()
                                {
                                    new_epoch_pending = true;
                                }
                                code.layout_epoch != epoch
                                    && code.layout_state == "ready"
                                    && code.complete_inline_extent.is_some()
                                    && panel
                                        .code_complete_line_width_for_test(LONG_ORDINAL)
                                        .is_some()
                                    && panel
                                        .code_inline_anchor_painted_position_for_test()
                                        .is_some()
                            })
                            .unwrap()
                    },
                    "font replacement must complete a fresh whole-source width scan",
                )
                .await;
                assert!(new_epoch_pending);
                panel
                    .read_with(cx, |panel, _| {
                        let code = &panel.diagnostic_snapshot().code_panels[0];
                        assert_ne!(code.complete_inline_extent, Some(original_width));
                        assert_eq!(
                            code.complete_inline_extent,
                            panel.code_complete_line_width_for_test(LONG_ORDINAL)
                        );
                        let restored = panel.code_inline_anchor_for_test().unwrap();
                        assert_eq!(restored.0, source_anchor.0);
                        assert_eq!(restored.1, source_anchor.1);
                        assert!(
                            (panel
                                .code_inline_anchor_painted_position_for_test()
                                .unwrap()
                                - painted_anchor)
                                .abs()
                                < 0.05
                        );
                        assert_extent_owner(panel, identity, key, outer);
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    assert_eq!(native.focused(app), Some(focus.clone()));
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
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                let code = &panel.diagnostic_snapshot().code_panels[0];
                                let maximum = (code.complete_inline_extent.unwrap()
                                    - (code.viewport_size.0 - 20.0))
                                    .max(0.0);
                                code.layout_state == "ready"
                                    && panel
                                        .code_horizontal_offset_for_test()
                                        .is_some_and(|left| (left - maximum).abs() < 0.05)
                                    && panel
                                        .code_layout_line_source_range_for_test(LONG_ORDINAL)
                                        .is_some_and(|range| {
                                            range.end == LONG_SOURCE_START + LONG_LINE_BYTES
                                        })
                            })
                            .unwrap()
                    },
                    "terminal movement must clamp using the whole-resource completed width",
                )
                .await;
                panel
                    .read_with(cx, |panel, _| {
                        assert_extent_owner(panel, identity, key, outer)
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    assert_eq!(native.focused(app), Some(focus.clone()))
                })
                .unwrap();
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

fn assert_extent_owner(
    panel: &SyndicTranscriptPanel,
    identity: u64,
    key: Option<[u8; 16]>,
    outer: f32,
) {
    let code = &panel.diagnostic_snapshot().code_panels[0];
    assert_eq!(panel.code_presentation_for_test().unwrap().0, identity);
    assert_eq!(code.key, key);
    assert!(code.selected_for_wheel);
    assert_eq!(
        panel
            .measured_frame_for_test()
            .unwrap()
            .manual_scroll_total_px,
        outer
    );
    assert!(code.layout_checkpoints <= 16 * 32 + 32);
    assert!(code.layout_fragments <= 16 * 4);
    assert!(code.layout_charged_bytes <= 16 * 1024 * 1024);
    assert!(
        panel
            .code_layout_max_fragment_bytes_for_test()
            .is_none_or(|bytes| bytes <= 1024)
    );
}
