#![allow(dead_code)]

// Phase 1 defines the full live-turn scroll state vocabulary before later
// phases wire every state transition into transcript measurement and rendering.

use super::transcript_anchor::{
    TranscriptSubmitAnchor, TranscriptSubmitAnchorSnapshot, TranscriptSubmitViewportAction,
};
use gpui::Pixels;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptLiveScrollState {
    phase: TranscriptLiveScrollPhase,
    last_prompt_anchor: Option<TranscriptSubmitAnchor>,
    last_final_anchor: Option<TranscriptFinalAnchor>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum TranscriptLiveScrollPhase {
    #[default]
    Inactive,
    TailActivation,
    PromptReread {
        anchor: TranscriptSubmitAnchor,
        applied: bool,
        pending_commentary: Option<TranscriptNarrativeAnchor>,
    },
    CommentaryFollow {
        anchor: TranscriptNarrativeAnchor,
    },
    FinalStart {
        anchor: TranscriptFinalAnchor,
    },
    FinalRead {
        anchor: TranscriptFinalAnchor,
        applied_scroll_offset: Pixels,
    },
    DetachedManual {
        previous_phase: TranscriptLiveScrollDetachedPhase,
        turn: Option<TranscriptLiveTurnAnchor>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptLiveScrollDetachedPhase {
    TailActivation,
    PromptReread,
    CommentaryFollow,
    FinalStart,
    FinalRead,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptLiveTurnAnchor {
    pub(crate) turn_index: usize,
    pub(crate) row_identity: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) turn_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptNarrativeAnchor {
    pub(crate) turn: TranscriptLiveTurnAnchor,
    pub(crate) item_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptFinalAnchor {
    pub(crate) turn: TranscriptLiveTurnAnchor,
    pub(crate) item_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptFinalReadAnchor {
    pub(crate) anchor: TranscriptFinalAnchor,
    pub(crate) applied_scroll_offset: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TranscriptLiveScrollEffectSnapshot {
    Prompt(TranscriptSubmitAnchorSnapshot),
    PromptWithPendingCommentary {
        prompt: TranscriptSubmitAnchorSnapshot,
        commentary: TranscriptNarrativeAnchor,
    },
    CommentaryFollow(TranscriptNarrativeAnchor),
    FinalStart(TranscriptFinalAnchor),
    FinalRead(TranscriptFinalReadAnchor),
    FinalRunway(TranscriptFinalAnchor),
}

impl TranscriptLiveScrollState {
    pub(crate) fn inactive() -> Self {
        Self {
            phase: TranscriptLiveScrollPhase::Inactive,
            last_prompt_anchor: None,
            last_final_anchor: None,
        }
    }

    pub(crate) fn tail_activation() -> Self {
        Self {
            phase: TranscriptLiveScrollPhase::TailActivation,
            last_prompt_anchor: None,
            last_final_anchor: None,
        }
    }

    pub(crate) fn phase(&self) -> &TranscriptLiveScrollPhase {
        &self.phase
    }

    pub(crate) fn clear_inactive(&mut self) {
        self.phase = TranscriptLiveScrollPhase::Inactive;
        self.last_prompt_anchor = None;
        self.last_final_anchor = None;
    }

    pub(crate) fn clear_for_tail_activation(&mut self) {
        self.phase = TranscriptLiveScrollPhase::TailActivation;
        self.last_prompt_anchor = None;
        self.last_final_anchor = None;
    }

    pub(crate) fn start_prompt_reread(&mut self, anchor: TranscriptSubmitAnchor) {
        self.last_prompt_anchor = Some(anchor.clone());
        self.last_final_anchor = None;
        self.phase = TranscriptLiveScrollPhase::PromptReread {
            anchor,
            applied: false,
            pending_commentary: None,
        };
    }

    pub(crate) fn preserve_for_steering(&mut self) {
        match &mut self.phase {
            TranscriptLiveScrollPhase::PromptReread {
                pending_commentary: Some(anchor),
                ..
            }
            | TranscriptLiveScrollPhase::CommentaryFollow { anchor } => {
                anchor.item_id = None;
            }
            _ => {}
        }
    }

    pub(crate) fn prompt_submit_anchor_snapshot(&self) -> Option<TranscriptSubmitAnchorSnapshot> {
        match &self.phase {
            TranscriptLiveScrollPhase::PromptReread {
                anchor, applied, ..
            } => Some(anchor.snapshot(if *applied {
                TranscriptSubmitViewportAction::MaintainPromptRunway
            } else {
                TranscriptSubmitViewportAction::PromptReread
            })),
            _ => None,
        }
    }

    pub(crate) fn effect_snapshot(&self) -> Option<TranscriptLiveScrollEffectSnapshot> {
        match &self.phase {
            TranscriptLiveScrollPhase::PromptReread {
                pending_commentary, ..
            } => self.prompt_submit_anchor_snapshot().map(|prompt| {
                if let Some(commentary) = pending_commentary {
                    TranscriptLiveScrollEffectSnapshot::PromptWithPendingCommentary {
                        prompt,
                        commentary: commentary.clone(),
                    }
                } else {
                    TranscriptLiveScrollEffectSnapshot::Prompt(prompt)
                }
            }),
            TranscriptLiveScrollPhase::CommentaryFollow { anchor } => Some(
                TranscriptLiveScrollEffectSnapshot::CommentaryFollow(anchor.clone()),
            ),
            TranscriptLiveScrollPhase::FinalStart { anchor } => Some(
                TranscriptLiveScrollEffectSnapshot::FinalStart(anchor.clone()),
            ),
            TranscriptLiveScrollPhase::FinalRead {
                anchor,
                applied_scroll_offset,
            } => Some(TranscriptLiveScrollEffectSnapshot::FinalRead(
                TranscriptFinalReadAnchor {
                    anchor: anchor.clone(),
                    applied_scroll_offset: *applied_scroll_offset,
                },
            )),
            TranscriptLiveScrollPhase::DetachedManual { .. } => {
                if let Some(anchor) = &self.last_final_anchor {
                    return Some(TranscriptLiveScrollEffectSnapshot::FinalRunway(
                        anchor.clone(),
                    ));
                }
                self.last_prompt_anchor.as_ref().map(|anchor| {
                    TranscriptLiveScrollEffectSnapshot::Prompt(
                        anchor.snapshot(TranscriptSubmitViewportAction::MaintainPromptRunway),
                    )
                })
            }
            TranscriptLiveScrollPhase::Inactive | TranscriptLiveScrollPhase::TailActivation => None,
        }
    }

    pub(crate) fn preserves_content_anchor_offset(&self) -> bool {
        match self.phase {
            TranscriptLiveScrollPhase::PromptReread { .. }
            | TranscriptLiveScrollPhase::FinalStart { .. }
            | TranscriptLiveScrollPhase::FinalRead { .. } => true,
            TranscriptLiveScrollPhase::DetachedManual { .. } => {
                self.last_final_anchor.is_some() || self.last_prompt_anchor.is_some()
            }
            TranscriptLiveScrollPhase::Inactive | TranscriptLiveScrollPhase::TailActivation => {
                false
            }
            TranscriptLiveScrollPhase::CommentaryFollow { .. } => false,
        }
    }

    pub(crate) fn detach_for_manual_scroll(&mut self) -> bool {
        let Some((previous_phase, turn)) = detached_phase_and_turn(&self.phase) else {
            return false;
        };
        self.phase = TranscriptLiveScrollPhase::DetachedManual {
            previous_phase,
            turn,
        };
        true
    }

    pub(crate) fn shift_turn_index(&mut self, amount: usize) {
        match &mut self.phase {
            TranscriptLiveScrollPhase::PromptReread {
                anchor,
                pending_commentary,
                ..
            } => {
                anchor.shift_turn_index(amount);
                if let Some(anchor) = pending_commentary {
                    anchor.turn.shift_turn_index(amount);
                }
            }
            TranscriptLiveScrollPhase::CommentaryFollow { anchor } => {
                anchor.turn.shift_turn_index(amount);
            }
            TranscriptLiveScrollPhase::FinalStart { anchor }
            | TranscriptLiveScrollPhase::FinalRead { anchor, .. } => {
                anchor.turn.shift_turn_index(amount);
            }
            TranscriptLiveScrollPhase::DetachedManual { turn, .. } => {
                if let Some(turn) = turn {
                    turn.shift_turn_index(amount);
                }
            }
            TranscriptLiveScrollPhase::Inactive | TranscriptLiveScrollPhase::TailActivation => {}
        }
        if let Some(anchor) = &mut self.last_prompt_anchor {
            anchor.shift_turn_index(amount);
        }
        if let Some(anchor) = &mut self.last_final_anchor {
            anchor.turn.shift_turn_index(amount);
        }
    }

    pub(crate) fn transition_to_commentary_follow(
        &mut self,
        anchor: TranscriptNarrativeAnchor,
    ) -> bool {
        match &mut self.phase {
            TranscriptLiveScrollPhase::DetachedManual { .. }
            | TranscriptLiveScrollPhase::FinalStart { .. }
            | TranscriptLiveScrollPhase::FinalRead { .. } => return false,
            TranscriptLiveScrollPhase::CommentaryFollow { anchor: current }
                if current == &anchor =>
            {
                return false;
            }
            TranscriptLiveScrollPhase::PromptReread {
                pending_commentary, ..
            } if pending_commentary.as_ref() == Some(&anchor) => {
                return false;
            }
            TranscriptLiveScrollPhase::PromptReread {
                pending_commentary, ..
            } => {
                *pending_commentary = Some(anchor);
                return true;
            }
            TranscriptLiveScrollPhase::Inactive
            | TranscriptLiveScrollPhase::TailActivation
            | TranscriptLiveScrollPhase::CommentaryFollow { .. } => {}
        }
        self.phase = TranscriptLiveScrollPhase::CommentaryFollow { anchor };
        true
    }

    pub(crate) fn mark_commentary_follow_applied(
        &mut self,
        expected_anchor: &TranscriptNarrativeAnchor,
    ) -> bool {
        let matches_expected = match &self.phase {
            TranscriptLiveScrollPhase::PromptReread {
                pending_commentary, ..
            } => pending_commentary.as_ref() == Some(expected_anchor),
            _ => false,
        };
        if !matches_expected {
            return false;
        }
        self.phase = TranscriptLiveScrollPhase::CommentaryFollow {
            anchor: expected_anchor.clone(),
        };
        true
    }

    pub(crate) fn transition_to_final_start(&mut self, anchor: TranscriptFinalAnchor) -> bool {
        match &self.phase {
            TranscriptLiveScrollPhase::DetachedManual { turn, .. } => {
                if detached_turn_matches(turn.as_ref(), &anchor.turn) {
                    self.last_final_anchor = Some(anchor);
                }
                return false;
            }
            TranscriptLiveScrollPhase::FinalRead { .. } => return false,
            TranscriptLiveScrollPhase::FinalStart { anchor: current } if current == &anchor => {
                return false;
            }
            TranscriptLiveScrollPhase::Inactive
            | TranscriptLiveScrollPhase::TailActivation
            | TranscriptLiveScrollPhase::PromptReread { .. }
            | TranscriptLiveScrollPhase::CommentaryFollow { .. }
            | TranscriptLiveScrollPhase::FinalStart { .. } => {}
        }
        self.last_final_anchor = Some(anchor.clone());
        self.phase = TranscriptLiveScrollPhase::FinalStart { anchor };
        true
    }

    pub(crate) fn mark_prompt_reread_applied(
        &mut self,
        expected_anchor: &TranscriptSubmitAnchorSnapshot,
    ) -> bool {
        let TranscriptLiveScrollPhase::PromptReread {
            anchor, applied, ..
        } = &mut self.phase
        else {
            return false;
        };
        if !prompt_anchor_matches_snapshot(anchor, expected_anchor) {
            return false;
        }
        if *applied {
            return false;
        }
        *applied = true;
        true
    }

    pub(crate) fn mark_final_start_applied(
        &mut self,
        expected_anchor: &TranscriptFinalAnchor,
        applied_scroll_offset: Pixels,
    ) -> bool {
        match &mut self.phase {
            TranscriptLiveScrollPhase::FinalStart { anchor } if anchor == expected_anchor => {
                let anchor = anchor.clone();
                self.last_final_anchor = Some(anchor.clone());
                self.phase = TranscriptLiveScrollPhase::FinalRead {
                    anchor,
                    applied_scroll_offset,
                };
                true
            }
            TranscriptLiveScrollPhase::FinalRead {
                anchor,
                applied_scroll_offset: current_offset,
            } if anchor == expected_anchor && *current_offset != applied_scroll_offset => {
                *current_offset = applied_scroll_offset;
                true
            }
            _ => false,
        }
    }

    pub(crate) fn note_terminal_event(&mut self) -> bool {
        false
    }
}

fn prompt_anchor_matches_snapshot(
    anchor: &TranscriptSubmitAnchor,
    expected: &TranscriptSubmitAnchorSnapshot,
) -> bool {
    let current = anchor.snapshot(expected.viewport_action);
    current.turn_index == expected.turn_index
        && current.row_identity == expected.row_identity
        && current.fragment_index == expected.fragment_index
        && current.user_input == expected.user_input
}

fn detached_turn_matches(
    detached_turn: Option<&TranscriptLiveTurnAnchor>,
    candidate: &TranscriptLiveTurnAnchor,
) -> bool {
    let Some(detached_turn) = detached_turn else {
        return false;
    };
    if detached_turn.turn_index != candidate.turn_index {
        return false;
    }
    optional_identity_matches(
        detached_turn.row_identity.as_deref(),
        candidate.row_identity.as_deref(),
    ) && optional_identity_matches(
        detached_turn.thread_id.as_deref(),
        candidate.thread_id.as_deref(),
    ) && optional_identity_matches(
        detached_turn.turn_id.as_deref(),
        candidate.turn_id.as_deref(),
    )
}

fn optional_identity_matches(left: Option<&str>, right: Option<&str>) -> bool {
    left.is_none() || right.is_none() || left == right
}

impl TranscriptLiveTurnAnchor {
    pub(crate) fn new(
        turn_index: usize,
        row_identity: Option<String>,
        thread_id: Option<String>,
        turn_id: Option<String>,
    ) -> Self {
        Self {
            turn_index,
            row_identity,
            thread_id,
            turn_id,
        }
    }

    fn shift_turn_index(&mut self, amount: usize) {
        self.turn_index = self.turn_index.saturating_add(amount);
    }
}

fn detached_phase_and_turn(
    phase: &TranscriptLiveScrollPhase,
) -> Option<(
    TranscriptLiveScrollDetachedPhase,
    Option<TranscriptLiveTurnAnchor>,
)> {
    match phase {
        TranscriptLiveScrollPhase::Inactive | TranscriptLiveScrollPhase::DetachedManual { .. } => {
            None
        }
        TranscriptLiveScrollPhase::TailActivation => {
            Some((TranscriptLiveScrollDetachedPhase::TailActivation, None))
        }
        TranscriptLiveScrollPhase::PromptReread { anchor, .. } => Some((
            TranscriptLiveScrollDetachedPhase::PromptReread,
            Some(TranscriptLiveTurnAnchor::new(
                anchor.turn_index(),
                anchor.row_identity().map(str::to_string),
                None,
                None,
            )),
        )),
        TranscriptLiveScrollPhase::CommentaryFollow { anchor } => Some((
            TranscriptLiveScrollDetachedPhase::CommentaryFollow,
            Some(anchor.turn.clone()),
        )),
        TranscriptLiveScrollPhase::FinalStart { anchor } => Some((
            TranscriptLiveScrollDetachedPhase::FinalStart,
            Some(anchor.turn.clone()),
        )),
        TranscriptLiveScrollPhase::FinalRead { anchor, .. } => Some((
            TranscriptLiveScrollDetachedPhase::FinalRead,
            Some(anchor.turn.clone()),
        )),
    }
}
