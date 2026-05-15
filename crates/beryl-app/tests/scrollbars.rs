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

#[test]
fn scrollbar_hit_classification_uses_current_thumb_bounds() {
    let metrics = scrollbars::scrollbar_metrics(px(240.0), px(240.0), px(120.0))
        .expect("overflow should produce a visible scrollbar");
    let geometry = scrollbars::scrollbar_track_geometry(px(240.0), metrics)
        .expect("visible scrollbar should have track geometry");

    assert_eq!(
        scrollbars::classify_scrollbar_axis_hit(px(240.0), metrics, geometry.thumb_start - px(1.0),),
        Some(scrollbars::ScrollbarAxisHit::LaneBeforeThumb)
    );
    assert_eq!(
        scrollbars::classify_scrollbar_axis_hit(px(240.0), metrics, geometry.thumb_start + px(1.0),),
        Some(scrollbars::ScrollbarAxisHit::Thumb)
    );
    assert_eq!(
        scrollbars::classify_scrollbar_axis_hit(px(240.0), metrics, geometry.thumb_end + px(1.0),),
        Some(scrollbars::ScrollbarAxisHit::LaneAfterThumb)
    );
}

#[test]
fn scrollbar_drag_mapping_preserves_pointer_grab_offset() {
    let metrics = scrollbars::scrollbar_metrics(px(240.0), px(240.0), px(0.0))
        .expect("overflow should produce a visible scrollbar");
    let geometry = scrollbars::scrollbar_track_geometry(px(240.0), metrics)
        .expect("visible scrollbar should have track geometry");
    let grab_offset = px(10.0);
    let pointer = geometry.track_start + px(57.0) + grab_offset;

    assert_eq!(
        scrollbars::scroll_offset_from_thumb_drag(
            px(240.0),
            px(240.0),
            metrics,
            pointer,
            grab_offset,
        ),
        Some(px(120.0))
    );
}

#[test]
fn scrollbar_drag_mapping_clamps_to_edges() {
    let metrics = scrollbars::scrollbar_metrics(px(240.0), px(240.0), px(0.0))
        .expect("overflow should produce a visible scrollbar");

    assert_eq!(
        scrollbars::scroll_offset_from_thumb_drag(
            px(240.0),
            px(240.0),
            metrics,
            px(-100.0),
            px(8.0),
        ),
        Some(px(0.0))
    );
    assert_eq!(
        scrollbars::scroll_offset_from_thumb_drag(
            px(240.0),
            px(240.0),
            metrics,
            px(999.0),
            px(8.0),
        ),
        Some(px(240.0))
    );
}
