#[allow(dead_code)]
#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;

#[allow(dead_code)]
#[path = "../src/shell/transcript_anchor.rs"]
mod transcript_anchor;

#[allow(dead_code)]
#[path = "../src/shell/virtual_list/mod.rs"]
mod virtual_list;

#[allow(dead_code)]
#[path = "../src/shell/transcript_scroll.rs"]
mod transcript_scroll;

use gpui::px;
use transcript_scroll::{TranscriptTurnJumpDirection, transcript_turn_jump_target};
use virtual_list::{ListAlignment, ListOffset, ListScrollPosition, ListState, test_support};

fn list_offset(item_ix: usize, offset_in_item: gpui::Pixels) -> ListOffset {
    ListOffset {
        item_ix,
        offset_in_item,
    }
}

fn content_position(item_ix: usize, offset_in_item: gpui::Pixels) -> ListScrollPosition {
    ListScrollPosition::Content(list_offset(item_ix, offset_in_item))
}

fn measured_turn_list(turn_count: usize) -> ListState {
    let list_state = ListState::new(turn_count, ListAlignment::Bottom, px(320.0));
    let heights = vec![px(120.0); turn_count];
    test_support::set_measured_item_heights(&list_state, &heights);
    test_support::set_viewport_height(&list_state, px(80.0));
    list_state
}

#[test]
fn turn_jump_target_noops_for_empty_transcript() {
    let list_state = ListState::new(0, ListAlignment::Bottom, px(320.0));

    assert_eq!(
        transcript_turn_jump_target(&list_state, 0, TranscriptTurnJumpDirection::Up),
        None
    );
    assert_eq!(
        transcript_turn_jump_target(&list_state, 0, TranscriptTurnJumpDirection::Down),
        None
    );
}

#[test]
fn turn_jump_up_from_inside_turn_targets_current_turn_top() {
    let list_state = measured_turn_list(3);
    list_state.scroll_to(list_offset(1, px(48.0)));

    assert_eq!(
        transcript_turn_jump_target(&list_state, 3, TranscriptTurnJumpDirection::Up),
        Some(content_position(1, px(0.0)))
    );
}

#[test]
fn turn_jump_up_from_exact_boundary_targets_previous_turn() {
    let list_state = measured_turn_list(3);
    list_state.scroll_to(list_offset(2, px(0.0)));

    assert_eq!(
        transcript_turn_jump_target(&list_state, 3, TranscriptTurnJumpDirection::Up),
        Some(content_position(1, px(0.0)))
    );
}

#[test]
fn turn_jump_up_from_first_boundary_noops() {
    let list_state = measured_turn_list(3);
    list_state.scroll_to(list_offset(0, px(0.0)));

    assert_eq!(
        transcript_turn_jump_target(&list_state, 3, TranscriptTurnJumpDirection::Up),
        None
    );
}

#[test]
fn turn_jump_down_targets_next_turn_boundary() {
    let list_state = measured_turn_list(3);
    list_state.scroll_to(list_offset(1, px(32.0)));

    assert_eq!(
        transcript_turn_jump_target(&list_state, 3, TranscriptTurnJumpDirection::Down),
        Some(content_position(2, px(0.0)))
    );
}

#[test]
fn turn_jump_down_from_last_turn_targets_bottom() {
    let list_state = measured_turn_list(3);
    list_state.scroll_to(list_offset(2, px(0.0)));

    assert_eq!(
        transcript_turn_jump_target(&list_state, 3, TranscriptTurnJumpDirection::Down),
        Some(ListScrollPosition::Bottom)
    );
}

#[test]
fn turn_jump_down_from_inside_last_turn_targets_bottom() {
    let list_state = measured_turn_list(3);
    list_state.scroll_to(list_offset(2, px(32.0)));

    assert_eq!(
        transcript_turn_jump_target(&list_state, 3, TranscriptTurnJumpDirection::Down),
        Some(ListScrollPosition::Bottom)
    );
}

#[test]
fn turn_jump_down_from_bottom_noops() {
    let list_state = ListState::new(3, ListAlignment::Bottom, px(320.0));

    assert_eq!(
        transcript_turn_jump_target(&list_state, 3, TranscriptTurnJumpDirection::Down),
        None
    );
}

#[test]
fn turn_jump_up_from_bottom_targets_last_turn() {
    let list_state = ListState::new(3, ListAlignment::Bottom, px(320.0));

    assert_eq!(
        transcript_turn_jump_target(&list_state, 3, TranscriptTurnJumpDirection::Up),
        Some(content_position(2, px(0.0)))
    );
}

#[test]
fn turn_jump_virtual_tail_handles_real_turn_boundaries() {
    let list_state = ListState::new(3, ListAlignment::Bottom, px(320.0));
    test_support::set_measured_item_heights(&list_state, &[px(40.0), px(60.0), px(140.0)]);
    test_support::set_viewport_height(&list_state, px(120.0));
    list_state.set_virtual_trailing_scroll_allowance(px(80.0));
    list_state.scroll_to_position(ListScrollPosition::VirtualTail {
        offset_from_content_end: px(40.0),
    });

    assert_eq!(
        transcript_turn_jump_target(&list_state, 3, TranscriptTurnJumpDirection::Up),
        Some(content_position(2, px(0.0)))
    );
    assert_eq!(
        transcript_turn_jump_target(&list_state, 3, TranscriptTurnJumpDirection::Down),
        Some(ListScrollPosition::Bottom)
    );
}

#[test]
fn loaded_history_reset_keeps_bottom_intent_through_tail_allowance_clear() {
    let list_state = ListState::new(3, ListAlignment::Bottom, px(320.0));
    test_support::set_measured_item_heights(&list_state, &[px(120.0), px(120.0), px(120.0)]);
    test_support::set_viewport_height(&list_state, px(120.0));
    list_state.set_virtual_trailing_scroll_allowance(px(80.0));
    list_state.scroll_to_position(ListScrollPosition::VirtualTail {
        offset_from_content_end: px(40.0),
    });

    list_state.reset(1);
    test_support::set_measured_item_heights(&list_state, &[px(360.0)]);

    assert_eq!(list_state.item_count(), 1);
    assert_eq!(list_state.scroll_position(), ListScrollPosition::Bottom);
    assert_eq!(test_support::visible_range(&list_state), 0..1);

    list_state.set_virtual_trailing_scroll_allowance(px(0.0));

    assert_eq!(list_state.scroll_position(), ListScrollPosition::Bottom);
    assert_eq!(test_support::visible_range(&list_state), 0..1);
}

#[test]
fn middle_row_insert_preserves_content_anchor_on_same_logical_row() {
    let list_state = measured_turn_list(4);
    list_state.scroll_to(ListOffset {
        item_ix: 3,
        offset_in_item: px(48.0),
    });

    list_state.splice(1..1, 1);

    assert_eq!(list_state.item_count(), 5);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 4,
            offset_in_item: px(48.0),
        })
    );
}

#[test]
fn middle_row_remove_preserves_content_anchor_on_same_logical_row() {
    let list_state = measured_turn_list(5);
    list_state.scroll_to(ListOffset {
        item_ix: 4,
        offset_in_item: px(32.0),
    });

    list_state.splice(1..2, 0);

    assert_eq!(list_state.item_count(), 4);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 3,
            offset_in_item: px(32.0),
        })
    );
}

#[test]
fn removing_content_anchor_row_reanchors_to_replacement_index() {
    let list_state = measured_turn_list(5);
    list_state.scroll_to(ListOffset {
        item_ix: 2,
        offset_in_item: px(72.0),
    });

    list_state.splice(2..3, 0);

    assert_eq!(list_state.item_count(), 4);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 2,
            offset_in_item: px(0.0),
        })
    );
}
