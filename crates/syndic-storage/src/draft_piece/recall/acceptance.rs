use std::sync::Arc;

use super::super::{
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionDisposeReceiptV1,
    DraftEditorCandidateSessionIdV1, DraftPieceOperationIdV1,
};
use crate::{AcceptedInputOrdinal, AcceptedRouteGeneration};
use crate::{ContentReference, FirstAcceptance, InputGateState};
use beryl_model::{
    SealedAssetReferenceSetProof, SyndicDraftId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposerAcceptedInputReferenceV1 {
    pub(crate) home: beryl_model::BerylHomeId,
    pub(crate) acceptance: Arc<FirstAcceptance>,
}

impl FirstAcceptance {
    pub fn composer_accepted_input_reference(
        &self,
        home: beryl_model::BerylHomeId,
    ) -> ComposerAcceptedInputReferenceV1 {
        ComposerAcceptedInputReferenceV1 {
            home,
            acceptance: Arc::new(self.clone()),
        }
    }
}

impl ComposerAcceptedInputReferenceV1 {
    pub fn home_id(&self) -> beryl_model::BerylHomeId {
        self.home
    }
    pub fn thread_id(&self) -> SyndicThreadId {
        self.acceptance.thread_id()
    }
    pub fn source_draft_id(&self) -> SyndicDraftId {
        self.acceptance.draft_id()
    }
    pub fn session_id(&self) -> DraftEditorCandidateSessionIdV1 {
        self.acceptance.candidate().session_id()
    }
    pub fn operation_id(&self) -> DraftPieceOperationIdV1 {
        self.acceptance.session_disposal_operation_id()
    }
    pub fn content(&self) -> ContentReference {
        self.acceptance.materialization().content()
    }
    pub fn asset_reference_set(&self) -> Option<SealedAssetReferenceSetProof> {
        self.acceptance.asset_reference_set()
    }
    pub fn same_content(&self, other: &Self) -> bool {
        self.content() == other.content()
            && self.asset_reference_set().map(|p| p.ordered_assets())
                == other.asset_reference_set().map(|p| p.ordered_assets())
    }
    pub(crate) fn key(&self) -> super::super::DraftEditorCandidateSessionRecordKeyV1 {
        super::super::DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
            self.source_draft_id(),
            self.session_id(),
            self.operation_id(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AcceptanceResultV1 {
    Idle {
        turn: SyndicTurnId,
        item: SyndicItemId,
    },
    Accepted {
        input: beryl_model::SyndicAcceptedInputId,
        ordinal: AcceptedInputOrdinal,
        generation: AcceptedRouteGeneration,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AcceptanceDisposalV1 {
    pub(crate) request: FirstAcceptance,
    pub(crate) result: AcceptanceResultV1,
}

impl AcceptanceDisposalV1 {
    pub(crate) fn idle(request: FirstAcceptance) -> Self {
        let result = AcceptanceResultV1::Idle {
            turn: request.submitted_turn_id(),
            item: request.idle_user_item_id(),
        };
        Self { request, result }
    }
    pub(crate) fn accepted(
        request: FirstAcceptance,
        ordinal: AcceptedInputOrdinal,
        generation: AcceptedRouteGeneration,
    ) -> Self {
        let result = AcceptanceResultV1::Accepted {
            input: request.accepted_input_id(),
            ordinal,
            generation,
        };
        Self { request, result }
    }
    pub(crate) fn is_exact(&self, disposal: &DraftEditorCandidateSessionDisposeReceiptV1) -> bool {
        let request = &self.request;
        let before = disposal.before_head();
        let after = disposal.after_head();
        let content = request.materialization().content();
        let summary = content.summary();
        DraftEditorCandidateActivationBindingV1::from_head(before) == request.candidate()
            && before.thread_id() == request.thread_id()
            && before.draft_id() == request.draft_id()
            && before.published_selector_revision() == request.expected_draft_revision()
            && before.active_operation().is_none()
            && request.expected_image_label_authority().is_exact()
            && request.expected_image_label_authority().thread_id() == request.thread_id()
            && after.disposal_operation_id() == Some(request.session_disposal_operation_id())
            && disposal.frontier().reference() == request.candidate().history()
            && request.materialization().key().source() == request.candidate().root()
            && content.encoding() == crate::ContentEncoding::ComposerV1
            && summary.atom_count() != 0
            && summary.encoded_bytes() >= 9
            && summary.logical_utf8_bytes()
                == request.candidate().root().summary().logical_utf8_bytes()
            && summary.image_marker_count() == request.candidate().root().summary().marker_count()
            && match request.asset_reference_set() {
                Some(proof) => {
                    summary.image_marker_count() != 0
                        && proof.sequential().marker_count() == summary.image_marker_count()
                        && proof.sequential().marker_digest() == summary.marker_digest()
                        && proof.sequential().maximum_image_label() == summary.maximum_image_label()
                        && proof.ordered_assets().marker_count() == summary.image_marker_count()
                }
                None => summary.image_marker_count() == 0,
            }
            && request.draft_id() != request.next_draft_id()
            && match self.result {
                AcceptanceResultV1::Idle { turn, item } => {
                    matches!(request.expected_gate_state(), InputGateState::Idle)
                        && turn == request.submitted_turn_id()
                        && item == request.idle_user_item_id()
                }
                AcceptanceResultV1::Accepted { input, .. } => {
                    !matches!(request.expected_gate_state(), InputGateState::Idle)
                        && input == request.accepted_input_id()
                }
            }
    }
}
