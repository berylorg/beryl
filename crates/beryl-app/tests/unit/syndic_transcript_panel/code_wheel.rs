use super::*;

pub(super) fn wheel(
    cx: &mut TestAppContext,
    window: gpui::WindowHandle<NestedPanel>,
    position: gpui::Point<gpui::Pixels>,
    x: f32,
    y: f32,
) {
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_event(gpui::ScrollWheelEvent {
        position,
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(x), px(y))),
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    draw_frames(cx, window, 5);
}

fn position(
    cx: &mut TestAppContext,
    window: gpui::WindowHandle<NestedPanel>,
    panel: &Entity<SyndicTranscriptPanel>,
) -> gpui::Point<gpui::Pixels> {
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let outer = visual.debug_bounds("nested-transcript-body").unwrap();
    panel.read_with(cx, |panel, _| {
        let inner = panel.code_panels[0].viewport.scroll.borrow().bounds;
        gpui::point(
            outer.center().x,
            px((f32::from(outer.top()).max(f32::from(inner.top()))
                + f32::from(outer.bottom()).min(f32::from(inner.bottom())))
                / 2.0),
        )
    })
}

pub(super) fn outer_offset(cx: &TestAppContext, panel: &Entity<SyndicTranscriptPanel>) -> f32 {
    panel.read_with(cx, |panel, _| {
        panel
            .last_frame_window
            .as_ref()
            .unwrap()
            .manual_scroll_total_px
    })
}

#[gpui::test]
fn unselected_code_routes_vertical_wheel_to_overflowing_outer_viewport(cx: &mut TestAppContext) {
    let (window, panel) = mounted_code(cx);
    panel.read_with(cx, |panel, _| {
        let frame = panel.last_frame_window.as_ref().unwrap();
        assert!(frame.records[0].height_px > 180.0);
    });
    let position = position(cx, window, &panel);
    let before = outer_offset(cx, &panel);
    let top = panel.read_with(cx, |panel, _| {
        panel.last_frame_window.as_ref().unwrap().records[0].top_px
    });
    wheel(cx, window, position, 0.0, 25.5);
    assert_eq!(outer_offset(cx, &panel), before - 25.5);
    panel.read_with(cx, |panel, _| {
        assert_ne!(
            panel.last_frame_window.as_ref().unwrap().records[0].top_px,
            top
        );
        assert_eq!(panel.code_wheel_owner.get(), None);
        assert_eq!(panel.code_panels[0].viewport.scroll.borrow().offset(), 0.0);
    });
}

#[gpui::test]
fn code_horizontal_and_selected_edge_gestures_keep_one_vertical_owner(cx: &mut TestAppContext) {
    let (window, panel) = mounted_code(cx);
    let position = position(cx, window, &panel);
    let before = outer_offset(cx, &panel);
    wheel(cx, window, position, -45.5, 0.0);
    assert_eq!(outer_offset(cx, &panel), before);
    panel.read_with(cx, |panel, _| {
        let scroll = panel.code_panels[0].viewport.scroll.borrow();
        assert_eq!(scroll.horizontal, 45.5);
        assert_eq!(scroll.offset(), 0.0);
        assert_eq!(panel.code_wheel_owner.get(), None);
    });
    wheel(cx, window, position, -4.5, 20.0);
    assert_eq!(outer_offset(cx, &panel), before - 20.0);
    panel.read_with(cx, |panel, _| {
        assert_eq!(
            panel.code_panels[0].viewport.scroll.borrow().horizontal,
            50.0
        );
        assert_eq!(panel.code_wheel_owner.get(), None);
    });
    select_code(cx, window, &panel);
    let position = self::position(cx, window, &panel);
    let before = outer_offset(cx, &panel);
    wheel(cx, window, position, 100.0, 100.0);
    assert_eq!(outer_offset(cx, &panel), before);
    panel.read_with(cx, |panel, _| {
        let scroll = panel.code_panels[0].viewport.scroll.borrow();
        assert_eq!(scroll.offset(), 0.0);
        assert_eq!(scroll.horizontal, 0.0);
        assert_eq!(panel.code_wheel_owner.get(), Some(panel.code_panels[0].id));
    });
    panel.update(cx, |panel, _| {
        let mut scroll = panel.code_panels[0].viewport.scroll.borrow_mut();
        let end = scroll.overflow();
        scroll.set_offset(end);
    });
    draw_frames(cx, window, 5);
    wheel(cx, window, position, -12.5, -100.0);
    assert_eq!(outer_offset(cx, &panel), before);
    panel.read_with(cx, |panel, _| {
        let scroll = panel.code_panels[0].viewport.scroll.borrow();
        assert_eq!(scroll.offset(), scroll.overflow());
        assert_eq!(scroll.horizontal, 12.5);
        assert_eq!(panel.code_wheel_owner.get(), Some(panel.code_panels[0].id));
    });
}
