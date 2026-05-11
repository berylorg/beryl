#[allow(dead_code)]
#[path = "../src/shell/virtual_list/mod.rs"]
mod virtual_list;

use gpui::{point, px};
use virtual_list::{ListAlignment, ListOffset, ListScrollPosition, ListState, test_support};

#[test]
fn virtual_allowance_extends_scrollbar_range_without_changing_item_count() {
    let state = ListState::new(3, ListAlignment::Top, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(20.0), px(20.0), px(20.0)]);
    test_support::set_viewport_height(&state, px(40.0));

    state.set_virtual_trailing_scroll_allowance(px(30.0));

    assert_eq!(state.item_count(), 3);
    assert_eq!(state.virtual_trailing_scroll_allowance(), px(30.0));
    assert_eq!(state.max_offset_for_scrollbar().height, px(50.0));
    assert_eq!(state.scroll_px_offset_for_scrollbar().y, px(0.0));
}

#[test]
fn bottom_following_stays_at_real_content_end_with_virtual_tail_available() {
    let state = ListState::new(3, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(20.0), px(20.0), px(20.0)]);
    test_support::set_viewport_height(&state, px(40.0));
    state.set_virtual_trailing_scroll_allowance(px(30.0));

    assert_eq!(state.scroll_position(), ListScrollPosition::Bottom);
    assert_eq!(state.max_offset_for_scrollbar().height, px(50.0));
    assert_eq!(state.scroll_px_offset_for_scrollbar().y, px(-20.0));
}

#[test]
fn virtual_tail_position_clamps_to_current_allowance() {
    let state = ListState::new(3, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(20.0), px(20.0), px(20.0)]);
    test_support::set_viewport_height(&state, px(40.0));
    state.set_virtual_trailing_scroll_allowance(px(10.0));

    state.scroll_to_position(ListScrollPosition::VirtualTail {
        offset_from_content_end: px(50.0),
    });
    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::VirtualTail {
            offset_from_content_end: px(10.0)
        }
    );

    state.set_virtual_trailing_scroll_allowance(px(5.0));
    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::VirtualTail {
            offset_from_content_end: px(5.0)
        }
    );
}

#[test]
fn visible_range_remains_content_only_inside_virtual_tail() {
    let state = ListState::new(3, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(20.0), px(20.0), px(20.0)]);
    test_support::set_viewport_height(&state, px(40.0));
    state.set_virtual_trailing_scroll_allowance(px(30.0));

    state.set_offset_from_scrollbar(point(px(0.0), px(-50.0)));

    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::VirtualTail {
            offset_from_content_end: px(30.0)
        }
    );
    assert_eq!(test_support::visible_range(&state), 2..3);
    assert_eq!(state.item_count(), 3);
}

#[test]
fn short_content_scroll_to_real_start_preserves_virtual_tail_intent() {
    let state = ListState::new(1, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(80.0)]);
    test_support::set_viewport_height(&state, px(200.0));
    state.set_virtual_trailing_scroll_allowance(px(160.0));
    state.scroll_to(ListOffset {
        item_ix: 0,
        offset_in_item: px(40.0),
    });

    state.scroll_by(px(-40.0));

    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        })
    );
    assert_eq!(
        test_support::visible_virtual_trailing_height(&state),
        px(120.0)
    );
}

#[test]
fn production_visible_range_uses_bottom_following_geometry() {
    let state = ListState::new(3, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(20.0), px(20.0), px(20.0)]);
    test_support::set_viewport_height(&state, px(40.0));

    assert_eq!(state.visible_range(), 1..3);
}

#[test]
fn presentation_range_includes_overdraw_and_clamps_to_real_items() {
    let state = ListState::new(5, ListAlignment::Top, px(20.0));
    test_support::set_measured_item_heights(
        &state,
        &[px(20.0), px(20.0), px(20.0), px(20.0), px(20.0)],
    );
    test_support::set_viewport_height(&state, px(40.0));
    state.scroll_to(ListOffset {
        item_ix: 2,
        offset_in_item: px(0.0),
    });

    assert_eq!(state.visible_range(), 2..4);
    assert_eq!(state.presentation_range(), 1..5);
    assert_eq!(test_support::presentation_range(&state), 1..5);
}

#[test]
fn layout_uses_only_visible_virtual_tail_height() {
    let state = ListState::new(3, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(100.0), px(100.0), px(100.0)]);
    test_support::set_viewport_height(&state, px(250.0));
    state.set_virtual_trailing_scroll_allowance(px(180.0));

    state.scroll_to(ListOffset {
        item_ix: 1,
        offset_in_item: px(0.0),
    });

    assert_eq!(
        test_support::visible_virtual_trailing_height(&state),
        px(50.0)
    );
}

#[test]
fn virtual_tail_clamped_to_zero_preserves_manual_non_following_position() {
    let state = ListState::new(3, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(20.0), px(20.0), px(20.0)]);
    test_support::set_viewport_height(&state, px(40.0));
    state.set_virtual_trailing_scroll_allowance(px(10.0));
    state.scroll_to_position(ListScrollPosition::VirtualTail {
        offset_from_content_end: px(10.0),
    });

    state.set_virtual_trailing_scroll_allowance(px(0.0));

    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 1,
            offset_in_item: px(0.0),
        })
    );
}

#[test]
fn invalidating_item_measurement_keeps_cached_scroll_geometry_until_remeasure() {
    let state = ListState::new(4, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(100.0), px(400.0), px(100.0), px(100.0)]);
    test_support::set_viewport_height(&state, px(200.0));
    state.scroll_to(ListOffset {
        item_ix: 2,
        offset_in_item: px(0.0),
    });
    let max_before = state.max_offset_for_scrollbar();
    let offset_before = state.scroll_px_offset_for_scrollbar();

    test_support::invalidate_item_measurement(&state, 1);

    assert_eq!(state.measured_item_size(1).unwrap().height, px(400.0));
    assert_eq!(state.max_offset_for_scrollbar(), max_before);
    assert_eq!(state.scroll_px_offset_for_scrollbar(), offset_before);
    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 2,
            offset_in_item: px(0.0),
        })
    );
}

#[test]
fn current_item_height_shrink_preserves_following_content_anchor() {
    let state = ListState::new(4, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(100.0), px(400.0), px(100.0), px(100.0)]);
    test_support::set_viewport_height(&state, px(200.0));
    state.scroll_to(ListOffset {
        item_ix: 1,
        offset_in_item: px(350.0),
    });

    let adjusted =
        test_support::apply_item_height_change_to_content_anchor(&state, 1, px(200.0)).unwrap();

    assert_eq!(
        adjusted,
        ListOffset {
            item_ix: 1,
            offset_in_item: px(150.0),
        }
    );
    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 1,
            offset_in_item: px(150.0),
        })
    );
}

#[test]
fn current_item_height_growth_preserves_following_content_anchor() {
    let state = ListState::new(4, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(100.0), px(200.0), px(100.0), px(100.0)]);
    test_support::set_viewport_height(&state, px(200.0));
    state.scroll_to(ListOffset {
        item_ix: 1,
        offset_in_item: px(150.0),
    });

    let adjusted =
        test_support::apply_item_height_change_to_content_anchor(&state, 1, px(400.0)).unwrap();

    assert_eq!(
        adjusted,
        ListOffset {
            item_ix: 1,
            offset_in_item: px(350.0),
        }
    );
}

#[test]
fn current_item_height_change_preserves_bottom_and_virtual_tail_intent() {
    let bottom = ListState::new(3, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&bottom, &[px(100.0), px(200.0), px(100.0)]);
    test_support::set_viewport_height(&bottom, px(200.0));
    assert_eq!(
        test_support::apply_item_height_change_to_content_anchor(&bottom, 1, px(400.0)),
        None
    );
    assert_eq!(bottom.scroll_position(), ListScrollPosition::Bottom);

    let virtual_tail = ListState::new(3, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&virtual_tail, &[px(100.0), px(200.0), px(100.0)]);
    test_support::set_viewport_height(&virtual_tail, px(200.0));
    virtual_tail.set_virtual_trailing_scroll_allowance(px(80.0));
    virtual_tail.scroll_to_position(ListScrollPosition::VirtualTail {
        offset_from_content_end: px(40.0),
    });

    assert_eq!(
        test_support::apply_item_height_change_to_content_anchor(&virtual_tail, 1, px(400.0)),
        None
    );
    assert_eq!(
        virtual_tail.scroll_position(),
        ListScrollPosition::VirtualTail {
            offset_from_content_end: px(40.0),
        }
    );
}
