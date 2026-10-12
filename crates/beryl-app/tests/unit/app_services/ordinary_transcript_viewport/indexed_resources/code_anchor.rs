use super::*;
use crate::syndic_transcript::SyndicTranscriptPanel;
use gpui::{AsyncApp, Entity, WindowHandle};

pub(super) async fn scroll_beyond_resident_lines(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    line_bytes: u64,
    cx: &mut AsyncApp,
) {
    let (identity, outer, key) = panel
        .read_with(cx, |panel, _| {
            (
                panel.code_presentation_for_test().unwrap().0,
                panel
                    .measured_frame_for_test()
                    .unwrap()
                    .manual_scroll_total_px,
                panel.diagnostic_snapshot().code_panels[0].key,
            )
        })
        .unwrap();
    for (delta, top) in [(-18000.0, 904), (18000.0, 4)] {
        cx.update(|app| {
            app.update_window(window.into(), |_, native, app| {
                let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                native.dispatch_input_for_test(
                    gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                        position: bounds.center(),
                        delta: gpui::ScrollDelta::Pixels(gpui::point(
                            gpui::px(0.0),
                            gpui::px(delta),
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
        let mut settled = false;
        for _ in 0..150 {
            cx.update(|app| {
                app.update_window(window.into(), |_, native, app| {
                    native.draw_and_present_for_test(app);
                })
                .unwrap();
            })
            .unwrap();
            settled = panel
                .read_with(cx, |panel, _| {
                    let (resource, _, actual_top, selected) =
                        panel.code_viewport_for_test().unwrap();
                    assert_eq!(actual_top, top);
                    assert!(selected);
                    panel
                        .indexed_resource_response(&resource)
                        .is_some_and(|response| {
                            matches!(&response.value, IndexedResourceValue::Range(range)
                            if range.request_complete && !range.items.is_empty()
                                && matches!(range.cursor.request,
                                    index::Request::Code { lines } if lines.start == top - 2))
                        })
                })
                .unwrap();
            if settled {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(20))
                .await;
        }
        assert!(
            settled,
            "late Code scrolling must receive its provider-backed range"
        );
        panel
            .read_with(cx, |panel, _| {
                let anchor = panel.code_anchor_for_test().unwrap();
                assert_eq!(anchor.position, "```rust\n".len() as u64 + top * line_bytes);
                assert_eq!(anchor.visual_offset, -11);
                let presentation = panel.code_presentation_for_test().unwrap();
                assert_eq!(presentation.0, identity);
                assert_eq!(presentation.2, 10.5);
                let diagnostics = panel.diagnostic_snapshot();
                let code = &diagnostics.code_panels[0];
                assert_eq!(code.key, key);
                assert_eq!(code.anchor_visual_offset, Some(-10.5));
                assert!(code.realized_visual_lines <= 16);
                assert_eq!(
                    panel
                        .measured_frame_for_test()
                        .unwrap()
                        .manual_scroll_total_px,
                    outer
                );
            })
            .unwrap();
    }
}
