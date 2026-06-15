#[allow(dead_code)]
#[path = "../src/shell/transcript_viewport.rs"]
mod transcript_viewport;

use gpui::px;
use transcript_viewport::{
    TranscriptFrameSegment, TranscriptFrameSegmentKey, TranscriptSegmentMeasurementCache,
    TranscriptSegmentMeasurementKey, TranscriptSegmentMeasurementQueue,
    TranscriptSegmentMeasurementRevision, TranscriptStreamedNavigationFrame,
    TranscriptViewportBoundary, TranscriptViewportChunkAnchor, TranscriptViewportFrame,
    TranscriptViewportInvalidation, TranscriptViewportLiveAutoscroll, TranscriptViewportMode,
    TranscriptViewportNavigationDirection, TranscriptViewportPlacement,
    TranscriptViewportReduceOutcome, TranscriptViewportRowMutation, TranscriptViewportState,
    TranscriptViewportTurnAnchor, TranscriptViewportTurnTarget,
};

fn turn(index: usize) -> TranscriptViewportTurnAnchor {
    TranscriptViewportTurnAnchor::new(
        index,
        Some(format!("row-{index}")),
        Some("thread-a".to_string()),
        Some(format!("turn-{index}")),
    )
}

fn chunk(index: usize) -> TranscriptViewportChunkAnchor {
    TranscriptViewportChunkAnchor::new(index, format!("chunk-{index}"))
}

fn segment(key: TranscriptFrameSegmentKey) -> TranscriptFrameSegment {
    TranscriptFrameSegment::new(key, None)
}

fn measured_segment(
    key: TranscriptFrameSegmentKey,
    height: gpui::Pixels,
) -> TranscriptFrameSegment {
    TranscriptFrameSegment::new(key, Some(height))
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

fn current_streamed_rendered_frame(
    turn_anchor: TranscriptViewportTurnAnchor,
    chunk_anchor: TranscriptViewportChunkAnchor,
    chunk_count: usize,
    local_offset: gpui::Pixels,
    local_max: gpui::Pixels,
) -> TranscriptViewportFrame {
    TranscriptViewportFrame::new(
        vec![streamed_segment(
            turn_anchor,
            chunk_anchor,
            chunk_count,
            local_max + px(120.0),
        )],
        0..1,
        local_offset,
        local_max,
    )
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

fn rendered_segment_top(
    frame: &TranscriptViewportFrame,
    key: &TranscriptFrameSegmentKey,
) -> gpui::Pixels {
    let index = frame
        .segments()
        .iter()
        .position(|segment| &segment.key == key)
        .expect("segment should exist in rendered frame");
    frame
        .segments()
        .iter()
        .take(index)
        .map(|segment| segment.measured_height.expect("segment should be measured"))
        .fold(px(0.0), |sum, height| sum + height)
}

fn rendered_top_for_anchor(
    frame: &TranscriptViewportFrame,
    key: &TranscriptFrameSegmentKey,
    local_offset: gpui::Pixels,
) -> gpui::Pixels {
    rendered_segment_top(frame, key) + local_offset
}

fn outcome_cursor_local(outcome: &TranscriptViewportReduceOutcome) -> Option<gpui::Pixels> {
    outcome
        .scroll_cursor
        .as_ref()
        .map(|cursor| cursor.local_offset)
}

fn streamed_state() -> TranscriptViewportState {
    let mut state = TranscriptViewportState::default();
    state.anchor_streamed(turn(2), chunk(5), 12, TranscriptViewportPlacement::Top);
    state
}

fn measurement_key(
    segment: TranscriptFrameSegmentKey,
    revision: u64,
) -> TranscriptSegmentMeasurementKey {
    TranscriptSegmentMeasurementKey::new(
        segment,
        TranscriptSegmentMeasurementRevision::new(revision),
    )
}

#[test]
fn frame_segments_find_adjacent_content_across_ordinary_and_streamed_boundaries() {
    let ordinary_before = TranscriptFrameSegmentKey::ordinary_row(turn(0));
    let first_chunk = TranscriptFrameSegmentKey::streamed_chunk(turn(1), chunk(0));
    let second_chunk = TranscriptFrameSegmentKey::streamed_chunk(turn(1), chunk(1));
    let ordinary_after = TranscriptFrameSegmentKey::ordinary_row(turn(2));
    let frame = TranscriptViewportFrame::new(
        vec![
            segment(ordinary_before.clone()),
            segment(first_chunk.clone()),
            segment(second_chunk.clone()),
            segment(ordinary_after.clone()),
        ],
        1..3,
        px(0.0),
        px(160.0),
    );

    assert_eq!(frame.visible_segment_range(), 1..3);
    assert_eq!(
        frame.first_visible_segment().map(|segment| &segment.key),
        Some(&first_chunk)
    );
    assert_eq!(
        frame.last_visible_segment().map(|segment| &segment.key),
        Some(&second_chunk)
    );
    assert_eq!(
        frame.segment_before_visible().map(|segment| &segment.key),
        Some(&ordinary_before)
    );
    assert_eq!(
        frame.segment_after_visible().map(|segment| &segment.key),
        Some(&ordinary_after)
    );
    assert_eq!(
        frame
            .adjacent_segment(&first_chunk, TranscriptViewportNavigationDirection::Up)
            .map(|segment| &segment.key),
        Some(&ordinary_before)
    );
    assert_eq!(
        frame
            .adjacent_segment(&second_chunk, TranscriptViewportNavigationDirection::Down)
            .map(|segment| &segment.key),
        Some(&ordinary_after)
    );
}

#[test]
fn frame_segment_keys_distinguish_render_and_resident_budget_fallbacks() {
    let turn_anchor = turn(3);
    let chunk_anchor = chunk(4);
    let streamed =
        TranscriptFrameSegmentKey::streamed_chunk(turn_anchor.clone(), chunk_anchor.clone());
    let render_budget_fallback = TranscriptFrameSegmentKey::render_budget_fallback_chunk(
        turn_anchor.clone(),
        chunk_anchor.clone(),
        "frame_cost_exceeds_limit",
    );
    let resident_budget_fallback = TranscriptFrameSegmentKey::resident_budget_fallback_row(
        turn_anchor.clone(),
        "resident_budget_oversized_turn",
    );

    assert_ne!(streamed, render_budget_fallback);
    assert_ne!(
        TranscriptFrameSegmentKey::ordinary_row(turn_anchor),
        resident_budget_fallback
    );
    assert_eq!(
        render_budget_fallback.streamed_chunk_anchor(),
        Some(&chunk_anchor)
    );
}

#[test]
fn segment_measurement_commit_coalesces_same_key_before_mutating_cache() {
    let segment = TranscriptFrameSegmentKey::ordinary_row(turn(1));
    let key = measurement_key(segment, 10);
    let mut queue = TranscriptSegmentMeasurementQueue::default();
    let mut cache = TranscriptSegmentMeasurementCache::default();

    queue.stage(key.clone(), px(80.0));
    queue.stage(key.clone(), px(96.0));
    let commit = queue.commit_into(&mut cache, None);

    assert_eq!(commit.changed.len(), 1);
    assert_eq!(commit.changed[0].measured_height, px(96.0));
    assert_eq!(commit.unchanged, 0);
    assert_eq!(cache.height(&key), Some(px(96.0)));
}

#[test]
fn segment_measurement_commit_treats_same_height_as_noop() {
    let segment = TranscriptFrameSegmentKey::ordinary_row(turn(1));
    let key = measurement_key(segment, 10);
    let mut queue = TranscriptSegmentMeasurementQueue::default();
    let mut cache = TranscriptSegmentMeasurementCache::default();

    queue.stage(key.clone(), px(80.0));
    let first = queue.commit_into(&mut cache, None);
    assert_eq!(first.changed.len(), 1);

    queue.stage(key.clone(), px(80.0));
    let second = queue.commit_into(&mut cache, None);

    assert!(second.changed.is_empty());
    assert_eq!(second.unchanged, 1);
    assert_eq!(second.anchor_offset_correction, px(0.0));
    assert_eq!(cache.height(&key), Some(px(80.0)));
}

#[test]
fn segment_measurement_commit_preserves_anchor_offset_for_changed_anchor_segment() {
    let segment = TranscriptFrameSegmentKey::ordinary_row(turn(1));
    let key = measurement_key(segment.clone(), 10);
    let mut queue = TranscriptSegmentMeasurementQueue::default();
    let mut cache = TranscriptSegmentMeasurementCache::default();
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_ordinary(turn(1), TranscriptViewportPlacement::Top, px(24.0));

    queue.stage(key.clone(), px(80.0));
    queue.commit_into(&mut cache, None);
    queue.stage(key, px(100.0));
    let anchor = viewport.segment_measurement_anchor();
    let commit = queue.commit_into(&mut cache, anchor.as_ref());

    assert_eq!(commit.changed.len(), 1);
    assert_eq!(commit.anchor_offset_correction, px(20.0));
    assert!(viewport.apply_segment_measurement_anchor_correction(commit.anchor_offset_correction));
    match viewport.mode() {
        TranscriptViewportMode::Ordinary(anchor) => {
            assert_eq!(anchor.local_offset, px(44.0));
        }
        other => panic!("expected ordinary viewport, got {other:?}"),
    }
}

#[test]
fn segment_measurement_commit_does_not_repeat_anchor_correction_for_same_height() {
    let segment = TranscriptFrameSegmentKey::ordinary_row(turn(1));
    let key = measurement_key(segment, 10);
    let mut queue = TranscriptSegmentMeasurementQueue::default();
    let mut cache = TranscriptSegmentMeasurementCache::default();
    let mut viewport = TranscriptViewportState::default();
    viewport.anchor_ordinary(turn(1), TranscriptViewportPlacement::Top, px(24.0));

    queue.stage(key.clone(), px(80.0));
    queue.commit_into(&mut cache, None);
    queue.stage(key.clone(), px(100.0));
    let anchor = viewport.segment_measurement_anchor();
    let changed = queue.commit_into(&mut cache, anchor.as_ref());
    viewport.apply_segment_measurement_anchor_correction(changed.anchor_offset_correction);

    queue.stage(key, px(100.0));
    let anchor = viewport.segment_measurement_anchor();
    let repeated = queue.commit_into(&mut cache, anchor.as_ref());

    assert!(repeated.changed.is_empty());
    assert_eq!(repeated.unchanged, 1);
    assert_eq!(repeated.anchor_offset_correction, px(0.0));
    match viewport.mode() {
        TranscriptViewportMode::Ordinary(anchor) => {
            assert_eq!(anchor.local_offset, px(44.0));
        }
        other => panic!("expected ordinary viewport, got {other:?}"),
    }
}

#[test]
fn segment_measurement_commit_treats_layout_revision_change_as_new_measurement() {
    let segment = TranscriptFrameSegmentKey::ordinary_row(turn(1));
    let old_key = measurement_key(segment.clone(), 10);
    let new_key = measurement_key(segment, 11);
    let mut queue = TranscriptSegmentMeasurementQueue::default();
    let mut cache = TranscriptSegmentMeasurementCache::default();

    queue.stage(old_key.clone(), px(80.0));
    queue.commit_into(&mut cache, None);
    queue.stage(new_key.clone(), px(80.0));
    let commit = queue.commit_into(&mut cache, None);

    assert_eq!(commit.changed.len(), 1);
    assert_eq!(commit.changed[0].previous_height, None);
    assert_eq!(cache.height(&old_key), None);
    assert_eq!(cache.height(&new_key), Some(px(80.0)));
}

#[test]
fn segment_measurement_commit_applies_multiple_segments_in_one_batch() {
    let first = measurement_key(TranscriptFrameSegmentKey::ordinary_row(turn(1)), 10);
    let second = measurement_key(
        TranscriptFrameSegmentKey::streamed_chunk(turn(2), chunk(0)),
        20,
    );
    let mut queue = TranscriptSegmentMeasurementQueue::default();
    let mut cache = TranscriptSegmentMeasurementCache::default();

    queue.stage(first.clone(), px(80.0));
    queue.stage(second.clone(), px(42.0));
    let commit = queue.commit_into(&mut cache, None);

    assert_eq!(commit.changed.len(), 2);
    assert_eq!(commit.unchanged, 0);
    assert_eq!(cache.height(&first), Some(px(80.0)));
    assert_eq!(cache.height(&second), Some(px(42.0)));
}

#[test]
fn frame_reducer_preserves_small_boundary_delta_without_jump() {
    let previous = TranscriptFrameSegmentKey::ordinary_row(turn(1));
    let current = TranscriptFrameSegmentKey::streamed_chunk(turn(2), chunk(0));
    let frame = TranscriptViewportFrame::new(
        vec![
            measured_segment(previous.clone(), px(120.0)),
            measured_segment(current, px(180.0)),
        ],
        1..2,
        px(0.0),
        px(90.0),
    );

    let reduction = frame.reduce_scroll_delta(TranscriptViewportNavigationDirection::Up, px(6.0));

    assert_eq!(
        reduction.cursor.as_ref().map(|cursor| &cursor.segment.key),
        Some(&previous)
    );
    assert_eq!(
        reduction.cursor.as_ref().map(|cursor| cursor.local_offset),
        Some(px(114.0))
    );
    assert_eq!(
        reduction.cursor.as_ref().map(|cursor| cursor.placement),
        Some(TranscriptViewportPlacement::Top)
    );
    assert_eq!(reduction.residual_delta, px(0.0));
    assert_eq!(reduction.boundary, None);
}

#[test]
fn frame_reducer_consumes_large_delta_across_multiple_measured_segments() {
    let farther = TranscriptFrameSegmentKey::ordinary_row(turn(0));
    let nearer = TranscriptFrameSegmentKey::streamed_chunk(turn(1), chunk(0));
    let current = TranscriptFrameSegmentKey::streamed_chunk(turn(2), chunk(0));
    let frame = TranscriptViewportFrame::new(
        vec![
            measured_segment(farther.clone(), px(30.0)),
            measured_segment(nearer, px(40.0)),
            measured_segment(current, px(200.0)),
        ],
        2..3,
        px(0.0),
        px(120.0),
    );

    let reduction = frame.reduce_scroll_delta(TranscriptViewportNavigationDirection::Up, px(55.0));

    assert_eq!(
        reduction.cursor.as_ref().map(|cursor| &cursor.segment.key),
        Some(&farther)
    );
    assert_eq!(
        reduction.cursor.as_ref().map(|cursor| cursor.local_offset),
        Some(px(15.0))
    );
    assert_eq!(
        reduction.cursor.as_ref().map(|cursor| cursor.placement),
        Some(TranscriptViewportPlacement::Top)
    );
    assert_eq!(reduction.residual_delta, px(0.0));
    assert_eq!(reduction.boundary, None);
}

#[test]
fn frame_reducer_reports_residual_delta_at_unknown_adjacent_geometry() {
    let unknown = TranscriptFrameSegmentKey::ordinary_row(turn(1));
    let current = TranscriptFrameSegmentKey::streamed_chunk(turn(2), chunk(0));
    let frame = TranscriptViewportFrame::new(
        vec![
            segment(unknown.clone()),
            measured_segment(current, px(200.0)),
        ],
        1..2,
        px(0.0),
        px(90.0),
    );

    let reduction = frame.reduce_scroll_delta(TranscriptViewportNavigationDirection::Up, px(25.0));

    assert_eq!(reduction.cursor, None);
    assert_eq!(reduction.residual_delta, px(25.0));
    assert_eq!(reduction.boundary, None);
}

#[test]
fn frame_reducer_clamps_at_resident_boundary_without_fake_geometry() {
    let current = TranscriptFrameSegmentKey::streamed_chunk(turn(0), chunk(0));
    let frame = TranscriptViewportFrame::new(
        vec![measured_segment(current, px(200.0))],
        0..1,
        px(0.0),
        px(90.0),
    );

    let reduction = frame.reduce_scroll_delta(TranscriptViewportNavigationDirection::Up, px(25.0));

    assert_eq!(reduction.cursor, None);
    assert_eq!(reduction.residual_delta, px(25.0));
    assert_eq!(reduction.boundary, Some(TranscriptViewportBoundary::Start));
}

#[test]
fn continuous_scroll_enters_adjacent_turn_streamed_chunk_with_frame_delta() {
    let mut state = TranscriptViewportState::default();
    state.anchor_streamed(turn(1), chunk(1), 2, TranscriptViewportPlacement::Bottom);
    let rendered_frame = TranscriptViewportFrame::new(
        vec![
            streamed_segment(turn(1), chunk(1), 2, px(80.0)),
            streamed_segment(turn(2), chunk(0), 4, px(120.0)),
        ],
        0..1,
        px(80.0),
        px(80.0),
    );

    let outcome = state.apply_scroll(
        transcript_viewport::TranscriptViewportScrollInput::wheel(
            TranscriptViewportNavigationDirection::Down,
            px(20.0),
            Some(TranscriptStreamedNavigationFrame::new(
                2,
                1..2,
                Some(chunk(1)),
                Some(chunk(1)),
                Some(chunk(0)),
                None,
                px(80.0),
                px(80.0),
            )),
        )
        .with_rendered_frame(rendered_frame),
    );

    assert!(outcome.semantic_refill);
    assert_eq!(outcome_cursor_local(&outcome), Some(px(20.0)));
    assert_eq!(outcome.boundary, None);
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.turn, turn(2));
            assert_eq!(anchor.anchor_chunk, chunk(0));
            assert_eq!(anchor.chunk_count, 4);
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Top);
            assert_eq!(anchor.local_anchor_offset, Some(px(20.0)));
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn wheel_up_from_visible_ordinary_tail_enters_previous_measured_row() {
    let mut state = TranscriptViewportState::default();
    state.anchor_ordinary(turn(2), TranscriptViewportPlacement::Bottom, px(0.0));
    let previous_key = TranscriptFrameSegmentKey::ordinary_row(turn(1));
    let current_key = TranscriptFrameSegmentKey::ordinary_row(turn(2));
    let rendered_frame = TranscriptViewportFrame::new(
        vec![
            measured_segment(previous_key.clone(), px(120.0)),
            measured_segment(current_key, px(320.0)),
        ],
        1..2,
        px(0.0),
        px(220.0),
    );

    let outcome = state.apply_scroll(
        transcript_viewport::TranscriptViewportScrollInput::wheel(
            TranscriptViewportNavigationDirection::Up,
            px(24.0),
            None,
        )
        .with_rendered_frame(rendered_frame),
    );

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    assert_eq!(outcome.boundary, None);
    assert_eq!(outcome_cursor_local(&outcome), Some(px(96.0)));
    match state.mode() {
        TranscriptViewportMode::Ordinary(anchor) => {
            assert_eq!(anchor.turn, turn(1));
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Top);
            assert_eq!(anchor.local_offset, px(96.0));
        }
        other => panic!("expected ordinary viewport, got {other:?}"),
    }
}

#[test]
fn wheel_up_within_bottom_anchored_ordinary_row_normalizes_to_top_offset() {
    let mut state = TranscriptViewportState::default();
    state.anchor_ordinary(turn(2), TranscriptViewportPlacement::Bottom, px(0.0));
    let rendered_frame = TranscriptViewportFrame::new(
        vec![measured_segment(
            TranscriptFrameSegmentKey::ordinary_row(turn(2)),
            px(320.0),
        )],
        0..1,
        px(100.0),
        px(100.0),
    );

    let outcome = state.apply_scroll(
        transcript_viewport::TranscriptViewportScrollInput::wheel(
            TranscriptViewportNavigationDirection::Up,
            px(24.0),
            None,
        )
        .with_rendered_frame(rendered_frame),
    );

    assert!(outcome.changed);
    assert!(!outcome.semantic_refill);
    assert_eq!(outcome.boundary, None);
    assert_eq!(outcome_cursor_local(&outcome), Some(px(76.0)));
    match state.mode() {
        TranscriptViewportMode::Ordinary(anchor) => {
            assert_eq!(anchor.turn, turn(2));
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Top);
            assert_eq!(anchor.local_offset, px(76.0));
        }
        other => panic!("expected ordinary viewport, got {other:?}"),
    }
}

#[test]
fn wheel_scroll_consumes_rendered_chunk_window_before_semantic_anchor_moves() {
    let mut state = streamed_state();
    let outcome = state.apply_scroll(
        transcript_viewport::TranscriptViewportScrollInput::wheel(
            TranscriptViewportNavigationDirection::Up,
            px(16.0),
            Some(streamed_frame(12, 4, 7, px(40.0), px(120.0))),
        )
        .with_rendered_frame(current_streamed_rendered_frame(
            turn(2),
            chunk(5),
            12,
            px(40.0),
            px(120.0),
        )),
    );

    assert!(outcome.changed);
    assert_eq!(outcome_cursor_local(&outcome), Some(px(24.0)));
    assert!(!outcome.semantic_refill);
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(5));
            assert_eq!(anchor.local_anchor_offset, Some(px(24.0)));
            assert_eq!(anchor.last_navigation_direction, None);
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn touchpad_scroll_consumes_rendered_chunk_window_before_semantic_anchor_moves() {
    let mut state = streamed_state();
    let outcome = state.apply_scroll(
        transcript_viewport::TranscriptViewportScrollInput::touchpad(
            TranscriptViewportNavigationDirection::Down,
            px(30.0),
            Some(streamed_frame(12, 4, 7, px(20.0), px(80.0))),
        )
        .with_rendered_frame(current_streamed_rendered_frame(
            turn(2),
            chunk(5),
            12,
            px(20.0),
            px(80.0),
        )),
    );

    assert!(outcome.changed);
    assert_eq!(outcome_cursor_local(&outcome), Some(px(50.0)));
    assert!(!outcome.semantic_refill);
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(5));
            assert_eq!(anchor.local_anchor_offset, Some(px(50.0)));
            assert_eq!(anchor.last_navigation_direction, None);
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn same_frame_scroll_cursor_is_segment_local_when_visible_start_precedes_anchor() {
    let mut state = TranscriptViewportState::default();
    state.anchor_streamed(turn(2), chunk(1), 4, TranscriptViewportPlacement::Top);
    let anchor_key = TranscriptFrameSegmentKey::streamed_chunk(turn(2), chunk(1));
    let rendered_frame = TranscriptViewportFrame::new(
        vec![
            streamed_segment(turn(2), chunk(0), 4, px(80.0)),
            streamed_segment(turn(2), chunk(1), 4, px(120.0)),
        ],
        0..2,
        px(90.0),
        px(140.0),
    );

    let outcome = state.apply_scroll(
        transcript_viewport::TranscriptViewportScrollInput::touchpad(
            TranscriptViewportNavigationDirection::Down,
            px(20.0),
            Some(streamed_frame(4, 0, 2, px(90.0), px(140.0))),
        )
        .with_rendered_frame(rendered_frame),
    );

    assert!(outcome.changed);
    assert!(!outcome.semantic_refill);
    assert_eq!(outcome_cursor_local(&outcome), Some(px(30.0)));
    assert_eq!(
        outcome
            .scroll_cursor
            .as_ref()
            .map(|cursor| &cursor.segment.key),
        Some(&anchor_key)
    );
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(1));
            assert_eq!(anchor.local_anchor_offset, Some(px(30.0)));
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn wheel_scroll_at_rendered_top_advances_to_previous_semantic_chunk_anchor() {
    let mut state = streamed_state();
    let previous_key = TranscriptFrameSegmentKey::streamed_chunk(turn(2), chunk(3));
    let current_key = TranscriptFrameSegmentKey::streamed_chunk(turn(2), chunk(4));
    let rendered_frame = TranscriptViewportFrame::new(
        vec![
            streamed_segment(turn(2), chunk(3), 12, px(100.0)),
            streamed_segment(turn(2), chunk(4), 12, px(100.0)),
        ],
        1..2,
        px(0.0),
        px(80.0),
    );
    let expected_render_offset =
        rendered_top_for_anchor(&rendered_frame, &current_key, px(0.0)) - px(24.0);
    let outcome = state.apply_scroll(
        transcript_viewport::TranscriptViewportScrollInput::wheel(
            TranscriptViewportNavigationDirection::Up,
            px(24.0),
            Some(streamed_frame(12, 4, 7, px(0.0), px(80.0))),
        )
        .with_rendered_frame(rendered_frame.clone()),
    );

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    assert_eq!(outcome_cursor_local(&outcome), Some(px(76.0)));
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(3));
            assert_eq!(anchor.rendered_chunk_range, 3..4);
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Top);
            assert_eq!(anchor.local_anchor_offset, Some(px(76.0)));
            assert_eq!(
                rendered_top_for_anchor(
                    &rendered_frame,
                    &previous_key,
                    anchor.local_anchor_offset.unwrap()
                ),
                expected_render_offset
            );
            assert_eq!(
                anchor.last_navigation_direction,
                Some(TranscriptViewportNavigationDirection::Up)
            );
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn wheel_scroll_at_rendered_bottom_advances_to_next_semantic_chunk_anchor() {
    let mut state = streamed_state();
    let current_key = TranscriptFrameSegmentKey::streamed_chunk(turn(2), chunk(6));
    let next_key = TranscriptFrameSegmentKey::streamed_chunk(turn(2), chunk(7));
    let rendered_frame = TranscriptViewportFrame::new(
        vec![
            streamed_segment(turn(2), chunk(6), 12, px(100.0)),
            streamed_segment(turn(2), chunk(7), 12, px(100.0)),
        ],
        0..1,
        px(80.0),
        px(80.0),
    );
    let expected_render_offset =
        rendered_top_for_anchor(&rendered_frame, &current_key, px(80.0)) + px(24.0);
    let outcome = state.apply_scroll(
        transcript_viewport::TranscriptViewportScrollInput::wheel(
            TranscriptViewportNavigationDirection::Down,
            px(24.0),
            Some(streamed_frame(12, 4, 7, px(80.0), px(80.0))),
        )
        .with_rendered_frame(rendered_frame.clone()),
    );

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    assert_eq!(outcome_cursor_local(&outcome), Some(px(4.0)));
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(7));
            assert_eq!(anchor.rendered_chunk_range, 7..8);
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Top);
            assert_eq!(anchor.local_anchor_offset, Some(px(4.0)));
            assert_eq!(
                rendered_top_for_anchor(
                    &rendered_frame,
                    &next_key,
                    anchor.local_anchor_offset.unwrap()
                ),
                expected_render_offset
            );
            assert_eq!(
                anchor.last_navigation_direction,
                Some(TranscriptViewportNavigationDirection::Down)
            );
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn page_up_anchors_first_rendered_chunk_at_viewport_bottom() {
    let mut state = streamed_state();
    let frame = TranscriptViewportFrame::new(
        vec![
            streamed_segment(turn(2), chunk(4), 12, px(100.0)),
            streamed_segment(turn(2), chunk(5), 12, px(100.0)),
            streamed_segment(turn(2), chunk(6), 12, px(100.0)),
        ],
        0..3,
        px(0.0),
        px(80.0),
    );
    let outcome = state.apply_page_to_frame(TranscriptViewportNavigationDirection::Up, &frame);

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(4));
            assert_eq!(anchor.rendered_chunk_range, 4..5);
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Bottom);
            assert_eq!(
                anchor.last_navigation_direction,
                Some(TranscriptViewportNavigationDirection::Up)
            );
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn page_down_anchors_last_rendered_chunk_at_viewport_top() {
    let mut state = streamed_state();
    let frame = TranscriptViewportFrame::new(
        vec![
            streamed_segment(turn(2), chunk(4), 12, px(100.0)),
            streamed_segment(turn(2), chunk(5), 12, px(100.0)),
            streamed_segment(turn(2), chunk(6), 12, px(100.0)),
        ],
        0..3,
        px(0.0),
        px(80.0),
    );
    let outcome = state.apply_page_to_frame(TranscriptViewportNavigationDirection::Down, &frame);

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(6));
            assert_eq!(anchor.rendered_chunk_range, 6..7);
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Top);
            assert_eq!(
                anchor.last_navigation_direction,
                Some(TranscriptViewportNavigationDirection::Down)
            );
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn page_up_anchors_first_visible_ordinary_row_at_viewport_bottom() {
    let mut state = TranscriptViewportState::default();
    state.anchor_ordinary(turn(4), TranscriptViewportPlacement::Top, px(0.0));
    let frame = TranscriptViewportFrame::new(
        vec![
            measured_segment(TranscriptFrameSegmentKey::ordinary_row(turn(3)), px(120.0)),
            measured_segment(TranscriptFrameSegmentKey::ordinary_row(turn(4)), px(120.0)),
            measured_segment(TranscriptFrameSegmentKey::ordinary_row(turn(5)), px(120.0)),
        ],
        0..3,
        px(0.0),
        px(240.0),
    );

    let outcome = state.apply_page_to_frame(TranscriptViewportNavigationDirection::Up, &frame);

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    match state.mode() {
        TranscriptViewportMode::Ordinary(anchor) => {
            assert_eq!(anchor.turn, turn(3));
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Bottom);
            assert_eq!(anchor.local_offset, px(0.0));
        }
        other => panic!("expected ordinary viewport, got {other:?}"),
    }
}

#[test]
fn page_down_anchors_last_visible_ordinary_row_at_viewport_top() {
    let mut state = TranscriptViewportState::default();
    state.anchor_ordinary(turn(4), TranscriptViewportPlacement::Top, px(0.0));
    let frame = TranscriptViewportFrame::new(
        vec![
            measured_segment(TranscriptFrameSegmentKey::ordinary_row(turn(3)), px(120.0)),
            measured_segment(TranscriptFrameSegmentKey::ordinary_row(turn(4)), px(120.0)),
            measured_segment(TranscriptFrameSegmentKey::ordinary_row(turn(5)), px(120.0)),
        ],
        0..3,
        px(0.0),
        px(240.0),
    );

    let outcome = state.apply_page_to_frame(TranscriptViewportNavigationDirection::Down, &frame);

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    match state.mode() {
        TranscriptViewportMode::Ordinary(anchor) => {
            assert_eq!(anchor.turn, turn(5));
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Top);
            assert_eq!(anchor.local_offset, px(0.0));
        }
        other => panic!("expected ordinary viewport, got {other:?}"),
    }
}

#[test]
fn ctrl_turn_jump_targets_turn_boundaries_not_chunks() {
    let mut state = streamed_state();
    let outcome = state.apply_turn_jump(Some(TranscriptViewportTurnTarget::ordinary(turn(3))));

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    match state.mode() {
        TranscriptViewportMode::Ordinary(anchor) => {
            assert_eq!(anchor.turn, turn(3));
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Top);
            assert_eq!(anchor.local_offset, px(0.0));
        }
        other => panic!("expected ordinary viewport after turn jump, got {other:?}"),
    }
}

#[test]
fn streamed_turn_jump_uses_target_turn_default_chunk_without_current_chunk_navigation() {
    let mut state = streamed_state();
    let outcome = state.apply_turn_jump(Some(TranscriptViewportTurnTarget::streamed(
        turn(1),
        chunk(0),
        8,
        TranscriptViewportPlacement::Top,
    )));

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.turn, turn(1));
            assert_eq!(anchor.anchor_chunk, chunk(0));
            assert_eq!(anchor.rendered_chunk_range, 0..1);
            assert_eq!(anchor.last_navigation_direction, None);
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn layout_invalidation_preserves_semantic_streamed_anchor_identity() {
    for reason in [
        TranscriptViewportInvalidation::Width,
        TranscriptViewportInvalidation::Theme,
        TranscriptViewportInvalidation::Font,
        TranscriptViewportInvalidation::Media,
        TranscriptViewportInvalidation::CodePanel,
    ] {
        let mut state = streamed_state();
        state.apply_scroll(
            transcript_viewport::TranscriptViewportScrollInput::touchpad(
                TranscriptViewportNavigationDirection::Down,
                px(20.0),
                Some(streamed_frame(12, 4, 7, px(10.0), px(80.0))),
            )
            .with_rendered_frame(current_streamed_rendered_frame(
                turn(2),
                chunk(5),
                12,
                px(10.0),
                px(80.0),
            )),
        );

        let outcome = state.invalidate_layout(reason);

        assert!(outcome.changed);
        assert!(outcome.semantic_refill);
        match state.mode() {
            TranscriptViewportMode::Streamed(anchor) => {
                assert_eq!(anchor.turn, turn(2));
                assert_eq!(anchor.anchor_chunk, chunk(5));
                assert_eq!(anchor.rendered_chunk_range, 5..6);
                assert_eq!(anchor.local_anchor_offset, None);
            }
            other => panic!("expected streamed viewport, got {other:?}"),
        }
    }
}

#[test]
fn layout_invalidation_preserves_ordinary_turn_identity_without_global_pixel_offset() {
    let mut state = TranscriptViewportState::default();
    state.anchor_ordinary(turn(4), TranscriptViewportPlacement::Bottom, px(72.0));

    let outcome = state.invalidate_layout(TranscriptViewportInvalidation::Theme);

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    match state.mode() {
        TranscriptViewportMode::Ordinary(anchor) => {
            assert_eq!(anchor.turn, turn(4));
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Top);
            assert_eq!(anchor.local_offset, px(0.0));
        }
        other => panic!("expected ordinary viewport, got {other:?}"),
    }
}

#[test]
fn scroll_up_clamps_at_first_streamed_chunk_boundary() {
    let mut state = TranscriptViewportState::default();
    state.anchor_streamed(turn(0), chunk(0), 4, TranscriptViewportPlacement::Top);
    let outcome = state.apply_scroll(
        transcript_viewport::TranscriptViewportScrollInput::wheel(
            TranscriptViewportNavigationDirection::Up,
            px(20.0),
            Some(streamed_frame(4, 0, 2, px(0.0), px(60.0))),
        )
        .with_rendered_frame(current_streamed_rendered_frame(
            turn(0),
            chunk(0),
            4,
            px(0.0),
            px(60.0),
        )),
    );

    assert!(!outcome.semantic_refill);
    assert_eq!(outcome.boundary, Some(TranscriptViewportBoundary::Start));
    assert_eq!(outcome.residual_delta, Some(px(20.0)));
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(0));
            assert_eq!(anchor.rendered_chunk_range, 0..2);
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn scroll_down_clamps_at_last_streamed_chunk_boundary() {
    let mut state = TranscriptViewportState::default();
    state.anchor_streamed(turn(0), chunk(3), 4, TranscriptViewportPlacement::Bottom);
    let outcome = state.apply_scroll(
        transcript_viewport::TranscriptViewportScrollInput::wheel(
            TranscriptViewportNavigationDirection::Down,
            px(20.0),
            Some(streamed_frame(4, 2, 4, px(60.0), px(60.0))),
        )
        .with_rendered_frame(current_streamed_rendered_frame(
            turn(0),
            chunk(3),
            4,
            px(60.0),
            px(60.0),
        )),
    );

    assert!(!outcome.semantic_refill);
    assert_eq!(outcome.boundary, Some(TranscriptViewportBoundary::End));
    assert_eq!(outcome.residual_delta, Some(px(20.0)));
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(3));
            assert_eq!(anchor.rendered_chunk_range, 2..4);
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn manual_navigation_detaches_live_tail_following() {
    let mut state = TranscriptViewportState::default();
    state.reset_to_tail(3);
    let frame = TranscriptViewportFrame::new(
        vec![measured_segment(
            TranscriptFrameSegmentKey::ordinary_row(turn(2)),
            px(120.0),
        )],
        0..1,
        px(0.0),
        px(80.0),
    );

    let outcome = state.apply_page_to_frame(TranscriptViewportNavigationDirection::Up, &frame);

    assert!(outcome.changed);
    assert!(outcome.live_autoscroll_detached);
    assert_eq!(
        state.live_autoscroll(),
        TranscriptViewportLiveAutoscroll::Detached
    );
}

#[test]
fn streamed_boundary_scroll_detaches_live_tail_following() {
    let mut state = TranscriptViewportState::default();
    state.anchor_streamed(turn(0), chunk(0), 4, TranscriptViewportPlacement::Top);
    state.follow_live_tail();

    let outcome = state.apply_scroll(
        transcript_viewport::TranscriptViewportScrollInput::wheel(
            TranscriptViewportNavigationDirection::Up,
            px(20.0),
            Some(streamed_frame(4, 0, 2, px(0.0), px(60.0))),
        )
        .with_rendered_frame(current_streamed_rendered_frame(
            turn(0),
            chunk(0),
            4,
            px(0.0),
            px(60.0),
        )),
    );

    assert!(outcome.changed);
    assert!(outcome.live_autoscroll_detached);
    assert!(!outcome.semantic_refill);
    assert_eq!(outcome.boundary, Some(TranscriptViewportBoundary::Start));
    assert_eq!(
        state.live_autoscroll(),
        TranscriptViewportLiveAutoscroll::Detached
    );
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(0));
            assert_eq!(anchor.rendered_chunk_range, 0..2);
        }
        other => panic!("expected streamed viewport, got {other:?}"),
    }
}

#[test]
fn row_mutations_shift_semantic_turn_anchor_without_preserving_pixel_offset() {
    let mut state = TranscriptViewportState::default();
    state.anchor_ordinary(turn(5), TranscriptViewportPlacement::Bottom, px(48.0));

    state.reconcile_row_mutation(TranscriptViewportRowMutation::Inserted { index: 2, count: 3 });
    match state.mode() {
        TranscriptViewportMode::Ordinary(anchor) => {
            assert_eq!(anchor.turn.turn_index, 8);
            assert_eq!(anchor.local_offset, px(0.0));
        }
        other => panic!("expected ordinary viewport, got {other:?}"),
    }

    state.reconcile_row_mutation(TranscriptViewportRowMutation::Removed { index: 1, count: 2 });
    match state.mode() {
        TranscriptViewportMode::Ordinary(anchor) => {
            assert_eq!(anchor.turn.turn_index, 6);
            assert_eq!(anchor.local_offset, px(0.0));
        }
        other => panic!("expected ordinary viewport, got {other:?}"),
    }
}

#[test]
fn source_defines_no_dedicated_chunk_to_chunk_navigation_command() {
    let shell_source = include_str!("../src/shell.rs");
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let viewport_source = include_str!("../src/shell/transcript_viewport.rs");

    for source in [shell_source, transcript_source, viewport_source] {
        assert!(!source.contains("JumpTranscriptChunk"));
        assert!(!source.contains("TranscriptChunkJump"));
        assert!(!source.contains("chunk_to_chunk"));
        assert!(!source.contains("chunk-to-chunk"));
    }
    assert!(shell_source.contains("JumpTranscriptTurnUp"));
    assert!(shell_source.contains("JumpTranscriptTurnDown"));
}
