use crate::syndic_transcript::SyndicTranscriptPanel;
use gpui::{App, Entity, Focusable, MouseButton, Pixels, PlatformInput, Point, Window, point, px};

fn dispatch(window: &mut Window, app: &mut App, input: PlatformInput) {
    window.dispatch_input_for_test(input, app);
    window.draw_and_present_for_test(app);
}

fn move_pointer(window: &mut Window, app: &mut App, position: Point<Pixels>, pressed: bool) {
    dispatch(
        window,
        app,
        PlatformInput::MouseMove(gpui::MouseMoveEvent {
            position,
            pressed_button: pressed.then_some(MouseButton::Left),
            modifiers: Default::default(),
        }),
    );
}

pub(super) fn press(window: &mut Window, app: &mut App, position: Point<Pixels>) {
    dispatch(
        window,
        app,
        PlatformInput::MouseDown(gpui::MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }),
    );
}

pub(super) fn release(window: &mut Window, app: &mut App, position: Point<Pixels>) {
    dispatch(
        window,
        app,
        PlatformInput::MouseUp(gpui::MouseUpEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }),
    );
}

pub(super) fn wheel(window: &mut Window, app: &mut App, position: Point<Pixels>) {
    dispatch(
        window,
        app,
        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-20.0))),
            modifiers: Default::default(),
            touch_phase: gpui::TouchPhase::Moved,
        }),
    );
}

pub(super) fn hover_without_acquiring(
    panel: &Entity<SyndicTranscriptPanel>,
    window: &mut Window,
    app: &mut App,
) {
    let (_, bounds, top, selected) = panel.read(app).code_viewport_for_test().unwrap();
    assert!(!selected);
    move_pointer(window, app, bounds.center(), false);
    wheel(window, app, bounds.center());
    let (_, _, after, selected) = panel.read(app).code_viewport_for_test().unwrap();
    assert_eq!(after, top, "unselected wheel must not scroll Code");
    assert!(!selected, "hover and wheel must not acquire Code");
}

pub(super) fn scrollbar_acquisition_drag_and_focus(
    panel: &Entity<SyndicTranscriptPanel>,
    window: &mut Window,
    app: &mut App,
) {
    let (_, bounds, top, selected) = panel.read(app).code_viewport_for_test().unwrap();
    assert_eq!(top, 0);
    assert!(!selected);
    let before = panel
        .read(app)
        .measured_frame_for_test()
        .unwrap()
        .manual_scroll_total_px;
    let geometry = panel.read(app).code_scrollbar_for_test().unwrap();
    let lane = point(
        bounds.origin.x + bounds.size.width - px(8.0),
        bounds.origin.y + (geometry.thumb_bounds.end + geometry.track_bounds.end) / 2.0,
    );
    press(window, app, lane);
    release(window, app, lane);
    let (_, _, top, selected) = panel.read(app).code_viewport_for_test().unwrap();
    assert_eq!(top, 12, "lane press requests one viewport page");
    assert!(selected, "lane interaction acquires Code wheel ownership");
    let focus = panel.read(app).focus_handle(app);
    window.focus(&focus);
    window.draw_and_present_for_test(app);
    wheel(window, app, bounds.center());
    let (_, _, top, selected) = panel.read(app).code_viewport_for_test().unwrap();
    assert_eq!(top, 13, "focus movement preserves inner wheel routing");
    assert!(selected);
    let outer = panel.read(app).viewport_bounds_for_test().unwrap();
    let outside = outer.origin + point(outer.size.width - px(2.0), outer.size.height - px(2.0));
    assert!(!bounds.contains(&outside));
    press(window, app, outside);
    release(window, app, outside);
    assert!(!panel.read(app).code_viewport_for_test().unwrap().3);
    let geometry = panel.read(app).code_scrollbar_for_test().unwrap();
    let thumb = point(
        lane.x,
        bounds.origin.y + (geometry.thumb_bounds.start + geometry.thumb_bounds.end) / 2.0,
    );
    move_pointer(window, app, thumb, false);
    press(window, app, thumb);
    move_pointer(window, app, thumb + point(px(0.0), px(5.0)), true);
    let destination = thumb + point(px(0.0), px(35.0));
    move_pointer(window, app, destination, true);
    release(window, app, destination);
    let (_, _, dragged, selected) = panel.read(app).code_viewport_for_test().unwrap();
    assert!(dragged > top, "native thumb drag moves the inner viewport");
    assert!(selected, "drag release preserves wheel ownership");
    move_pointer(window, app, destination + point(px(0.0), px(20.0)), false);
    assert_eq!(
        panel.read(app).code_viewport_for_test().unwrap().2,
        dragged,
        "movement after release must not continue the drag"
    );
    assert_eq!(
        panel
            .read(app)
            .measured_frame_for_test()
            .unwrap()
            .manual_scroll_total_px,
        before,
        "scrollbar interaction and focus movement must not scroll the transcript"
    );
}
