#[path = "../src/shell/render/scrollbars.rs"]
mod scrollbars;

use gpui::px;

#[test]
fn scrollbar_metrics_hide_when_content_fits() {
    assert_eq!(
        scrollbars::scrollbar_metrics(px(240.0), px(0.0), px(0.0)),
        None
    );
}

#[test]
fn scrollbar_metrics_move_the_thumb_as_scroll_advances() {
    let top = scrollbars::scrollbar_metrics(px(240.0), px(240.0), px(0.0))
        .expect("overflow should produce a visible scrollbar");
    let middle = scrollbars::scrollbar_metrics(px(240.0), px(240.0), px(120.0))
        .expect("overflow should produce a visible scrollbar");

    assert_eq!(top.thumb_length, middle.thumb_length);
    assert!(middle.thumb_offset > top.thumb_offset);
}

#[test]
fn scrollbar_metrics_clamp_scroll_offset_to_the_track() {
    let clamped = scrollbars::scrollbar_metrics(px(200.0), px(100.0), px(999.0))
        .expect("overflow should produce a visible scrollbar");
    let maxed = scrollbars::scrollbar_metrics(px(200.0), px(100.0), px(100.0))
        .expect("overflow should produce a visible scrollbar");

    assert_eq!(clamped, maxed);
}
