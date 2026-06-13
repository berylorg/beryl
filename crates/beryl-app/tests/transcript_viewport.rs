#[allow(dead_code)]
#[path = "../src/shell/transcript_viewport.rs"]
mod transcript_viewport;

use gpui::px;
use transcript_viewport::{
    TranscriptStreamedNavigationFrame, TranscriptViewportBoundary, TranscriptViewportChunkAnchor,
    TranscriptViewportInvalidation, TranscriptViewportLiveAutoscroll, TranscriptViewportMode,
    TranscriptViewportNavigationDirection, TranscriptViewportPlacement,
    TranscriptViewportRowMutation, TranscriptViewportState, TranscriptViewportTurnAnchor,
    TranscriptViewportTurnTarget,
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

fn streamed_state() -> TranscriptViewportState {
    let mut state = TranscriptViewportState::default();
    state.anchor_streamed(turn(2), chunk(5), 12, TranscriptViewportPlacement::Top);
    state
}

#[test]
fn wheel_scroll_consumes_rendered_chunk_window_before_semantic_anchor_moves() {
    let mut state = streamed_state();
    let outcome = state.apply_scroll(transcript_viewport::TranscriptViewportScrollInput::wheel(
        TranscriptViewportNavigationDirection::Up,
        px(16.0),
        Some(streamed_frame(12, 4, 7, px(40.0), px(120.0))),
    ));

    assert!(outcome.changed);
    assert_eq!(outcome.local_scroll_offset, Some(px(24.0)));
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
        ),
    );

    assert!(outcome.changed);
    assert_eq!(outcome.local_scroll_offset, Some(px(50.0)));
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
fn wheel_scroll_at_rendered_top_advances_to_previous_semantic_chunk_anchor() {
    let mut state = streamed_state();
    let outcome = state.apply_scroll(transcript_viewport::TranscriptViewportScrollInput::wheel(
        TranscriptViewportNavigationDirection::Up,
        px(24.0),
        Some(streamed_frame(12, 4, 7, px(0.0), px(80.0))),
    ));

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    assert_eq!(outcome.local_scroll_offset, None);
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(3));
            assert_eq!(anchor.rendered_chunk_range, 3..4);
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Top);
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
    let outcome = state.apply_scroll(transcript_viewport::TranscriptViewportScrollInput::wheel(
        TranscriptViewportNavigationDirection::Down,
        px(24.0),
        Some(streamed_frame(12, 4, 7, px(80.0), px(80.0))),
    ));

    assert!(outcome.changed);
    assert!(outcome.semantic_refill);
    match state.mode() {
        TranscriptViewportMode::Streamed(anchor) => {
            assert_eq!(anchor.anchor_chunk, chunk(7));
            assert_eq!(anchor.rendered_chunk_range, 7..8);
            assert_eq!(anchor.placement, TranscriptViewportPlacement::Bottom);
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
    let outcome = state.apply_page(
        TranscriptViewportNavigationDirection::Up,
        Some(streamed_frame(12, 4, 7, px(0.0), px(80.0))),
    );

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
    let outcome = state.apply_page(
        TranscriptViewportNavigationDirection::Down,
        Some(streamed_frame(12, 4, 7, px(0.0), px(80.0))),
    );

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
            ),
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
    let outcome = state.apply_scroll(transcript_viewport::TranscriptViewportScrollInput::wheel(
        TranscriptViewportNavigationDirection::Up,
        px(20.0),
        Some(streamed_frame(4, 0, 2, px(0.0), px(60.0))),
    ));

    assert!(!outcome.semantic_refill);
    assert_eq!(outcome.boundary, Some(TranscriptViewportBoundary::Start));
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
    let outcome = state.apply_scroll(transcript_viewport::TranscriptViewportScrollInput::wheel(
        TranscriptViewportNavigationDirection::Down,
        px(20.0),
        Some(streamed_frame(4, 2, 4, px(60.0), px(60.0))),
    ));

    assert!(!outcome.semantic_refill);
    assert_eq!(outcome.boundary, Some(TranscriptViewportBoundary::End));
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

    let outcome = state.apply_page(TranscriptViewportNavigationDirection::Up, None);

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

    let outcome = state.apply_scroll(transcript_viewport::TranscriptViewportScrollInput::wheel(
        TranscriptViewportNavigationDirection::Up,
        px(20.0),
        Some(streamed_frame(4, 0, 2, px(0.0), px(60.0))),
    ));

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
