use beryl_state::WindowClaimSelection;
use syndic_storage::{DraftPieceOperationIdV1, PreparedDraftEditorCandidateSessionAbandonFreshV1};

use crate::composer_host::{ComposerHostFlushTicket, SyndicComposerHost};

use super::{MainWindowComposerActivationReceipt, MainWindowComposerSelectionIdentity};

pub(super) struct SelectedComposer {
    pub(super) identity: MainWindowComposerSelectionIdentity,
    pub(super) host: SyndicComposerHost,
}

pub(super) enum PendingStage {
    Ready,
    Publishing(ComposerHostFlushTicket),
    Retiring,
    Reconciliation,
    Departed,
}

pub(super) struct PendingComposer {
    pub(super) receipt: MainWindowComposerActivationReceipt,
    pub(super) claim: WindowClaimSelection,
    pub(super) retirement_operation_id: DraftPieceOperationIdV1,
    pub(super) host: SyndicComposerHost,
    pub(super) stage: PendingStage,
    pub(super) abandonment: Option<PreparedDraftEditorCandidateSessionAbandonFreshV1>,
}
