mod admission;
mod adoption;
mod drive;
mod recovery;
pub use recovery::ComposerHostHistoricalRestoration;
pub(in crate::composer_host) use recovery::HistoricalPreflush;
pub(in crate::composer_host) use recovery::RetiredHistoricalReplacement;

use super::{ComposerHostBinding, ComposerHostError, SyndicComposerHost};
use beryl_home_store::{HomeStore, ReconciliationHandle};
use syndic_storage::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostHistoricalTarget {
    pub thread_id: beryl_model::SyndicThreadId,
    pub turn_id: beryl_model::SyndicTurnId,
    pub item_id: beryl_model::SyndicItemId,
    pub transcript: CurrentTranscriptEntryProof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostHistoricalTicket {
    binding: ComposerHostBinding,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostHistoricalCompletion {
    previous: ComposerHostBinding,
    binding: ComposerHostBinding,
    intent: DraftSubmissionIntent,
    caret: Option<gpui_text_input::SourcePosition>,
    selection: Option<gpui_text_input::SourcePosition>,
}

impl ComposerHostHistoricalCompletion {
    pub const fn previous_binding(self) -> ComposerHostBinding {
        self.previous
    }
    pub const fn binding(self) -> ComposerHostBinding {
        self.binding
    }
    pub const fn intent(self) -> DraftSubmissionIntent {
        self.intent
    }
    pub const fn caret(self) -> Option<gpui_text_input::SourcePosition> {
        self.caret
    }
    pub const fn selection(self) -> Option<gpui_text_input::SourcePosition> {
        self.selection
    }
    pub const fn requires_rebind(self) -> bool {
        self.caret.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostHistoricalAdvance {
    Progress,
    Ready(ComposerHostHistoricalCompletion),
    NotCommitted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostHistoricalStage {
    Beginning,
    Importing,
    Starting,
    Cancelling,
    Terminating,
    Reconciling,
    PublicationReady,
    NotCommitted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposerHostHistoricalRecovery {
    Unadmitted,
    ImportOpen,
    OriginalOutcome(DraftReplacementCommandOutcomeV1),
    PublicationReady(ComposerHostHistoricalCompletion),
}

pub(super) struct PendingHistoricalReplacement {
    prior_publication_binding: ComposerHostBinding,
    ticket: ComposerHostHistoricalTicket,
    import: Option<PreparedDraftReplacementImportV1>,
    position: DraftCompositePositionV1,
    assets: Option<beryl_state::AssetState>,
    at: Option<SyndicTimestamp>,
    stage: HistoricalStage,
    pub(super) cancelled: bool,
}

enum HistoricalStage {
    Begin,
    Streaming,
    Quantum {
        begin: bool,
        outcome: QuantumOutcome,
    },
    Start {
        prepared: PreparedDraftReplacementStartV1,
        executed: bool,
    },
    Cancel {
        prepared: PreparedDraftReplacementCancellationV1,
        executed: bool,
    },
    Terminate {
        prepared: PreparedDraftReplacementImportTerminationV1,
        executed: bool,
    },
    Ready(ComposerHostHistoricalCompletion),
    NotCommitted,
}

enum QuantumOutcome {
    Committed,
    Noncommit,
    Pending(ReconciliationHandle),
}

impl SyndicComposerHost {
    pub fn submission_intent(&self) -> Option<DraftSubmissionIntent> {
        self.active
            .as_ref()
            .and_then(|active| active.submission_intent)
    }

    pub fn historical_replacement_pending(&self) -> bool {
        self.historical_replacement.is_some()
    }

    pub fn historical_replacement_ticket(&self) -> Option<ComposerHostHistoricalTicket> {
        self.historical_replacement
            .as_ref()
            .map(|pending| pending.ticket)
    }

    pub fn historical_replacement_stage(&self) -> Option<ComposerHostHistoricalStage> {
        self.historical_replacement
            .as_ref()
            .map(|pending| match &pending.stage {
                HistoricalStage::Begin => ComposerHostHistoricalStage::Beginning,
                HistoricalStage::Streaming => ComposerHostHistoricalStage::Importing,
                HistoricalStage::Quantum { .. } => ComposerHostHistoricalStage::Reconciling,
                HistoricalStage::Start {
                    executed: false, ..
                } => ComposerHostHistoricalStage::Starting,
                HistoricalStage::Cancel {
                    executed: false, ..
                } => ComposerHostHistoricalStage::Cancelling,
                HistoricalStage::Terminate {
                    executed: false, ..
                } => ComposerHostHistoricalStage::Terminating,
                HistoricalStage::Start { executed: true, .. }
                | HistoricalStage::Cancel { executed: true, .. }
                | HistoricalStage::Terminate { executed: true, .. } => {
                    ComposerHostHistoricalStage::Reconciling
                }
                HistoricalStage::Ready(_) => ComposerHostHistoricalStage::PublicationReady,
                HistoricalStage::NotCommitted => ComposerHostHistoricalStage::NotCommitted,
            })
    }

    pub fn acknowledge_historical_replacement(
        &mut self,
        ticket: ComposerHostHistoricalTicket,
        binding: ComposerHostBinding,
    ) -> Result<(), ComposerHostError> {
        let pending = self
            .historical_replacement
            .as_ref()
            .ok_or(ComposerHostError::OldBinding)?;
        if pending.ticket != ticket
            || self.binding() != Some(binding)
            || !matches!(pending.stage, HistoricalStage::Ready(c) if c.binding == binding)
        {
            return Err(ComposerHostError::OldBinding);
        }
        self.historical_replacement = None;
        Ok(())
    }

    fn historical_admission(
        &self,
        store: &HomeStore,
        binding: ComposerHostBinding,
    ) -> Result<(), ComposerHostError> {
        super::request::validate_store(binding, store)?;
        if self.binding() != Some(binding) {
            return Err(ComposerHostError::OldBinding);
        }
        if self.live_operation_pending()
            || !self.detached_mutations.is_empty()
            || !self.detached_history.is_empty()
            || self.publication.lane.is_some()
            || self.submission_pending()
            || self.lifecycle.has_barrier()
            || self.lifecycle.is_service_disposed()
            || self.is_unavailable()
        {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        Ok(())
    }
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}
