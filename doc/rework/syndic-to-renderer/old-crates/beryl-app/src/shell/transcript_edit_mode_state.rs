use super::{
    composer_draft::AcceptedComposerDraft,
    transcript_edit_menu_state::{TranscriptEditRequest, TranscriptEditTarget},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptEditModeState {
    target: TranscriptEditTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptEditModeSnapshot {
    source_thread_id: String,
    source_turn_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptEditSubmitContext {
    pub(crate) status_operation_active: bool,
    pub(crate) active_turn_active: bool,
    pub(crate) selected_thread_compaction_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptEditSubmitRoute {
    EditCommit,
}

impl TranscriptEditModeState {
    pub(crate) fn from_request(request: TranscriptEditRequest) -> Self {
        Self {
            target: request.into_target(),
        }
    }

    pub(crate) fn target(&self) -> &TranscriptEditTarget {
        &self.target
    }

    pub(crate) fn source_thread_id(&self) -> &str {
        self.target.source_thread_id()
    }

    pub(crate) fn source_turn_index(&self) -> usize {
        self.target.source_turn_index()
    }

    #[allow(dead_code)]
    pub(crate) fn rollback_turn_count(&self) -> u32 {
        self.target.rollback_turn_count()
    }

    #[allow(dead_code)]
    pub(crate) fn draft_seed_text(&self) -> String {
        self.target.draft_seed_text()
    }

    pub(crate) fn draft_seed(&self) -> &AcceptedComposerDraft {
        self.target.draft_seed()
    }

    pub(crate) fn snapshot(&self) -> TranscriptEditModeSnapshot {
        TranscriptEditModeSnapshot {
            source_thread_id: self.source_thread_id().to_string(),
            source_turn_index: self.source_turn_index(),
        }
    }

    pub(crate) fn remains_valid(
        &self,
        selected_thread_id: Option<&str>,
        source_thread_idle: bool,
        selected_thread_compaction_active: bool,
        pending_thread_activation: bool,
        target_loaded: bool,
    ) -> bool {
        selected_thread_id == Some(self.source_thread_id())
            && source_thread_idle
            && !selected_thread_compaction_active
            && !pending_thread_activation
            && target_loaded
    }
}

impl TranscriptEditModeSnapshot {
    pub(crate) fn dims_row(&self, row_thread_id: Option<&str>, source_turn_index: usize) -> bool {
        row_thread_id == Some(self.source_thread_id.as_str())
            && source_turn_index >= self.source_turn_index
    }
}

pub(crate) fn transcript_edit_submit_route(
    edit_mode: Option<&TranscriptEditModeState>,
    context: TranscriptEditSubmitContext,
) -> Option<TranscriptEditSubmitRoute> {
    let _normal_submit_would_have_conflicts = context.status_operation_active
        || context.active_turn_active
        || context.selected_thread_compaction_active;
    edit_mode.map(|_| TranscriptEditSubmitRoute::EditCommit)
}

pub(crate) fn cancel_transcript_edit_mode_slot(
    edit_mode: &mut Option<TranscriptEditModeState>,
) -> bool {
    edit_mode.take().is_some()
}
