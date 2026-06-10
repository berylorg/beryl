#[allow(dead_code)]
#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;

#[allow(dead_code)]
#[path = "../src/shell/transcript_anchor.rs"]
mod transcript_anchor;

#[allow(dead_code)]
#[path = "../src/shell/transcript_live_scroll.rs"]
mod transcript_live_scroll;

use gpui::px;
use transcript_anchor::TranscriptSubmitAnchor;
use transcript_live_scroll::{
    TranscriptFinalAnchor, TranscriptLiveScrollDetachedPhase, TranscriptLiveScrollPhase,
    TranscriptLiveScrollState, TranscriptLiveTurnAnchor, TranscriptNarrativeAnchor,
};

fn turn_anchor(index: usize) -> TranscriptLiveTurnAnchor {
    TranscriptLiveTurnAnchor::new(
        index,
        Some(format!("thread:main:turn:{index}")),
        Some("main".to_string()),
        Some(format!("turn-{index}")),
    )
}

fn prompt_anchor(index: usize) -> TranscriptSubmitAnchor {
    TranscriptSubmitAnchor::new(
        index,
        Some(format!("thread:main:turn:{index}")),
        0,
        "submitted prompt".to_string(),
    )
}

fn final_anchor(index: usize) -> TranscriptFinalAnchor {
    TranscriptFinalAnchor {
        turn: turn_anchor(index),
        item_id: "final-answer".to_string(),
    }
}

fn commentary_anchor(index: usize) -> TranscriptNarrativeAnchor {
    TranscriptNarrativeAnchor {
        turn: turn_anchor(index),
        item_id: Some("commentary".to_string()),
    }
}

#[test]
fn new_user_turn_starts_prompt_reread_snapshot() {
    let mut state = TranscriptLiveScrollState::inactive();

    state.start_prompt_reread(prompt_anchor(4));

    assert!(state.preserves_content_anchor_offset());
    let snapshot = state.prompt_submit_anchor_snapshot().unwrap();
    assert_eq!(snapshot.turn_index, 4);
    assert_eq!(snapshot.fragment_index, 0);
    assert_eq!(snapshot.user_input, "submitted prompt");
    assert_eq!(
        snapshot.viewport_action,
        transcript_anchor::TranscriptSubmitViewportAction::PromptReread
    );
}

#[test]
fn applied_prompt_reread_keeps_runway_without_reissuing_scroll() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(4));

    let applied_anchor = state.prompt_submit_anchor_snapshot().unwrap();

    assert!(state.mark_prompt_reread_applied(&applied_anchor));
    assert!(!state.mark_prompt_reread_applied(&applied_anchor));

    let snapshot = state.prompt_submit_anchor_snapshot().unwrap();
    assert_eq!(snapshot.turn_index, 4);
    assert_eq!(
        snapshot.viewport_action,
        transcript_anchor::TranscriptSubmitViewportAction::MaintainPromptRunway
    );
}

#[test]
fn stale_prompt_reread_defer_does_not_apply_new_anchor() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(1));
    let stale_anchor = state.prompt_submit_anchor_snapshot().unwrap();
    state.start_prompt_reread(prompt_anchor(2));

    assert!(!state.mark_prompt_reread_applied(&stale_anchor));

    let snapshot = state.prompt_submit_anchor_snapshot().unwrap();
    assert_eq!(snapshot.turn_index, 2);
    assert_eq!(
        snapshot.viewport_action,
        transcript_anchor::TranscriptSubmitViewportAction::PromptReread
    );
    assert!(state.mark_prompt_reread_applied(&snapshot));
}

#[test]
fn steering_preserves_every_active_state_except_commentary_target() {
    let states = [
        {
            let mut state = TranscriptLiveScrollState::inactive();
            state.clear_for_tail_activation();
            state
        },
        {
            let mut state = TranscriptLiveScrollState::inactive();
            state.start_prompt_reread(prompt_anchor(1));
            state
        },
        {
            let mut state = TranscriptLiveScrollState::inactive();
            state.transition_to_final_start(final_anchor(1));
            state
        },
        {
            let mut state = TranscriptLiveScrollState::inactive();
            state.transition_to_final_start(final_anchor(1));
            state.mark_final_start_applied(&final_anchor(1), px(24.0));
            state
        },
        {
            let mut state = TranscriptLiveScrollState::inactive();
            state.start_prompt_reread(prompt_anchor(1));
            state.detach_for_manual_scroll();
            state
        },
    ];

    for mut state in states {
        let expected = state.clone();
        state.preserve_for_steering();
        assert_eq!(state, expected);
    }
}

#[test]
fn steering_during_commentary_follow_keeps_phase_and_follows_latest_block() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.transition_to_commentary_follow(commentary_anchor(1));

    state.preserve_for_steering();

    assert!(matches!(
        state.phase(),
        TranscriptLiveScrollPhase::CommentaryFollow { anchor }
            if anchor.turn.turn_index == 1 && anchor.item_id.is_none()
    ));
}

#[test]
fn commentary_after_prompt_preserves_prompt_runway_until_overflow_applies_follow() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(1));
    let prompt_snapshot = state.prompt_submit_anchor_snapshot().unwrap();
    assert!(state.mark_prompt_reread_applied(&prompt_snapshot));

    assert!(state.transition_to_commentary_follow(commentary_anchor(1)));
    assert!(!state.transition_to_commentary_follow(commentary_anchor(1)));

    assert!(state.preserves_content_anchor_offset());
    assert!(matches!(
        state.phase(),
        TranscriptLiveScrollPhase::PromptReread { .. }
    ));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::PromptWithPendingCommentary {
            prompt,
            commentary
        }) if prompt.turn_index == 1
            && prompt.viewport_action
                == transcript_anchor::TranscriptSubmitViewportAction::MaintainPromptRunway
            && commentary.turn.turn_index == 1
            && commentary.item_id.as_deref() == Some("commentary")
    ));
}

#[test]
fn measured_commentary_overflow_promotes_prompt_to_commentary_follow() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(1));
    assert!(state.transition_to_commentary_follow(commentary_anchor(1)));

    assert!(state.mark_commentary_follow_applied(&commentary_anchor(1)));

    assert!(!state.preserves_content_anchor_offset());
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::CommentaryFollow(anchor))
            if anchor.turn.turn_index == 1 && anchor.item_id.as_deref() == Some("commentary")
    ));
}

#[test]
fn stale_commentary_follow_defer_does_not_apply_new_prompt_anchor() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(1));
    assert!(state.transition_to_commentary_follow(commentary_anchor(1)));
    let stale_anchor = commentary_anchor(1);

    state.start_prompt_reread(prompt_anchor(2));
    assert!(state.transition_to_commentary_follow(commentary_anchor(2)));

    assert!(!state.mark_commentary_follow_applied(&stale_anchor));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::PromptWithPendingCommentary {
            prompt,
            commentary
        }) if prompt.turn_index == 2 && commentary.turn.turn_index == 2
    ));
}

#[test]
fn final_start_overrides_pending_commentary_without_bottom_follow() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(1));
    assert!(state.transition_to_commentary_follow(commentary_anchor(1)));

    assert!(state.transition_to_final_start(final_anchor(1)));

    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalStart(anchor))
            if anchor.turn.turn_index == 1 && anchor.item_id == "final-answer"
    ));
    assert!(!state.mark_commentary_follow_applied(&commentary_anchor(1)));
}

#[test]
fn steering_during_pending_commentary_keeps_prompt_phase_and_follows_latest_block_if_needed() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(1));
    assert!(state.transition_to_commentary_follow(commentary_anchor(1)));

    state.preserve_for_steering();

    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::PromptWithPendingCommentary {
            commentary,
            ..
        }) if commentary.turn.turn_index == 1 && commentary.item_id.is_none()
    ));
}

#[test]
fn manual_scroll_detaches_live_prompt_once() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(2));

    assert!(state.detach_for_manual_scroll());
    assert!(matches!(
        state.phase(),
        TranscriptLiveScrollPhase::DetachedManual {
            previous_phase: TranscriptLiveScrollDetachedPhase::PromptReread,
            turn: Some(turn)
        } if turn.turn_index == 2
            && turn.row_identity.as_deref() == Some("thread:main:turn:2")
    ));
    assert!(!state.detach_for_manual_scroll());
}

#[test]
fn manual_scroll_before_final_preserves_prompt_runway_without_reissuing_scroll() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(2));
    let prompt_snapshot = state.prompt_submit_anchor_snapshot().unwrap();
    assert!(state.mark_prompt_reread_applied(&prompt_snapshot));

    assert!(state.detach_for_manual_scroll());

    assert!(state.preserves_content_anchor_offset());
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::Prompt(snapshot))
            if snapshot.turn_index == 2
                && snapshot.row_identity.as_deref() == Some("thread:main:turn:2")
                && snapshot.viewport_action
                    == transcript_anchor::TranscriptSubmitViewportAction::MaintainPromptRunway
    ));
}

#[test]
fn manual_scroll_after_final_preserves_final_runway_without_reissuing_scroll() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(2));
    let prompt_snapshot = state.prompt_submit_anchor_snapshot().unwrap();
    assert!(state.mark_prompt_reread_applied(&prompt_snapshot));
    assert!(state.transition_to_final_start(final_anchor(2)));
    assert!(state.mark_final_start_applied(&final_anchor(2), px(24.0)));

    assert!(state.detach_for_manual_scroll());

    assert!(state.preserves_content_anchor_offset());
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRunway(anchor))
            if anchor.turn.turn_index == 2
                && anchor.turn.row_identity.as_deref() == Some("thread:main:turn:2")
                && anchor.item_id == "final-answer"
    ));
}

#[test]
fn manual_scroll_before_final_records_final_runway_when_final_later_arrives() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(2));
    assert!(state.detach_for_manual_scroll());

    assert!(!state.transition_to_final_start(final_anchor(2)));

    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRunway(anchor))
            if anchor.turn.turn_index == 2 && anchor.item_id == "final-answer"
    ));
}

#[test]
fn detached_manual_ignores_final_runway_from_another_turn() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(2));
    assert!(state.detach_for_manual_scroll());

    assert!(!state.transition_to_final_start(final_anchor(3)));

    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::Prompt(snapshot))
            if snapshot.turn_index == 2
                && snapshot.viewport_action
                    == transcript_anchor::TranscriptSubmitViewportAction::MaintainPromptRunway
    ));
}

#[test]
fn existing_thread_tail_activation_clears_manual_runway() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(2));
    assert!(state.transition_to_final_start(final_anchor(2)));
    state.detach_for_manual_scroll();

    state.clear_for_tail_activation();

    assert!(!state.preserves_content_anchor_offset());
    assert_eq!(state.effect_snapshot(), None);
}

#[test]
fn loaded_history_final_runway_preserves_tail_activation_without_autoscroll() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.clear_for_tail_activation();

    state.set_passive_final_runway(final_anchor(2));

    assert!(!state.preserves_content_anchor_offset());
    assert!(matches!(
        state.phase(),
        TranscriptLiveScrollPhase::TailActivation
    ));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRunway(anchor))
            if anchor.turn.turn_index == 2
                && anchor.turn.row_identity.as_deref() == Some("thread:main:turn:2")
                && anchor.item_id == "final-answer"
    ));

    assert!(state.detach_for_manual_scroll());
    assert!(state.preserves_content_anchor_offset());
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRunway(anchor))
            if anchor.turn.turn_index == 2 && anchor.item_id == "final-answer"
    ));
}

#[test]
fn loaded_history_final_detection_refreshes_passive_runway_without_final_start() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.set_passive_final_runway(final_anchor(2));

    assert!(!state.transition_to_final_start(final_anchor(2)));

    assert!(!state.preserves_content_anchor_offset());
    assert!(matches!(
        state.phase(),
        TranscriptLiveScrollPhase::TailActivation
    ));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRunway(anchor))
            if anchor.turn.turn_index == 2 && anchor.item_id == "final-answer"
    ));
}

#[test]
fn tail_activation_late_final_records_passive_runway_without_autoscroll() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.clear_for_tail_activation();

    assert!(!state.transition_to_final_start(final_anchor(2)));

    assert!(!state.preserves_content_anchor_offset());
    assert!(matches!(
        state.phase(),
        TranscriptLiveScrollPhase::TailActivation
    ));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRunway(anchor))
            if anchor.turn.turn_index == 2 && anchor.item_id == "final-answer"
    ));
}

#[test]
fn loaded_history_detail_refresh_records_passive_runway_only_for_activation_scope() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.clear_for_tail_activation();

    assert!(state.refresh_loaded_history_final_runway(final_anchor(2)));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRunway(anchor))
            if anchor.turn.turn_index == 2 && anchor.item_id == "final-answer"
    ));

    assert!(state.detach_for_manual_scroll());
    assert!(state.refresh_loaded_history_final_runway(final_anchor(2)));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRunway(anchor))
            if anchor.turn.turn_index == 2 && anchor.item_id == "final-answer"
    ));

    state.start_prompt_reread(prompt_anchor(3));

    assert!(!state.refresh_loaded_history_final_runway(final_anchor(2)));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::Prompt(snapshot))
            if snapshot.turn_index == 3
    ));
}

#[test]
fn detached_tail_activation_late_final_records_passive_runway_without_autoscroll() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.clear_for_tail_activation();
    assert!(state.detach_for_manual_scroll());

    assert!(!state.transition_to_final_start(final_anchor(2)));

    assert!(state.preserves_content_anchor_offset());
    assert!(matches!(
        state.phase(),
        TranscriptLiveScrollPhase::DetachedManual {
            previous_phase: TranscriptLiveScrollDetachedPhase::TailActivation,
            turn: None,
        }
    ));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRunway(anchor))
            if anchor.turn.turn_index == 2 && anchor.item_id == "final-answer"
    ));
}

#[test]
fn new_user_turn_replaces_passive_loaded_history_runway() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.set_passive_final_runway(final_anchor(2));

    state.start_prompt_reread(prompt_anchor(3));

    assert!(state.preserves_content_anchor_offset());
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::Prompt(snapshot))
            if snapshot.turn_index == 3
                && snapshot.viewport_action
                    == transcript_anchor::TranscriptSubmitViewportAction::PromptReread
    ));
}

#[test]
fn late_commentary_does_not_move_final_read_backward() {
    let mut state = TranscriptLiveScrollState::inactive();
    assert!(state.transition_to_final_start(final_anchor(3)));
    assert!(state.mark_final_start_applied(&final_anchor(3), px(24.0)));

    assert!(
        !state.transition_to_commentary_follow(TranscriptNarrativeAnchor {
            turn: turn_anchor(3),
            item_id: Some("late-commentary".to_string()),
        })
    );
    assert!(matches!(
        state.phase(),
        TranscriptLiveScrollPhase::FinalRead {
            anchor,
            applied_scroll_offset
        } if anchor.turn.turn_index == 3
            && anchor.item_id == "final-answer"
            && *applied_scroll_offset == px(24.0)
    ));
}

#[test]
fn final_start_snapshot_is_consumed_once() {
    let mut state = TranscriptLiveScrollState::inactive();
    assert!(!state.preserves_content_anchor_offset());
    assert!(state.transition_to_final_start(final_anchor(3)));
    assert!(state.preserves_content_anchor_offset());
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalStart(anchor))
            if anchor.item_id == "final-answer"
    ));

    assert!(state.mark_final_start_applied(&final_anchor(3), px(36.0)));
    assert!(state.preserves_content_anchor_offset());
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRead(read))
            if read.anchor.item_id == "final-answer" && read.applied_scroll_offset == px(36.0)
    ));
    assert!(!state.mark_final_start_applied(&final_anchor(3), px(36.0)));
}

#[test]
fn final_read_offset_updates_are_anchor_keyed() {
    let mut state = TranscriptLiveScrollState::inactive();
    assert!(state.transition_to_final_start(final_anchor(3)));
    assert!(state.mark_final_start_applied(&final_anchor(3), px(36.0)));

    assert!(!state.mark_final_start_applied(&final_anchor(2), px(48.0)));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRead(read))
            if read.anchor.turn.turn_index == 3 && read.applied_scroll_offset == px(36.0)
    ));

    assert!(state.mark_final_start_applied(&final_anchor(3), px(48.0)));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRead(read))
            if read.anchor.turn.turn_index == 3 && read.applied_scroll_offset == px(48.0)
    ));
    assert!(!state.mark_final_start_applied(&final_anchor(3), px(48.0)));
}

#[test]
fn stale_final_start_defer_does_not_apply_new_anchor() {
    let mut state = TranscriptLiveScrollState::inactive();
    assert!(state.transition_to_final_start(final_anchor(1)));
    let stale_anchor = final_anchor(1);
    assert!(state.transition_to_final_start(final_anchor(2)));

    assert!(!state.mark_final_start_applied(&stale_anchor, px(24.0)));
    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalStart(anchor))
            if anchor.turn.turn_index == 2 && anchor.item_id == "final-answer"
    ));
    assert!(state.mark_final_start_applied(&final_anchor(2), px(48.0)));
}

#[test]
fn terminal_event_does_not_create_end_anchor() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.transition_to_commentary_follow(commentary_anchor(5));
    let expected = state.clone();

    assert!(!state.note_terminal_event());
    assert_eq!(state, expected);
}

#[test]
fn prepended_history_shifts_semantic_turn_indexes() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(2));

    state.shift_turn_index(3);

    let snapshot = state.prompt_submit_anchor_snapshot().unwrap();
    assert_eq!(snapshot.turn_index, 5);
    assert_eq!(snapshot.row_identity.as_deref(), Some("thread:main:turn:2"));
}

#[test]
fn prepended_history_shifts_detached_manual_prompt_runway() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(2));
    state.detach_for_manual_scroll();

    state.shift_turn_index(3);

    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::Prompt(snapshot))
            if snapshot.turn_index == 5
                && snapshot.row_identity.as_deref() == Some("thread:main:turn:2")
                && snapshot.viewport_action
                    == transcript_anchor::TranscriptSubmitViewportAction::MaintainPromptRunway
    ));
}

#[test]
fn prepended_history_shifts_detached_manual_final_runway() {
    let mut state = TranscriptLiveScrollState::inactive();
    state.start_prompt_reread(prompt_anchor(2));
    assert!(state.transition_to_final_start(final_anchor(2)));
    assert!(state.detach_for_manual_scroll());

    state.shift_turn_index(3);

    assert!(matches!(
        state.effect_snapshot(),
        Some(transcript_live_scroll::TranscriptLiveScrollEffectSnapshot::FinalRunway(anchor))
            if anchor.turn.turn_index == 5
                && anchor.turn.row_identity.as_deref() == Some("thread:main:turn:2")
                && anchor.item_id == "final-answer"
    ));
}
