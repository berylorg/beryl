use crate::syndic_transcript::SyndicTranscriptPanel;
use gpui::{App, Entity, PlatformInput, Window, point, px};

pub(super) fn route_wheel(
    panel: &Entity<SyndicTranscriptPanel>,
    window: &mut Window,
    app: &mut App,
    selected: bool,
) {
    let (_, bounds, top, ownership) = panel.read(app).code_viewport_for_test().unwrap();
    assert_eq!(ownership, selected);
    let (identity, _, remainder) = panel.read(app).code_presentation_for_test().unwrap();
    assert_eq!(panel.read(app).code_horizontal_offset_for_test(), Some(0.0));
    let outer = panel
        .read(app)
        .measured_frame_for_test()
        .unwrap()
        .manual_scroll_total_px;
    let key = panel.read(app).diagnostic_snapshot().code_panels[0].key;
    let vertical = if selected { -20.0 } else { 0.0 };
    for (x, y, expected_x, expected_top) in [
        (-45.5, 0.0, 45.5, top),
        (-10.25, vertical, 55.75, top + u64::from(selected)),
        (10.25, -vertical, 45.5, top),
        (100.0, 0.0, 0.0, top),
        (10.0, 0.0, 0.0, top),
    ] {
        window.dispatch_input_for_test(
            PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                position: bounds.center(),
                delta: gpui::ScrollDelta::Pixels(point(px(x), px(y))),
                modifiers: Default::default(),
                touch_phase: gpui::TouchPhase::Moved,
            }),
            app,
        );
        window.draw_and_present_for_test(app);
        let panel = panel.read(app);
        let (actual_id, _, actual_remainder) = panel.code_presentation_for_test().unwrap();
        let (_, _, actual_top, actual_selected) = panel.code_viewport_for_test().unwrap();
        assert_eq!(actual_id, identity);
        assert_eq!(panel.code_horizontal_offset_for_test(), Some(expected_x));
        assert_eq!(actual_top, expected_top);
        assert_eq!(actual_remainder, remainder);
        assert_eq!(actual_selected, selected);
        assert_eq!(
            panel
                .measured_frame_for_test()
                .unwrap()
                .manual_scroll_total_px,
            outer
        );
        let diagnostics = panel.diagnostic_snapshot();
        assert_eq!(diagnostics.code_panels[0].key, key);
        assert!(diagnostics.code_panels[0].realized_visual_lines <= 16);
    }
}
