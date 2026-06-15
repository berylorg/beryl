#[allow(dead_code)]
#[path = "../src/shell/transcript_viewport.rs"]
mod transcript_viewport;

#[allow(dead_code)]
#[path = "../src/shell/virtual_list/mod.rs"]
mod virtual_list;

#[allow(dead_code)]
#[path = "../src/shell/transcript_viewport_scroll_coordinator.rs"]
mod transcript_viewport_scroll_coordinator;

use gpui::px;
use transcript_viewport::{
    TranscriptFrameSegment, TranscriptFrameSegmentKey, TranscriptStreamedNavigationFrame,
    TranscriptViewportChunkAnchor, TranscriptViewportFrame, TranscriptViewportLocalOffsetBasis,
    TranscriptViewportMode, TranscriptViewportNavigationDirection, TranscriptViewportPlacement,
    TranscriptViewportState, TranscriptViewportTurnAnchor,
};
use transcript_viewport_scroll_coordinator::{
    TranscriptEventTimeScrollState, TranscriptStreamedNavigationSnapshot,
    TranscriptViewportScrollKindForShell, apply_transcript_viewport_scroll,
    build_rendered_transcript_streamed_navigation_snapshot,
};
use virtual_list::{ListAlignment, ListOffset, ListScrollPosition, ListState};

fn turn(index: usize) -> TranscriptViewportTurnAnchor {
    turn_with_identity(index, format!("row-{index}"))
}

fn turn_with_identity(
    index: usize,
    row_identity: impl Into<String>,
) -> TranscriptViewportTurnAnchor {
    TranscriptViewportTurnAnchor::new(
        index,
        Some(row_identity.into()),
        Some("thread-a".to_string()),
        Some(format!("turn-{index}")),
    )
}

fn chunk(index: usize) -> TranscriptViewportChunkAnchor {
    TranscriptViewportChunkAnchor::new(index, format!("chunk-{index}"))
}

fn streamed_segment(
    turn: TranscriptViewportTurnAnchor,
    chunk: TranscriptViewportChunkAnchor,
    chunk_count: usize,
    height: gpui::Pixels,
) -> TranscriptFrameSegment {
    TranscriptFrameSegment::new(
        TranscriptFrameSegmentKey::streamed_chunk(turn, chunk),
        Some(height),
    )
    .with_streamed_chunk_count(chunk_count)
}

fn ordinary_segment(
    turn: TranscriptViewportTurnAnchor,
    height: gpui::Pixels,
) -> TranscriptFrameSegment {
    TranscriptFrameSegment::new(TranscriptFrameSegmentKey::ordinary_row(turn), Some(height))
}

fn streamed_frame(
    chunk_count: usize,
    rendered_start: usize,
    rendered_end: usize,
    local_offset: gpui::Pixels,
    local_max: gpui::Pixels,
) -> TranscriptStreamedNavigationFrame {
    TranscriptStreamedNavigationFrame::new(
        chunk_count,
        rendered_start..rendered_end,
        (rendered_start < rendered_end).then(|| chunk(rendered_start)),
        (rendered_start < rendered_end).then(|| chunk(rendered_end - 1)),
        (rendered_start > 0).then(|| chunk(rendered_start - 1)),
        (rendered_end < chunk_count).then(|| chunk(rendered_end)),
        local_offset,
        local_max,
    )
}

#[test]
fn render_built_streamed_snapshot_keeps_row_local_range_out_of_rendered_frame() {
    let rendered_frame = TranscriptViewportFrame::new(
        vec![
            ordinary_segment(turn(1), px(90.0)),
            streamed_segment(turn(2), chunk(0), 4, px(80.0)),
            streamed_segment(turn(2), chunk(1), 4, px(120.0)),
        ],
        0..3,
        px(110.0),
        px(180.0),
    );
    let snapshot = build_rendered_transcript_streamed_navigation_snapshot(
        2,
        "row-2",
        4,
        0..2,
        Some(chunk(0)),
        Some(chunk(1)),
        None,
        Some(chunk(2)),
        px(30.0),
        px(400.0),
        rendered_frame,
    );

    assert_eq!(snapshot.frame.local_scroll_offset, px(30.0));
    assert_eq!(snapshot.frame.local_scroll_max, px(400.0));
    assert_eq!(snapshot.rendered_frame.local_scroll_offset(), px(110.0));
    assert_eq!(snapshot.rendered_frame.local_scroll_max(), px(180.0));
}

#[test]
fn event_time_scroll_frame_accumulates_repeated_wheel_before_repaint() {
    let list_state = ListState::new(4, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_ordinary(turn(1), TranscriptViewportPlacement::Top, px(40.0));
    let prepaint_frame = TranscriptViewportFrame::new(
        vec![ordinary_segment(turn(1), px(200.0))],
        0..1,
        px(40.0),
        px(100.0),
    );
    let mut event_time = TranscriptEventTimeScrollState::default();

    let first = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Down,
        px(10.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        Some(prepaint_frame.clone()),
    );
    assert!(first.consumed);
    event_time.apply_navigation_application(TranscriptViewportNavigationDirection::Down, &first);

    let second_frame = event_time.effective_rendered_frame(Some(prepaint_frame));
    let second = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Down,
        px(10.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        second_frame,
    );

    assert!(second.consumed);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 1,
            offset_in_item: px(60.0),
        })
    );
}

#[test]
fn event_time_scroll_frame_accumulates_precise_touchpad_delta_before_repaint() {
    let list_state = ListState::new(4, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_ordinary(turn(1), TranscriptViewportPlacement::Top, px(40.0));
    let prepaint_frame = TranscriptViewportFrame::new(
        vec![ordinary_segment(turn(1), px(200.0))],
        0..1,
        px(40.0),
        px(100.0),
    );
    let mut event_time = TranscriptEventTimeScrollState::default();

    let first = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Down,
        px(2.5),
        TranscriptViewportScrollKindForShell::Touchpad,
        None,
        Some(prepaint_frame.clone()),
    );
    assert!(first.consumed);
    event_time.apply_navigation_application(TranscriptViewportNavigationDirection::Down, &first);

    let second = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Down,
        px(2.5),
        TranscriptViewportScrollKindForShell::Touchpad,
        None,
        event_time.effective_rendered_frame(Some(prepaint_frame)),
    );

    assert!(second.consumed);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 1,
            offset_in_item: px(45.0),
        })
    );
}

#[test]
fn event_time_scroll_frame_rebases_after_crossing_visible_boundary_before_repaint() {
    let list_state = ListState::new(4, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_ordinary(turn(2), TranscriptViewportPlacement::Top, px(0.0));
    let prepaint_frame = TranscriptViewportFrame::new(
        vec![
            ordinary_segment(turn(1), px(120.0)),
            ordinary_segment(turn(2), px(200.0)),
        ],
        1..2,
        px(0.0),
        px(100.0),
    );
    let mut event_time = TranscriptEventTimeScrollState::default();

    let first = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Up,
        px(20.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        Some(prepaint_frame.clone()),
    );
    assert!(first.consumed);
    event_time.apply_navigation_application(TranscriptViewportNavigationDirection::Up, &first);

    let second = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Up,
        px(10.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        event_time.effective_rendered_frame(Some(prepaint_frame)),
    );

    assert!(second.consumed);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 1,
            offset_in_item: px(90.0),
        })
    );
}

#[test]
fn ordinary_boundary_remeasurement_preserves_bottom_relative_crossing_anchor() {
    let list_state = ListState::new(12, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_ordinary(turn(11), TranscriptViewportPlacement::Top, px(0.0));
    let prepaint_frame = TranscriptViewportFrame::new(
        vec![
            ordinary_segment(turn(10), px(120.0)),
            ordinary_segment(turn(11), px(320.0)),
        ],
        1..2,
        px(0.0),
        px(220.0),
    );
    let mut event_time = TranscriptEventTimeScrollState::default();

    let crossed = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Up,
        px(24.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        Some(prepaint_frame.clone()),
    );
    assert!(crossed.consumed);
    event_time.apply_navigation_application(TranscriptViewportNavigationDirection::Up, &crossed);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 10,
            offset_in_item: px(96.0),
        })
    );

    assert!(event_time.clear());
    let fresh_frame_after_measurement = TranscriptViewportFrame::new(
        vec![
            ordinary_segment(turn(10), px(200.0)),
            ordinary_segment(turn(11), px(320.0)),
        ],
        0..1,
        px(96.0),
        px(420.0),
    );

    let after_refresh = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Up,
        px(10.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        event_time.effective_rendered_frame(Some(fresh_frame_after_measurement)),
    );

    assert!(after_refresh.consumed);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 10,
            offset_in_item: px(166.0),
        })
    );
}

#[test]
fn ordinary_boundary_trajectory_from_local_max_preserves_remeasurement_anchor() {
    let list_state = ListState::new(12, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_ordinary(turn(11), TranscriptViewportPlacement::Top, px(220.0));
    let prepaint_frame = TranscriptViewportFrame::new(
        vec![
            ordinary_segment(turn(10), px(120.0)),
            ordinary_segment(turn(11), px(320.0)),
        ],
        1..2,
        px(220.0),
        px(220.0),
    );
    assert_eq!(
        prepaint_frame.local_scroll_offset(),
        prepaint_frame.local_scroll_max()
    );
    let mut event_time = TranscriptEventTimeScrollState::default();

    let same_row = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Up,
        px(220.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        Some(prepaint_frame.clone()),
    );
    assert!(same_row.consumed);
    event_time.apply_navigation_application(TranscriptViewportNavigationDirection::Up, &same_row);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 11,
            offset_in_item: px(0.0),
        })
    );

    let crossed = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Up,
        px(24.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        event_time.effective_rendered_frame(Some(prepaint_frame.clone())),
    );
    assert!(crossed.consumed);
    event_time.apply_navigation_application(TranscriptViewportNavigationDirection::Up, &crossed);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 10,
            offset_in_item: px(96.0),
        })
    );
    match viewport.mode() {
        TranscriptViewportMode::Ordinary(anchor) => {
            assert_eq!(anchor.turn.turn_index, 10);
            assert_eq!(anchor.local_offset, px(96.0));
            assert_eq!(
                anchor.local_offset_basis,
                TranscriptViewportLocalOffsetBasis::Trailing {
                    distance_from_end: px(24.0)
                }
            );
        }
        other => panic!("expected ordinary viewport, got {other:?}"),
    }

    assert!(event_time.clear());
    let fresh_frame_after_measurement = TranscriptViewportFrame::new(
        vec![
            ordinary_segment(turn(10), px(200.0)),
            ordinary_segment(turn(11), px(320.0)),
        ],
        0..1,
        px(96.0),
        px(420.0),
    );

    let after_refresh = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Up,
        px(10.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        event_time.effective_rendered_frame(Some(fresh_frame_after_measurement)),
    );

    assert!(after_refresh.consumed);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 10,
            offset_in_item: px(166.0),
        })
    );
}

#[test]
fn event_time_scroll_frame_preserves_latest_cursor_for_alternating_direction() {
    let list_state = ListState::new(4, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_ordinary(turn(1), TranscriptViewportPlacement::Top, px(40.0));
    let prepaint_frame = TranscriptViewportFrame::new(
        vec![ordinary_segment(turn(1), px(200.0))],
        0..1,
        px(40.0),
        px(100.0),
    );
    let mut event_time = TranscriptEventTimeScrollState::default();

    let down = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Down,
        px(10.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        Some(prepaint_frame.clone()),
    );
    event_time.apply_navigation_application(TranscriptViewportNavigationDirection::Down, &down);

    let up = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Up,
        px(6.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        event_time.effective_rendered_frame(Some(prepaint_frame)),
    );

    assert!(up.consumed);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 1,
            offset_in_item: px(44.0),
        })
    );
}

#[test]
fn event_time_scroll_frame_overrides_streamed_snapshot_rendered_frame() {
    let list_state = ListState::new(4, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_streamed(turn(2), chunk(1), 4, TranscriptViewportPlacement::Top);
    let prepaint_frame = TranscriptViewportFrame::new(
        vec![
            streamed_segment(turn(2), chunk(0), 4, px(80.0)),
            streamed_segment(turn(2), chunk(1), 4, px(120.0)),
        ],
        0..2,
        px(90.0),
        px(140.0),
    );
    let snapshot = TranscriptStreamedNavigationSnapshot {
        row_index: 2,
        row_identity: "row-2".to_string(),
        frame: streamed_frame(4, 0, 2, px(90.0), px(140.0)),
        rendered_frame: prepaint_frame.clone(),
    };
    let mut event_time = TranscriptEventTimeScrollState::default();

    let first = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Down,
        px(20.0),
        TranscriptViewportScrollKindForShell::Touchpad,
        Some(snapshot.clone()),
        None,
    );
    event_time.apply_navigation_application(TranscriptViewportNavigationDirection::Down, &first);

    let effective = event_time
        .effective_streamed_snapshot(Some(snapshot))
        .expect("streamed snapshot should remain available");
    assert_eq!(effective.rendered_frame.local_scroll_offset(), px(110.0));
}

#[test]
fn missing_adjacent_geometry_blocks_same_direction_stale_reuse() {
    let list_state = ListState::new(4, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_ordinary(turn(2), TranscriptViewportPlacement::Top, px(0.0));
    let prepaint_frame = TranscriptViewportFrame::new(
        vec![
            TranscriptFrameSegment::new(TranscriptFrameSegmentKey::ordinary_row(turn(1)), None),
            ordinary_segment(turn(2), px(200.0)),
        ],
        1..2,
        px(0.0),
        px(100.0),
    );
    let mut event_time = TranscriptEventTimeScrollState::default();

    let blocked = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Up,
        px(20.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        Some(prepaint_frame),
    );
    assert!(blocked.consumed);
    assert_eq!(
        blocked.blocked_direction,
        Some(TranscriptViewportNavigationDirection::Up)
    );

    event_time.apply_navigation_application(TranscriptViewportNavigationDirection::Up, &blocked);
    assert!(event_time.is_blocked(TranscriptViewportNavigationDirection::Up));
    assert!(!event_time.is_blocked(TranscriptViewportNavigationDirection::Down));
}

#[test]
fn event_time_scroll_state_clears_on_fresh_prepaint() {
    let application = transcript_viewport_scroll_coordinator::TranscriptViewportNavigationApplication {
        event_time_rendered_frame: Some(TranscriptViewportFrame::new(
            vec![ordinary_segment(turn(1), px(200.0))],
            0..1,
            px(50.0),
            px(100.0),
        )),
        consumed: true,
        ..transcript_viewport_scroll_coordinator::TranscriptViewportNavigationApplication::default()
    };
    let mut event_time = TranscriptEventTimeScrollState::default();
    assert!(
        event_time.apply_navigation_application(
            TranscriptViewportNavigationDirection::Down,
            &application
        )
    );

    assert!(event_time.clear());
    assert_eq!(event_time.effective_rendered_frame(None), None);
}

#[test]
fn continuous_scroll_syncs_list_state_to_crossed_segment_anchor() {
    let list_state = ListState::new(3, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_streamed(turn(1), chunk(1), 2, TranscriptViewportPlacement::Bottom);
    let rendered_frame = TranscriptViewportFrame::new(
        vec![
            streamed_segment(turn(1), chunk(1), 2, px(80.0)),
            streamed_segment(turn(2), chunk(0), 4, px(120.0)),
        ],
        0..1,
        px(80.0),
        px(80.0),
    );
    let snapshot = TranscriptStreamedNavigationSnapshot {
        row_index: 1,
        row_identity: "row-1".to_string(),
        frame: streamed_frame(2, 1, 2, px(80.0), px(80.0)),
        rendered_frame: rendered_frame.clone(),
    };

    let application = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Down,
        px(20.0),
        TranscriptViewportScrollKindForShell::Wheel,
        Some(snapshot),
        None,
    );

    assert!(application.changed);
    assert!(application.consumed);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 2,
            offset_in_item: px(20.0),
        })
    );
}

#[test]
fn continuous_scroll_syncs_list_state_to_segment_local_cursor() {
    let list_state = ListState::new(4, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_streamed(turn(2), chunk(1), 4, TranscriptViewportPlacement::Top);
    let rendered_frame = TranscriptViewportFrame::new(
        vec![
            streamed_segment(turn(2), chunk(0), 4, px(80.0)),
            streamed_segment(turn(2), chunk(1), 4, px(120.0)),
        ],
        0..2,
        px(90.0),
        px(140.0),
    );
    let snapshot = TranscriptStreamedNavigationSnapshot {
        row_index: 2,
        row_identity: "row-2".to_string(),
        frame: streamed_frame(4, 0, 2, px(90.0), px(140.0)),
        rendered_frame,
    };

    let application = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Down,
        px(20.0),
        TranscriptViewportScrollKindForShell::Touchpad,
        Some(snapshot),
        None,
    );

    assert!(application.changed);
    assert!(application.consumed);
    assert_eq!(
        list_state.scroll_position(),
        ListScrollPosition::Content(ListOffset {
            item_ix: 2,
            offset_in_item: px(30.0),
        })
    );
}

#[test]
fn stale_streamed_snapshot_does_not_jump_to_snapshot_target() {
    let list_state = ListState::new(8, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_streamed(turn(4), chunk(2), 6, TranscriptViewportPlacement::Top);
    let stale_rendered_frame = TranscriptViewportFrame::new(
        vec![streamed_segment(turn(1), chunk(0), 3, px(120.0))],
        0..1,
        px(0.0),
        px(80.0),
    );
    let stale_snapshot = TranscriptStreamedNavigationSnapshot {
        row_index: 1,
        row_identity: "row-1".to_string(),
        frame: streamed_frame(3, 0, 1, px(0.0), px(80.0)),
        rendered_frame: stale_rendered_frame,
    };

    let application = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Up,
        px(12.0),
        TranscriptViewportScrollKindForShell::Touchpad,
        Some(stale_snapshot),
        None,
    );

    assert!(!application.consumed);
    assert_eq!(list_state.scroll_position(), ListScrollPosition::Bottom);
    match viewport.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.turn, turn(4));
            assert_eq!(anchor.anchor_chunk, chunk(2));
        }
        other => panic!("expected original streamed viewport, got {other:?}"),
    }
}

#[test]
fn stale_streamed_snapshot_with_reused_row_index_does_not_match_new_identity() {
    let list_state = ListState::new(8, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_streamed(
        turn_with_identity(4, "row-current"),
        chunk(2),
        6,
        TranscriptViewportPlacement::Top,
    );
    let stale_turn = turn_with_identity(4, "row-stale");
    let stale_rendered_frame = TranscriptViewportFrame::new(
        vec![streamed_segment(stale_turn, chunk(0), 3, px(120.0))],
        0..1,
        px(0.0),
        px(80.0),
    );
    let stale_snapshot = TranscriptStreamedNavigationSnapshot {
        row_index: 4,
        row_identity: "row-stale".to_string(),
        frame: streamed_frame(3, 0, 1, px(0.0), px(80.0)),
        rendered_frame: stale_rendered_frame,
    };

    let application = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Up,
        px(12.0),
        TranscriptViewportScrollKindForShell::Touchpad,
        Some(stale_snapshot),
        None,
    );

    assert!(!application.consumed);
    assert_eq!(list_state.scroll_position(), ListScrollPosition::Bottom);
    match viewport.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.turn.row_identity.as_deref(), Some("row-current"));
            assert_eq!(anchor.turn.turn_index, 4);
            assert_eq!(anchor.anchor_chunk, chunk(2));
        }
        other => panic!("expected original streamed viewport, got {other:?}"),
    }
}

#[test]
fn missing_rendered_frame_does_not_move_list_state() {
    let list_state = ListState::new(4, ListAlignment::Bottom, px(320.0));
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_ordinary(turn(2), TranscriptViewportPlacement::Top, px(40.0));

    let application = apply_transcript_viewport_scroll(
        &mut viewport,
        &list_state,
        TranscriptViewportNavigationDirection::Down,
        px(10.0),
        TranscriptViewportScrollKindForShell::Wheel,
        None,
        None,
    );

    assert!(!application.changed);
    assert!(!application.consumed);
    assert_eq!(list_state.scroll_position(), ListScrollPosition::Bottom);
}
