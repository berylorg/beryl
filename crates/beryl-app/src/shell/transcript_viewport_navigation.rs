use gpui::{Context, Pixels, ScrollWheelEvent, Window, px};

use super::{
    ConversationSurfaceState, ListOffset, ListScrollEvent, ListScrollPosition, ShellView,
    TranscriptHistoryBoundaryState,
    transcript_viewport::{
        TranscriptStreamedNavigationFrame, TranscriptViewportChunkAnchor, TranscriptViewportMode,
        TranscriptViewportNavigationDirection, TranscriptViewportPlacement,
        TranscriptViewportScrollInput, TranscriptViewportTurnAnchor, TranscriptViewportTurnTarget,
        TranscriptViewportTurnTargetKind,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TranscriptContentScrollSignature {
    pub(super) visible_range: std::ops::Range<usize>,
    pub(super) item_count: usize,
    pub(super) scroll_position: ListScrollPosition,
    pub(super) user_scrolled: bool,
    pub(super) anchor_preserves_offset: bool,
    pub(super) resident_boundary: TranscriptHistoryBoundaryState,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptStreamedNavigationSnapshot {
    pub(crate) row_index: usize,
    pub(crate) row_identity: String,
    pub(crate) target: TranscriptViewportTurnTarget,
    pub(crate) frame: TranscriptStreamedNavigationFrame,
}

#[derive(Clone, Copy, Debug)]
enum TranscriptViewportScrollKindForShell {
    Wheel,
    Touchpad,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TranscriptViewportNavigationApplication {
    pub(super) changed: bool,
    pub(super) consumed: bool,
}

impl ConversationSurfaceState {
    pub(super) fn transcript_content_scroll_signature(
        &self,
        event: &ListScrollEvent,
    ) -> TranscriptContentScrollSignature {
        let source_visible_range = self
            .transcript_presentation
            .source_range_for_presentation_range(&event.visible_range);
        TranscriptContentScrollSignature {
            visible_range: event.visible_range.clone(),
            item_count: event.count,
            scroll_position: self.transcript_list_state.scroll_position(),
            user_scrolled: self.transcript_user_scrolled,
            anchor_preserves_offset: self.transcript_live_scroll_preserves_anchor_offset(),
            resident_boundary: self
                .transcript_history_window
                .boundary_state_for_visible_range(&source_visible_range),
        }
    }

    pub(super) fn note_transcript_content_scroll_signature(
        &mut self,
        event: &ListScrollEvent,
    ) -> bool {
        let signature = self.transcript_content_scroll_signature(event);
        if self.last_transcript_content_scroll_signature.as_ref() == Some(&signature) {
            return false;
        }
        self.last_transcript_content_scroll_signature = Some(signature);
        true
    }

    pub(super) fn replace_transcript_streamed_navigation_snapshot(
        &mut self,
        snapshot: Option<TranscriptStreamedNavigationSnapshot>,
    ) -> bool {
        if self.transcript_streamed_navigation_snapshot == snapshot {
            return false;
        }
        self.transcript_streamed_navigation_snapshot = snapshot;
        true
    }

    pub(super) fn current_transcript_streamed_navigation_snapshot(
        &self,
    ) -> Option<&TranscriptStreamedNavigationSnapshot> {
        self.transcript_streamed_navigation_snapshot.as_ref()
    }

    pub(super) fn transcript_viewport_turn_target_for_row(
        &self,
        row_index: usize,
        streamed_placement: TranscriptViewportPlacement,
    ) -> Option<TranscriptViewportTurnTarget> {
        let row = self.transcript_presentation.turn_at(row_index)?;
        let turn = TranscriptViewportTurnAnchor::new(
            row.index,
            Some(row.identity.as_str().to_string()),
            row.turn.thread_id.clone(),
            row.turn.turn_id.clone(),
        );
        let chunks = row.model.chunk_presentation().chunks();
        if row.model.chunk_presentation().requires_chunking() && !chunks.is_empty() {
            let chunk_index = match streamed_placement {
                TranscriptViewportPlacement::Top => 0,
                TranscriptViewportPlacement::Bottom => chunks.len().saturating_sub(1),
            };
            let chunk = chunks.get(chunk_index)?;
            return Some(TranscriptViewportTurnTarget::streamed(
                turn,
                TranscriptViewportChunkAnchor::new(chunk_index, chunk.identity.clone()),
                chunks.len(),
                streamed_placement,
            ));
        }
        Some(TranscriptViewportTurnTarget::ordinary(turn))
    }

    pub(super) fn transcript_viewport_turn_target_for_scroll_position(
        &self,
        position: &ListScrollPosition,
    ) -> Option<TranscriptViewportTurnTarget> {
        match position {
            ListScrollPosition::Bottom => {
                let last = self.transcript_presentation.len().checked_sub(1)?;
                self.transcript_viewport_turn_target_for_row(
                    last,
                    TranscriptViewportPlacement::Bottom,
                )
            }
            ListScrollPosition::Content(offset) => self.transcript_viewport_turn_target_for_row(
                offset
                    .item_ix
                    .min(self.transcript_presentation.len().saturating_sub(1)),
                TranscriptViewportPlacement::Top,
            ),
            ListScrollPosition::VirtualTail { .. } => {
                let last = self.transcript_presentation.len().checked_sub(1)?;
                self.transcript_viewport_turn_target_for_row(
                    last,
                    TranscriptViewportPlacement::Bottom,
                )
            }
        }
    }

    fn apply_transcript_viewport_scroll(
        &mut self,
        direction: TranscriptViewportNavigationDirection,
        distance: Pixels,
        kind: TranscriptViewportScrollKindForShell,
        streamed: Option<TranscriptStreamedNavigationSnapshot>,
    ) -> TranscriptViewportNavigationApplication {
        let Some(streamed) = streamed else {
            let outcome = self.transcript_viewport.apply_scroll(match kind {
                TranscriptViewportScrollKindForShell::Wheel => {
                    TranscriptViewportScrollInput::wheel(direction, distance, None)
                }
                TranscriptViewportScrollKindForShell::Touchpad => {
                    TranscriptViewportScrollInput::touchpad(direction, distance, None)
                }
            });
            return TranscriptViewportNavigationApplication {
                changed: outcome.changed,
                consumed: false,
            };
        };

        if !self
            .transcript_viewport_is_streamed_for(streamed.row_index, streamed.row_identity.as_str())
        {
            self.transcript_viewport
                .apply_turn_jump(Some(streamed.target.clone()));
        }

        let input = match kind {
            TranscriptViewportScrollKindForShell::Wheel => {
                TranscriptViewportScrollInput::wheel(direction, distance, Some(streamed.frame))
            }
            TranscriptViewportScrollKindForShell::Touchpad => {
                TranscriptViewportScrollInput::touchpad(direction, distance, Some(streamed.frame))
            }
        };
        let outcome = self.transcript_viewport.apply_scroll(input);
        self.apply_transcript_viewport_reduce_outcome(outcome, Some(streamed.row_index))
    }

    pub(super) fn apply_transcript_viewport_page(
        &mut self,
        direction: TranscriptViewportNavigationDirection,
    ) -> TranscriptViewportNavigationApplication {
        let streamed = self
            .current_transcript_streamed_navigation_snapshot()
            .cloned();
        let Some(streamed) = streamed else {
            let outcome = self.transcript_viewport.apply_page(direction, None);
            return TranscriptViewportNavigationApplication {
                changed: outcome.changed,
                consumed: false,
            };
        };

        if !self
            .transcript_viewport_is_streamed_for(streamed.row_index, streamed.row_identity.as_str())
        {
            self.transcript_viewport
                .apply_turn_jump(Some(streamed.target.clone()));
        }
        let outcome = self
            .transcript_viewport
            .apply_page(direction, Some(streamed.frame));
        self.apply_transcript_viewport_reduce_outcome(outcome, Some(streamed.row_index))
    }

    pub(super) fn apply_transcript_viewport_turn_jump(
        &mut self,
        target: TranscriptViewportTurnTarget,
        fallback_position: ListScrollPosition,
    ) -> TranscriptViewportNavigationApplication {
        let row_index = target.turn.turn_index;
        let outcome = self
            .transcript_viewport
            .apply_turn_jump(Some(target.clone()));
        match target.kind {
            TranscriptViewportTurnTargetKind::Ordinary => {
                self.transcript_list_state
                    .scroll_to_position(fallback_position);
            }
            TranscriptViewportTurnTargetKind::Streamed { placement, .. } => {
                self.transcript_list_state
                    .invalidate_item_measurement(row_index);
                self.scroll_transcript_streamed_row_to_placement(row_index, placement);
            }
        }
        TranscriptViewportNavigationApplication {
            changed: outcome.changed,
            consumed: true,
        }
    }

    fn transcript_viewport_is_streamed_for(&self, row_index: usize, row_identity: &str) -> bool {
        matches!(
            self.transcript_viewport.mode(),
            TranscriptViewportMode::Streamed(anchor)
                if anchor.turn.turn_index == row_index
                    || anchor.turn.row_identity.as_deref() == Some(row_identity)
        )
    }

    fn apply_transcript_viewport_reduce_outcome(
        &mut self,
        outcome: super::transcript_viewport::TranscriptViewportReduceOutcome,
        row_index: Option<usize>,
    ) -> TranscriptViewportNavigationApplication {
        if let (Some(row_index), Some(local_offset)) = (row_index, outcome.local_scroll_offset) {
            self.transcript_list_state
                .scroll_to_position(ListScrollPosition::Content(ListOffset {
                    item_ix: row_index,
                    offset_in_item: local_offset,
                }));
        }
        if outcome.semantic_refill
            && let Some(row_index) = row_index
        {
            self.transcript_list_state
                .invalidate_item_measurement(row_index);
            if let TranscriptViewportMode::Streamed(anchor) = self.transcript_viewport.mode() {
                self.scroll_transcript_streamed_row_to_placement(row_index, anchor.placement);
            }
        }
        TranscriptViewportNavigationApplication {
            changed: outcome.changed,
            consumed: outcome.local_scroll_offset.is_some()
                || outcome.semantic_refill
                || outcome.live_autoscroll_detached,
        }
    }

    fn scroll_transcript_streamed_row_to_placement(
        &self,
        row_index: usize,
        placement: TranscriptViewportPlacement,
    ) {
        let offset = match placement {
            TranscriptViewportPlacement::Top => px(0.0),
            TranscriptViewportPlacement::Bottom => {
                let viewport_height = self.transcript_list_state.viewport_bounds().size.height;
                self.transcript_list_state
                    .measured_item_size(row_index)
                    .map(|size| (size.height - viewport_height).max(px(0.0)))
                    .unwrap_or(px(0.0))
            }
        };
        self.transcript_list_state
            .scroll_to_position(ListScrollPosition::Content(ListOffset {
                item_ix: row_index,
                offset_in_item: offset,
            }));
    }
}

impl ShellView {
    pub(super) fn apply_transcript_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        streamed: Option<TranscriptStreamedNavigationSnapshot>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let delta = event.delta.pixel_delta(window.line_height());
        if delta.y == px(0.0) || delta.x.abs() > delta.y.abs() {
            return false;
        }
        let direction = if delta.y > px(0.0) {
            TranscriptViewportNavigationDirection::Up
        } else {
            TranscriptViewportNavigationDirection::Down
        };
        let kind = if event.delta.precise() {
            TranscriptViewportScrollKindForShell::Touchpad
        } else {
            TranscriptViewportScrollKindForShell::Wheel
        };
        let application = self
            .conversation_surface_mut()
            .map(|surface| {
                let application = surface.apply_transcript_viewport_scroll(
                    direction,
                    delta.y.abs(),
                    kind,
                    streamed,
                );
                if application.consumed {
                    surface.release_transcript_submit_anchor();
                }
                application
            })
            .unwrap_or_default();
        if !application.consumed {
            return false;
        }

        let event = self.conversation_surface().map(|surface| {
            let list_state = surface.transcript_list_state();
            ListScrollEvent {
                visible_range: list_state.visible_range(),
                count: list_state.item_count(),
                is_scrolled: !matches!(list_state.scroll_position(), ListScrollPosition::Bottom),
            }
        });
        if let Some(event) = event {
            self.note_transcript_scroll_event(&event, window, cx);
        }
        if application.changed {
            self.notify_transcript_panel(cx);
            cx.notify();
        }
        true
    }
}
