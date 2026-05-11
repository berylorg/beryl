#[path = "../src/shell/image_preview_popup.rs"]
mod image_preview_popup;

use gpui::{Bounds, point, px, size};

#[test]
fn preview_popup_dimensions_are_bounded() {
    assert_eq!(image_preview_popup::popup_width(), px(560.0));
    assert_eq!(image_preview_popup::popup_height(), px(380.0));
}

#[test]
fn preview_popup_dismisses_only_outside_container() {
    let bounds = Bounds::new(point(px(80.0), px(120.0)), size(px(560.0), px(380.0)));

    assert!(!image_preview_popup::should_dismiss_for_mouse_down(
        Some(bounds),
        point(px(100.0), px(140.0)),
    ));
    assert!(image_preview_popup::should_dismiss_for_mouse_down(
        Some(bounds),
        point(px(40.0), px(140.0)),
    ));
    assert!(image_preview_popup::should_dismiss_for_mouse_down(
        None,
        point(px(100.0), px(140.0)),
    ));
}
