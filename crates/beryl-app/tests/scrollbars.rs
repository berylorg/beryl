use gpui::px;
use gpui_scrollbar::{
    ScrollbarAxisHit, ScrollbarGeometryStyle, classify_scrollbar_axis_hit,
    scroll_offset_from_thumb_drag, scrollbar_metrics, scrollbar_track_geometry,
};

fn geometry_style() -> ScrollbarGeometryStyle {
    ScrollbarGeometryStyle::default()
}

#[test]
fn scrollbar_metrics_hide_when_content_fits() {
    assert_eq!(
        scrollbar_metrics(geometry_style(), px(240.0), px(0.0), px(0.0)),
        None
    );
}

#[test]
fn scrollbar_metrics_move_the_thumb_as_scroll_advances() {
    let top = scrollbar_metrics(geometry_style(), px(240.0), px(240.0), px(0.0))
        .expect("overflow should produce a visible scrollbar");
    let middle = scrollbar_metrics(geometry_style(), px(240.0), px(240.0), px(120.0))
        .expect("overflow should produce a visible scrollbar");

    assert_eq!(top.thumb_length, middle.thumb_length);
    assert!(middle.thumb_offset > top.thumb_offset);
}

#[test]
fn scrollbar_metrics_clamp_scroll_offset_to_the_track() {
    let clamped = scrollbar_metrics(geometry_style(), px(200.0), px(100.0), px(999.0))
        .expect("overflow should produce a visible scrollbar");
    let maxed = scrollbar_metrics(geometry_style(), px(200.0), px(100.0), px(100.0))
        .expect("overflow should produce a visible scrollbar");

    assert_eq!(clamped, maxed);
}

#[test]
fn scrollbar_hit_classification_uses_current_thumb_bounds() {
    let metrics = scrollbar_metrics(geometry_style(), px(240.0), px(240.0), px(120.0))
        .expect("overflow should produce a visible scrollbar");
    let geometry = scrollbar_track_geometry(geometry_style(), px(240.0), metrics)
        .expect("visible scrollbar should have track geometry");

    assert_eq!(
        classify_scrollbar_axis_hit(
            geometry_style(),
            px(240.0),
            metrics,
            geometry.thumb_start - px(1.0),
        ),
        Some(ScrollbarAxisHit::LaneBeforeThumb)
    );
    assert_eq!(
        classify_scrollbar_axis_hit(
            geometry_style(),
            px(240.0),
            metrics,
            geometry.thumb_start + px(1.0),
        ),
        Some(ScrollbarAxisHit::Thumb)
    );
    assert_eq!(
        classify_scrollbar_axis_hit(
            geometry_style(),
            px(240.0),
            metrics,
            geometry.thumb_end + px(1.0),
        ),
        Some(ScrollbarAxisHit::LaneAfterThumb)
    );
}

#[test]
fn scrollbar_drag_mapping_preserves_pointer_grab_offset() {
    let metrics = scrollbar_metrics(geometry_style(), px(240.0), px(240.0), px(0.0))
        .expect("overflow should produce a visible scrollbar");
    let geometry = scrollbar_track_geometry(geometry_style(), px(240.0), metrics)
        .expect("visible scrollbar should have track geometry");
    let grab_offset = px(10.0);
    let pointer = geometry.track_start + px(57.0) + grab_offset;

    assert_eq!(
        scroll_offset_from_thumb_drag(
            geometry_style(),
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
    let metrics = scrollbar_metrics(geometry_style(), px(240.0), px(240.0), px(0.0))
        .expect("overflow should produce a visible scrollbar");

    assert_eq!(
        scroll_offset_from_thumb_drag(
            geometry_style(),
            px(240.0),
            px(240.0),
            metrics,
            px(-100.0),
            px(8.0),
        ),
        Some(px(0.0))
    );
    assert_eq!(
        scroll_offset_from_thumb_drag(
            geometry_style(),
            px(240.0),
            px(240.0),
            metrics,
            px(999.0),
            px(8.0),
        ),
        Some(px(240.0))
    );
}
