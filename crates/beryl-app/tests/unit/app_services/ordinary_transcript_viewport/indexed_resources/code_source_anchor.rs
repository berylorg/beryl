use super::*;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use syndic_storage::{
    ProjectionSourceRange, ResourceBacking, ResourceMetadataRecord, ResourceStructure,
};

#[test]
fn native_code_source_anchor_survives_font_and_saved_source_epochs() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            seed_history_text(
                path,
                1,
                Some(&format!(
                    "```text\n{}\ny\n```",
                    "x".repeat(LONG_LINE_BYTES as usize)
                )),
            );
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                wait_anchor_layout(window, &panel, cx).await;
                let (identity, key, outer, resource) = panel
                    .read_with(cx, |panel, _| {
                        (
                            panel.code_presentation_for_test().unwrap().0,
                            panel.diagnostic_snapshot().code_panels[0].key,
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            panel.code_viewport_for_test().unwrap().0,
                        )
                    })
                    .unwrap();
                let focus = with_native(window, cx, |native, app| {
                    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                    code_interaction::press(native, app, bounds.center());
                    code_interaction::release(native, app, bounds.center());
                    let target = panel
                        .read(app)
                        .code_layout_first_fragment_end_for_test(0)
                        .unwrap()
                        * 16.0
                        + 0.375;
                    native.dispatch_input_for_test(
                        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: bounds.center(),
                            delta: gpui::ScrollDelta::Pixels(point(px(-target), px(0.0))),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                    native.focused(app).unwrap()
                })
                .unwrap();
                wait_anchor_layout(window, &panel, cx).await;
                let (anchor, painted, horizontal) = panel
                    .read_with(cx, |panel, _| {
                        let anchor = panel.code_inline_anchor_for_test().unwrap();
                        assert_eq!(anchor.0, 0);
                        assert!(anchor.1 > BODY_START + 8192);
                        assert!(anchor.2.fract() != 0.0);
                        let painted = panel
                            .code_inline_anchor_painted_position_for_test()
                            .unwrap();
                        assert!((painted - anchor.2).abs() < 0.05);
                        (
                            anchor,
                            painted,
                            panel.code_horizontal_offset_for_test().unwrap(),
                        )
                    })
                    .unwrap();
                let observed_epoch = panel
                    .read_with(cx, |panel, _| {
                        panel.diagnostic_snapshot().code_panels[0].layout_epoch
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let before = native.text_system().font_environment_key().unwrap();
                    native
                        .text_system()
                        .adopt_font_environment_for_test()
                        .unwrap();
                    assert_ne!(native.text_system().font_environment_key(), Some(before));
                    panel.update(app, |_, cx| cx.notify());
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| panel.read_with(cx, |panel, _| {
                        let diagnostics = panel.diagnostic_snapshot();
                        let code = &diagnostics.code_panels[0];
                        code.layout_epoch != observed_epoch
                            && code.layout_state == "ready"
                            && code.complete_inline_extent.is_some()
                            && panel.code_painted_body_glyphs_for_test() > 0
                    }).unwrap(),
                    "observed font collection adoption must invalidate unchanged requested-font geometry",
                )
                .await;
                panel
                    .read_with(cx, |panel, _| {
                        let restored = panel.code_inline_anchor_for_test().unwrap();
                        assert_eq!((restored.0, restored.1), (anchor.0, anchor.1));
                        assert!((restored.2 - anchor.2).abs() < 0.05);
                        assert!(
                            (panel
                                .code_inline_anchor_painted_position_for_test()
                                .unwrap()
                                - painted)
                                .abs()
                                < 0.05
                        );
                        assert!(
                            (panel.code_horizontal_offset_for_test().unwrap() - horizontal).abs()
                                < 0.05
                        );
                        assert_anchor_owner(panel, identity, key, outer);
                    })
                    .unwrap();
                let original_appearance = window
                    .read_with(cx, |root, _| root.test_appearance())
                    .unwrap();
                let prepared = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    native_appearance::system_font_appearance_with_code_size(graph.state(), 48)
                };
                let large = Arc::new(crate::theme_runtime::AppearanceGeneration::new(
                    original_appearance.number().checked_next().unwrap(),
                    prepared,
                    crate::theme_runtime::AppearancePublication::Durable,
                ));
                for (appearance, enlarged) in [(large, true), (original_appearance, false)] {
                    let epoch = panel
                        .read_with(cx, |panel, _| {
                            panel.diagnostic_snapshot().code_panels[0].layout_epoch
                        })
                        .unwrap();
                    cx.update(|app| {
                        MainWindowShellRoot::test_apply_appearance(window, appearance, app)
                    })
                    .unwrap();
                    wait_until(
                        window,
                        cx,
                        |cx| {
                            panel
                                .read_with(cx, |panel, _| {
                                    let diagnostics = panel.diagnostic_snapshot();
                                    let code = &diagnostics.code_panels[0];
                                    code.layout_epoch != epoch
                                        && code.layout_state == "ready"
                                        && code.complete_inline_extent.is_some()
                                        && panel
                                            .code_inline_anchor_painted_position_for_test()
                                            .is_some()
                                        && panel.code_painted_body_glyphs_for_test() > 0
                                })
                                .unwrap()
                        },
                        "new font epoch must remeasure the saved caret before publication",
                    )
                    .await;
                    panel
                        .read_with(cx, |panel, _| {
                            let restored = panel.code_inline_anchor_for_test().unwrap();
                            assert_eq!(restored.0, anchor.0);
                            assert_eq!(restored.1, anchor.1);
                            assert!((restored.2 - anchor.2).abs() < 0.05);
                            assert!(
                                (panel
                                    .code_inline_anchor_painted_position_for_test()
                                    .unwrap()
                                    - painted)
                                    .abs()
                                    < 0.05
                            );
                            let current = panel.code_horizontal_offset_for_test().unwrap();
                            if enlarged {
                                assert!((current - horizontal).abs() > 1.0);
                            } else {
                                assert!((current - horizontal).abs() < 0.05);
                            }
                            assert_anchor_owner(panel, identity, key, outer);
                        })
                        .unwrap();
                    with_native(window, cx, |native, app| {
                        assert_eq!(native.focused(app), Some(focus.clone()))
                    })
                    .unwrap();
                }

                let original = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    graph
                        .syndic()
                        .resource(
                            graph.home(),
                            resource.0.parse().unwrap(),
                            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
                        )
                        .unwrap()
                        .unwrap()
                };
                let ResourceBacking::TextRange { source, range } = original.backing() else {
                    panic!("saved Code source")
                };
                let mut resource_revision = original.revision().get();
                for bytes in [50_000, LONG_LINE_BYTES + 3, 8000, 0] {
                    resource_revision += 1;
                    let full = bytes == LONG_LINE_BYTES + 3;
                    let content = if full {
                        "x".repeat(LONG_LINE_BYTES as usize) + "\ny\n"
                    } else {
                        "x".repeat(bytes as usize)
                    };
                    let metadata = ResourceMetadataRecord::new(
                        original.id(),
                        beryl_model::ProjectionRevision::new(resource_revision).unwrap(),
                        original.projection_id().unwrap(),
                        original.item_id(),
                        original.ordinal().unwrap(),
                        original.kind(),
                        original.media_type().unwrap(),
                        ResourceBacking::TextRange {
                            source,
                            range: ProjectionSourceRange::new(range.start(), range.start() + bytes)
                                .unwrap(),
                        },
                        Sha256::digest(content.as_bytes()).into(),
                        None,
                        ResourceStructure::code(
                            Some("text"),
                            if bytes == 0 {
                                0
                            } else if full {
                                2
                            } else {
                                1
                            },
                        )
                        .unwrap(),
                    )
                    .unwrap();
                    let epoch = panel
                        .read_with(cx, |panel, _| {
                            panel.diagnostic_snapshot().code_panels[0].layout_epoch
                        })
                        .unwrap();
                    let revision =
                        history_storage::advance_history_revision(&owner, Some(metadata));
                    wait_until(window, cx, |cx| panel.read_with(cx, |panel, _| {
                        let diagnostics = panel.diagnostic_snapshot();
                        let code = &diagnostics.code_panels[0];
                        code.layout_epoch != epoch
                            && panel.code_provider_revision_for_test() == Some(revision)
                            && code.layout_state == "ready"
                            && code.source_bytes == Some(bytes)
                            && code.complete_inline_extent.is_some()
                    }).unwrap(), "successor source must resolve the caret through ordinary indexed admission").await;
                    panel
                        .read_with(cx, |panel, _| {
                            assert_anchor_owner(panel, identity, key, outer);
                            let code = &panel.diagnostic_snapshot().code_panels[0];
                            if bytes > 8192 {
                                assert_eq!(
                                    panel.code_inline_anchor_for_test().unwrap().1,
                                    anchor.1
                                );
                                assert!(
                                    (panel
                                        .code_inline_anchor_painted_position_for_test()
                                        .unwrap()
                                        - painted)
                                        .abs()
                                        < 0.05
                                );
                                assert!(
                                    (panel.code_horizontal_offset_for_test().unwrap() - horizontal)
                                        .abs()
                                        < 0.05
                                );
                                assert!(panel.code_painted_body_glyphs_for_test() > 0);
                            } else if bytes != 0 {
                                assert!(
                                    panel.code_inline_anchor_for_test().unwrap().1
                                        <= BODY_START + bytes
                                );
                                let maximum = (code.complete_inline_extent.unwrap()
                                    - (code.viewport_size.0 - 20.0))
                                    .max(0.0);
                                assert!(
                                    (panel.code_horizontal_offset_for_test().unwrap() - maximum)
                                        .abs()
                                        < 0.05
                                );
                                assert!(panel.code_painted_body_glyphs_for_test() > 0);
                            } else {
                                assert_eq!(panel.code_horizontal_offset_for_test(), Some(0.0));
                                assert!(panel.code_inline_anchor_for_test().is_none());
                                assert_eq!(code.complete_inline_extent, Some(0.0));
                                assert_eq!(code.layout_fragments, 0);
                            }
                        })
                        .unwrap();
                    with_native(window, cx, |native, app| {
                        assert_eq!(native.focused(app), Some(focus.clone()))
                    })
                    .unwrap();
                }
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

fn assert_anchor_owner(
    panel: &SyndicTranscriptPanel,
    identity: u64,
    key: Option<[u8; 16]>,
    outer: f32,
) {
    assert_eq!(panel.code_presentation_for_test().unwrap().0, identity);
    let code = &panel.diagnostic_snapshot().code_panels[0];
    assert_eq!(code.key, key);
    assert!(!code.selected_for_wheel);
    assert!(code.layout_checkpoints <= 64);
    assert!(code.layout_fragments <= 8);
    assert!(
        panel
            .code_layout_max_fragment_bytes_for_test()
            .is_none_or(|bytes| bytes <= 1024)
    );
    assert_eq!(
        panel
            .measured_frame_for_test()
            .unwrap()
            .manual_scroll_total_px,
        outer
    );
}

async fn wait_anchor_layout(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    wait_until(
        window,
        cx,
        |cx| {
            panel
                .read_with(cx, |panel, _| {
                    let diagnostics = panel.diagnostic_snapshot();
                    diagnostics.code_panels.first().is_some_and(|code| {
                        code.layout_state == "ready"
                            && code.complete_inline_extent.is_some()
                            && panel
                                .code_inline_anchor_painted_position_for_test()
                                .is_some()
                            && panel.code_painted_body_glyphs_for_test() > 0
                    })
                })
                .unwrap()
        },
        "saved Code must publish bounded source-mapped body paint",
    )
    .await;
}
