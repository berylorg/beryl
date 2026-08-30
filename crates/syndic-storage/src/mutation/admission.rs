use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, MutationContribution, ReconciliationReservation,
};
use beryl_model::{
    DraftRevision, InputGateRevision, SealedAssetReferenceSetProof, SyndicDraftId, SyndicItemId,
    SyndicThreadId, ThreadRevision,
};

use crate::{
    DraftComposerMaterializationRecordV1, DraftEditorCandidateActivationBindingV1,
    DraftPieceOperationIdV1, ImageLabelAuthorityHeadV1, InputGateState, SyndicMutationError,
    SyndicStorage, SyndicTimestamp, codec::*, domain::SyndicDomain,
};

mod accepted;
mod idle;
mod shared;
mod successor;

use shared::AcceptanceRecords;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirstAcceptanceKind {
    Idle { user_item_id: SyndicItemId },
    Accepted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirstAcceptanceStatus {
    ExactOld,
    ExactNew(FirstAcceptanceKind),
    Collision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirstAcceptance {
    thread_id: SyndicThreadId,
    expected_thread_revision: ThreadRevision,
    expected_image_label_authority: ImageLabelAuthorityHeadV1,
    draft_id: SyndicDraftId,
    expected_draft_revision: DraftRevision,
    candidate: DraftEditorCandidateActivationBindingV1,
    materialization: DraftComposerMaterializationRecordV1,
    expected_gate_revision: InputGateRevision,
    expected_gate_state: InputGateState,
    next_draft_id: SyndicDraftId,
    idle_user_item_id: SyndicItemId,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
    session_disposal_operation_id: DraftPieceOperationIdV1,
    admitted_at: SyndicTimestamp,
}

impl FirstAcceptance {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_thread_revision: ThreadRevision,
        expected_image_label_authority: ImageLabelAuthorityHeadV1,
        draft_id: SyndicDraftId,
        expected_draft_revision: DraftRevision,
        candidate: DraftEditorCandidateActivationBindingV1,
        materialization: DraftComposerMaterializationRecordV1,
        expected_gate_revision: InputGateRevision,
        expected_gate_state: InputGateState,
        next_draft_id: SyndicDraftId,
        idle_user_item_id: SyndicItemId,
        asset_reference_set: Option<SealedAssetReferenceSetProof>,
        session_disposal_operation_id: DraftPieceOperationIdV1,
        admitted_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            expected_thread_revision,
            expected_image_label_authority,
            draft_id,
            expected_draft_revision,
            candidate,
            materialization,
            expected_gate_revision,
            expected_gate_state,
            next_draft_id,
            idle_user_item_id,
            asset_reference_set,
            session_disposal_operation_id,
            admitted_at,
        }
    }

    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    pub const fn expected_thread_revision(&self) -> ThreadRevision {
        self.expected_thread_revision
    }
    pub const fn expected_image_label_authority(&self) -> ImageLabelAuthorityHeadV1 {
        self.expected_image_label_authority
    }
    pub const fn draft_id(&self) -> SyndicDraftId {
        self.draft_id
    }
    pub const fn expected_draft_revision(&self) -> DraftRevision {
        self.expected_draft_revision
    }
    pub const fn candidate(&self) -> DraftEditorCandidateActivationBindingV1 {
        self.candidate
    }
    pub const fn materialization(&self) -> DraftComposerMaterializationRecordV1 {
        self.materialization
    }
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }
    pub const fn expected_gate_state(&self) -> &InputGateState {
        &self.expected_gate_state
    }
    pub const fn next_draft_id(&self) -> SyndicDraftId {
        self.next_draft_id
    }
    pub const fn idle_user_item_id(&self) -> SyndicItemId {
        self.idle_user_item_id
    }
    pub const fn asset_reference_set(&self) -> Option<SealedAssetReferenceSetProof> {
        self.asset_reference_set
    }
    pub const fn session_disposal_operation_id(&self) -> DraftPieceOperationIdV1 {
        self.session_disposal_operation_id
    }
    pub const fn admitted_at(&self) -> SyndicTimestamp {
        self.admitted_at
    }
    pub const fn submitted_turn_id(&self) -> beryl_model::SyndicTurnId {
        self.draft_id.submitted_turn_id()
    }
    pub const fn accepted_input_id(&self) -> beryl_model::SyndicAcceptedInputId {
        self.draft_id.accepted_input_id()
    }
}

impl SyndicStorage {
    pub fn first_acceptance(
        &self,
        expected_domain_revision: beryl_model::DomainRevision,
        acceptance: FirstAcceptance,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            FirstAcceptanceMutation { acceptance },
        )
    }
}

struct FirstAcceptanceMutation {
    acceptance: FirstAcceptance,
}

impl DomainMutation<SyndicDomain> for FirstAcceptanceMutation {
    type Error = SyndicMutationError;
    type Prepared = AcceptanceRecords;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        self.records(reader)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        shared::reserve_acceptance_records(reservation)?;
        if !matches!(self.acceptance.expected_gate_state(), InputGateState::Idle) {
            reservation.reserve_successor_source::<
                beryl_home_store::FirstAcceptancePromotionProtocolV1,
                _,
            >(successor::FirstAcceptancePromotionSourceV1)?;
        }
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        prepared.contribute(mutations)
    }
}

impl FirstAcceptanceMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<AcceptanceRecords, SyndicMutationError> {
        let base = shared::load_base(reader, &self.acceptance)?;
        if matches!(base.gate.state(), InputGateState::Idle) {
            idle::records(reader, &self.acceptance, base).map(AcceptanceRecords::Idle)
        } else {
            accepted::records(reader, &self.acceptance, base).map(AcceptanceRecords::Accepted)
        }
    }
}
