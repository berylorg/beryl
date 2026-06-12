#[allow(dead_code)]
#[path = "../src/shell/virtual_list/mod.rs"]
mod virtual_list;

use gpui::{point, px};
use virtual_list::{
    ListAlignment, ListContentAnchorResizePolicy, ListOffset, ListScrollPosition, ListState,
    test_support,
};

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
fn clamped_scroll_delta_does_not_require_view_notification() {
    let state = ListState::new(3, ListAlignment::Top, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(100.0), px(100.0), px(100.0)]);
    test_support::set_viewport_height(&state, px(250.0));

    let should_notify =
        test_support::apply_scroll_delta_should_notify_view(&state, point(px(0.0), px(20.0)));

    assert_eq!(should_notify, Some(false));
    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        })
    );
    assert_eq!(test_support::visible_range(&state), 0..3);
}

#[test]
fn offset_only_scroll_delta_requires_view_notification() {
    let state = ListState::new(3, ListAlignment::Top, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(100.0), px(100.0), px(100.0)]);
    test_support::set_viewport_height(&state, px(250.0));

    let should_notify =
        test_support::apply_scroll_delta_should_notify_view(&state, point(px(0.0), px(-10.0)));

    assert_eq!(should_notify, Some(true));
    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 0,
            offset_in_item: px(10.0),
        })
    );
    assert_eq!(test_support::visible_range(&state), 0..3);
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
fn virtual_tail_visible_range_clamps_after_middle_row_removal() {
    let state = ListState::new(3, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(20.0), px(20.0), px(20.0)]);
    test_support::set_viewport_height(&state, px(40.0));
    state.set_virtual_trailing_scroll_allowance(px(30.0));
    state.scroll_to_position(ListScrollPosition::VirtualTail {
        offset_from_content_end: px(30.0),
    });

    state.splice(1..2, 0);

    assert_eq!(state.item_count(), 2);
    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::VirtualTail {
            offset_from_content_end: px(30.0)
        }
    );
    assert_eq!(test_support::visible_range(&state), 1..2);
}

#[test]
fn page_scroll_by_viewport_enters_virtual_tail_without_fake_items() {
    let state = ListState::new(3, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(20.0), px(20.0), px(20.0)]);
    test_support::set_viewport_height(&state, px(40.0));
    state.set_virtual_trailing_scroll_allowance(px(30.0));

    state.scroll_by(px(40.0));

    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::VirtualTail {
            offset_from_content_end: px(30.0)
        }
    );
    assert_eq!(state.item_count(), 3);
    assert_eq!(test_support::visible_range(&state), 2..3);
}

#[test]
fn page_scroll_up_from_bottom_starts_at_real_viewport_top() {
    let state = ListState::new(5, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(
        &state,
        &[px(100.0), px(100.0), px(100.0), px(100.0), px(100.0)],
    );
    test_support::set_viewport_height(&state, px(200.0));

    state.scroll_by(px(-200.0));

    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 1,
            offset_in_item: px(0.0),
        })
    );
    assert_eq!(test_support::visible_range(&state), 1..3);
}

#[test]
fn scrollbar_drag_keeps_scrollbar_height_stable_until_ended() {
    let state = ListState::new(3, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(20.0), px(20.0), px(20.0)]);
    test_support::set_viewport_height(&state, px(40.0));
    state.set_virtual_trailing_scroll_allowance(px(30.0));
    let max_before_drag = state.max_offset_for_scrollbar();

    state.scrollbar_drag_started();
    state.set_virtual_trailing_scroll_allowance(px(80.0));

    assert_eq!(state.max_offset_for_scrollbar(), max_before_drag);

    state.scrollbar_drag_ended();

    assert_eq!(state.max_offset_for_scrollbar().height, px(100.0));
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
fn retained_final_runway_manual_scroll_reaches_guarded_anchor_offset() {
    let state = ListState::new(1, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(168.0)]);
    test_support::set_viewport_height(&state, px(240.0));
    state.set_virtual_trailing_scroll_allowance(px(192.0));

    state.scroll_by(px(500.0));

    assert_eq!(state.max_offset_for_scrollbar().height, px(120.0));
    assert_eq!(
        state.logical_scroll_top(),
        ListOffset {
            item_ix: 0,
            offset_in_item: px(120.0),
        }
    );
    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::VirtualTail {
            offset_from_content_end: px(120.0),
        }
    );
    assert_eq!(
        test_support::visible_virtual_trailing_height(&state),
        px(192.0)
    );
}

#[test]
fn retained_final_runway_manual_scroll_reaches_scaled_guarded_anchor_offset() {
    let state = ListState::new(1, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(208.0)]);
    test_support::set_viewport_height(&state, px(240.0));
    state.set_virtual_trailing_scroll_allowance(px(140.0));

    state.scroll_by(px(500.0));

    assert_eq!(state.max_offset_for_scrollbar().height, px(108.0));
    assert_eq!(
        state.logical_scroll_top(),
        ListOffset {
            item_ix: 0,
            offset_in_item: px(108.0),
        }
    );
    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::VirtualTail {
            offset_from_content_end: px(108.0)
        }
    );
    assert_eq!(
        test_support::visible_virtual_trailing_height(&state),
        px(140.0)
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
fn current_item_height_growth_can_preserve_anchor_offset() {
    let state = ListState::new(4, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(100.0), px(200.0), px(100.0), px(100.0)]);
    test_support::set_viewport_height(&state, px(200.0));
    state.scroll_to(ListOffset {
        item_ix: 1,
        offset_in_item: px(150.0),
    });
    state.set_content_anchor_resize_policy(ListContentAnchorResizePolicy::PreserveAnchorOffset);

    let adjusted = test_support::apply_item_height_change_to_content_anchor(&state, 1, px(400.0));

    assert_eq!(adjusted, None);
    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 1,
            offset_in_item: px(150.0),
        })
    );
}

#[test]
fn preserved_content_anchor_extends_runway_when_viewport_grows() {
    let state = ListState::new(2, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(500.0), px(80.0)]);
    test_support::set_viewport_height(&state, px(200.0));
    state.scroll_to(ListOffset {
        item_ix: 1,
        offset_in_item: px(0.0),
    });
    state.set_virtual_trailing_scroll_allowance(px(120.0));
    state.set_content_anchor_resize_policy(ListContentAnchorResizePolicy::PreserveAnchorOffset);

    let extra = test_support::extend_virtual_trailing_height_for_preserved_anchor(
        &state,
        px(320.0),
        ListOffset {
            item_ix: 1,
            offset_in_item: px(0.0),
        },
        px(200.0),
    );

    assert_eq!(extra, Some(px(120.0)));
    assert_eq!(state.virtual_trailing_scroll_allowance(), px(240.0));
    assert_eq!(
        state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 1,
            offset_in_item: px(0.0),
        })
    );
}

#[test]
fn ordinary_content_anchor_does_not_extend_runway_when_viewport_grows() {
    let state = ListState::new(2, ListAlignment::Bottom, px(10.0));
    test_support::set_measured_item_heights(&state, &[px(500.0), px(80.0)]);
    test_support::set_viewport_height(&state, px(200.0));
    state.scroll_to(ListOffset {
        item_ix: 1,
        offset_in_item: px(0.0),
    });
    state.set_virtual_trailing_scroll_allowance(px(120.0));

    let extra = test_support::extend_virtual_trailing_height_for_preserved_anchor(
        &state,
        px(320.0),
        ListOffset {
            item_ix: 1,
            offset_in_item: px(0.0),
        },
        px(200.0),
    );

    assert_eq!(extra, None);
    assert_eq!(state.virtual_trailing_scroll_allowance(), px(120.0));
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

#[test]
fn scroll_handler_dispatch_runs_after_list_state_borrow_is_released() {
    let state_source = include_str!("../src/shell/virtual_list/state.rs");
    let scroll_state_source = include_str!("../src/shell/virtual_list/scroll_state.rs");
    let element_source = include_str!("../src/shell/virtual_list/element.rs");

    let list_state_scroll_body = rust_function_body(state_source, "pub(super) fn scroll(");
    let state_inner_scroll_body = rust_function_body(scroll_state_source, "pub(super) fn scroll(");

    assert!(element_source.contains("list_state.scroll("));
    assert!(!element_source.contains("list_state.0.borrow_mut().scroll("));

    assert!(list_state_scroll_body.contains("let (event, mut handler, should_notify_view) = {"));
    assert!(list_state_scroll_body.contains("let mut state = self.0.borrow_mut();"));
    assert!(
        list_state_scroll_body.contains("let previous_scroll_position = state.scroll_position();")
    );
    assert!(
        list_state_scroll_body
            .contains("let previous_visible_range = state.current_visible_range();")
    );
    assert!(list_state_scroll_body.contains("state.scroll(scroll_top, height, delta)"));
    assert!(list_state_scroll_body.contains("state.scroll_handler.take()"));
    assert!(list_state_scroll_body.contains("if should_notify_view {"));
    assert_order(
        list_state_scroll_body,
        "state.scroll_handler.take()",
        "handler(&event",
    );
    assert_order(
        list_state_scroll_body,
        "let Some(event) = event else",
        "handler(&event",
    );

    assert!(!state_inner_scroll_body.contains("scroll_handler"));
    assert!(!state_inner_scroll_body.contains("window"));
    assert!(!state_inner_scroll_body.contains("cx"));
}

#[test]
fn list_row_render_context_is_layout_owned_and_refreshed_after_bottom_up_fill() {
    let mod_source = include_str!("../src/shell/virtual_list/mod.rs");
    let layout_source = include_str!("../src/shell/virtual_list/layout_state.rs");
    let layout_body = rust_function_body(layout_source, "pub(super) fn layout_items");
    let refresh_body = rust_function_body(layout_source, "fn refresh_item_layout_render_contexts");
    let replace_size_body = rust_function_body(layout_source, "fn replace_measured_item_size");
    let context_body = rust_function_body(layout_source, "fn list_item_render_context");

    assert!(mod_source.contains("pub struct ListItemRenderContext"));
    assert!(mod_source.contains("dyn FnMut(usize, ListItemRenderContext"));
    assert!(
        layout_body.contains("list_item_render_context(&scroll_top, item_index, available_height)")
    );
    assert!(layout_body.contains("self.refresh_item_layout_render_contexts("));
    assert_order(
        layout_body,
        "scroll_top = ListOffset",
        "self.refresh_item_layout_render_contexts(",
    );
    assert!(refresh_body.contains("render_item(item_layout.index, render_context"));
    assert!(refresh_body.contains("let old_size = item_layout.size"));
    assert!(refresh_body.contains("if size == old_size"));
    assert!(refresh_body.contains("self.replace_measured_item_size(item_layout.index, size)"));
    assert!(
        refresh_body.contains("preserve_bottom_offset && item_layout.index == scroll_top.item_ix")
    );
    assert!(refresh_body.contains("scroll_top.offset_in_item ="));
    assert!(!refresh_body.contains("SumTree::from_iter"));
    assert!(replace_size_body.contains("old_items.slice(&Count(index), Bias::Right)"));
    assert!(replace_size_body.contains("new_items.extend("));
    assert!(replace_size_body.contains("new_items.append(old_items.suffix(), ())"));
    assert!(context_body.contains("scroll_top.item_ix == item_index"));
    assert!(context_body.contains("scroll_top.offset_in_item.max(px(0.0))"));
}

fn rust_function_body<'a>(source: &'a str, function_signature: &str) -> &'a str {
    let signature_index = source
        .find(function_signature)
        .unwrap_or_else(|| panic!("missing function {function_signature}"));
    let after_signature = &source[signature_index..];
    let open_offset = after_signature
        .find('{')
        .unwrap_or_else(|| panic!("missing body for function {function_signature}"));
    let body_start = signature_index + open_offset;
    let mut depth = 0usize;

    for (offset, character) in source[body_start..].char_indices() {
        match character {
            '{' => depth = depth.saturating_add(1),
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return &source[body_start..body_start + offset + character.len_utf8()];
                }
            }
            _ => {}
        }
    }

    panic!("unterminated body for function {function_signature}");
}

fn assert_order(source: &str, before: &str, after: &str) {
    let before_index = source
        .find(before)
        .unwrap_or_else(|| panic!("missing {before:?}"));
    let after_index = source
        .find(after)
        .unwrap_or_else(|| panic!("missing {after:?}"));
    assert!(
        before_index < after_index,
        "expected {before:?} to appear before {after:?}"
    );
}
