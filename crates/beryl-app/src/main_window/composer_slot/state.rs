use beryl_state::WindowClaimSelection;
use syndic_storage::{
    DraftEditorCurrentSelectorV1, DraftPieceOperationIdV1,
    PreparedDraftEditorCandidateSessionAbandonFreshV1,
};

use crate::composer_host::{ComposerHostFlushTicket, SyndicComposerHost};

use super::MainWindowComposerDispatcher;
use super::{MainWindowComposerActivationReceipt, MainWindowComposerSelectionIdentity};
use crate::main_window::MainWindowComposerDraftState;

pub(super) struct SelectedComposer {
    pub(super) identity: MainWindowComposerSelectionIdentity,
    pub(super) dispatcher: MainWindowComposerDispatcher,
    pub(super) draft_state: MainWindowComposerDraftState,
    pub(super) host: SyndicComposerHost,
}

#[derive(Clone, Copy)]
pub(super) enum PendingStage {
    Ready,
    Publishing(ComposerHostFlushTicket),
    AwaitingWidgetRelease,
    Finalizing,
    Retiring,
    Reconciliation,
    Departed,
}

#[derive(Clone, Copy)]
pub(super) enum DisposalStage {
    Flushing(ComposerHostFlushTicket),
    AwaitingWidgetRelease,
}

pub(super) struct PendingComposer {
    pub(super) receipt: MainWindowComposerActivationReceipt,
    pub(super) claim: WindowClaimSelection,
    pub(super) retirement_operation_id: DraftPieceOperationIdV1,
    pub(super) host: SyndicComposerHost,
    pub(super) dispatcher: super::MainWindowComposerDispatcher,
    pub(super) source_selector: Option<DraftEditorCurrentSelectorV1>,
    pub(super) stage: PendingStage,
    pub(super) abandonment: Option<PreparedDraftEditorCandidateSessionAbandonFreshV1>,
}
