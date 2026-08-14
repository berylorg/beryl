use gpui::px;

use super::virtual_list::{ListOffset, ListScrollPosition, ListState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptTurnJumpDirection {
    Up,
    Down,
}

pub(crate) fn transcript_turn_jump_target(
    list_state: &ListState,
    turn_count: usize,
    direction: TranscriptTurnJumpDirection,
) -> Option<ListScrollPosition> {
    let turn_count = turn_count.min(list_state.item_count());
    if turn_count == 0 {
        return None;
    }

    let current_position = list_state.scroll_position();
    let current = list_state.logical_scroll_top();
    match direction {
        TranscriptTurnJumpDirection::Up => turn_jump_up_target(current, turn_count),
        TranscriptTurnJumpDirection::Down => {
            turn_jump_down_target(current, current_position, turn_count)
        }
    }
}

fn turn_jump_up_target(current: ListOffset, turn_count: usize) -> Option<ListScrollPosition> {
    if current.item_ix >= turn_count {
        return Some(turn_top(turn_count - 1));
    }
    if current.offset_in_item > px(0.0) {
        return Some(turn_top(current.item_ix));
    }
    current.item_ix.checked_sub(1).map(turn_top)
}

fn turn_jump_down_target(
    current: ListOffset,
    current_position: ListScrollPosition,
    turn_count: usize,
) -> Option<ListScrollPosition> {
    if matches!(current_position, ListScrollPosition::Bottom) {
        return None;
    }
    if current.item_ix >= turn_count {
        return Some(ListScrollPosition::Bottom);
    }
    let next_turn = current.item_ix + 1;
    if next_turn < turn_count {
        Some(turn_top(next_turn))
    } else {
        Some(ListScrollPosition::Bottom)
    }
}

fn turn_top(item_ix: usize) -> ListScrollPosition {
    ListScrollPosition::Content(ListOffset {
        item_ix,
        offset_in_item: px(0.0),
    })
}

pub(crate) struct LiveTranscriptRows {
    pub(crate) previous_turn_count: usize,
    pub(crate) current_turn_count: usize,
    pub(crate) preserve_user_scroll: bool,
}

pub(crate) fn sync_live_transcript_rows(list_state: &ListState, rows: LiveTranscriptRows) {
    let preserved_scroll: Option<ListScrollPosition> = (rows.preserve_user_scroll
        && rows.previous_turn_count == rows.current_turn_count
        && rows.current_turn_count > 0)
        .then(|| list_state.scroll_position());

    splice_live_transcript_rows(list_state, rows);

    if let Some(scroll_top) = preserved_scroll {
        list_state.scroll_to_position(scroll_top);
    }
}

fn splice_live_transcript_rows(list_state: &ListState, rows: LiveTranscriptRows) {
    match rows.current_turn_count.cmp(&rows.previous_turn_count) {
        std::cmp::Ordering::Greater => list_state.splice(
            rows.previous_turn_count..rows.previous_turn_count,
            rows.current_turn_count - rows.previous_turn_count,
        ),
        std::cmp::Ordering::Less => {
            list_state.splice(rows.current_turn_count..rows.previous_turn_count, 0)
        }
        std::cmp::Ordering::Equal => {}
    }
}
