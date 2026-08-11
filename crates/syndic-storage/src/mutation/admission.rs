use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, MutationContribution, ReconciliationReservation,
};
use beryl_model::{
    AcceptedInputRevision, DiscussionContextOwnerId, DomainRevision, DraftRevision,
    InputGateRevision, ProjectionRevision, SealedAssetReferenceSetProof, SyndicDraftId,
    SyndicItemId, SyndicThreadId, SyndicTurnId, ThreadRevision,
};

use crate::{
    AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedNextSourceRecord,
    AcceptedReadySourceRecord, AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord,
    AcceptedRouteHeadProof, AcceptedRouteLeafRecord, AcceptedRouteLeafState, AcceptedRouteRevision,
    AcceptedRouteTarget, BindingHeadRecord, BindingLifecycle, BindingRecord, BindingState,
    CanonicalItemRecord, ContentEncoding, ConversationParent, DraftByThreadRecord, DraftRecord,
    DraftSubmissionIntent, HistorySummaryRecord, ImageLabelOriginSpanRecord, InputGateRecord,
    InputGateState, NextTurnReason, PreparedContent, ProjectionLifecycle, SelectedPathProof,
    SyndicMutationError, SyndicRecordError, SyndicStorage, SyndicTimestamp,
    ThreadParentIndexRecord, ThreadRecord, TranscriptViewHeadRecord, TurnChildIndexRecord,
    TurnDepth, TurnItemIndexRecord, TurnItemOrdinal, TurnKind, TurnLifecycle, TurnRecord,
    TurnStateRecord, TurnStateRevision, child_turn_chain_digest, codec::*, domain::SyndicDomain,
    root_turn_chain_digest,
};

use super::{current_draft, point, required};

/// Stable result of reconciling one natural draft-derived admission identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputAdmissionStatus {
    Absent,
    ExactSubmitted,
    ExactAccepted,
    Collision,
}

/// Exact caller-owned identities and revisions for one idle draft submission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdleSubmission {
    thread_id: SyndicThreadId,
    expected_thread_revision: ThreadRevision,
    draft_id: SyndicDraftId,
    expected_draft_revision: DraftRevision,
    expected_content: crate::ContentReference,
    expected_gate_revision: InputGateRevision,
    next_draft_id: SyndicDraftId,
    user_item_id: SyndicItemId,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
    admitted_at: SyndicTimestamp,
}

impl IdleSubmission {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_thread_revision: ThreadRevision,
        draft_id: SyndicDraftId,
        expected_draft_revision: DraftRevision,
        expected_content: crate::ContentReference,
        expected_gate_revision: InputGateRevision,
        next_draft_id: SyndicDraftId,
        user_item_id: SyndicItemId,
        asset_reference_set: Option<SealedAssetReferenceSetProof>,
        admitted_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            expected_thread_revision,
            draft_id,
            expected_draft_revision,
            expected_content,
            expected_gate_revision,
            next_draft_id,
            user_item_id,
            asset_reference_set,
            admitted_at,
        }
    }

    #[must_use]
    pub const fn submitted_turn_id(&self) -> SyndicTurnId {
        self.draft_id.submitted_turn_id()
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn expected_thread_revision(&self) -> ThreadRevision {
        self.expected_thread_revision
    }

    #[must_use]
    pub const fn expected_draft_revision(&self) -> DraftRevision {
        self.expected_draft_revision
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn draft_id(&self) -> SyndicDraftId {
        self.draft_id
    }

    #[must_use]
    pub const fn expected_content(&self) -> crate::ContentReference {
        self.expected_content
    }

    #[must_use]
    pub const fn next_draft_id(&self) -> SyndicDraftId {
        self.next_draft_id
    }

    #[must_use]
    pub const fn user_item_id(&self) -> SyndicItemId {
        self.user_item_id
    }

    #[must_use]
    pub const fn asset_reference_set(&self) -> Option<SealedAssetReferenceSetProof> {
        self.asset_reference_set
    }

    #[must_use]
    pub const fn admitted_at(&self) -> SyndicTimestamp {
        self.admitted_at
    }
}

/// Exact caller-owned identities and revisions for one non-idle input admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedInputAdmission {
    thread_id: SyndicThreadId,
    expected_thread_revision: ThreadRevision,
    draft_id: SyndicDraftId,
    expected_draft_revision: DraftRevision,
    expected_content: crate::ContentReference,
    expected_gate_revision: InputGateRevision,
    next_draft_id: SyndicDraftId,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
    admitted_at: SyndicTimestamp,
}

impl AcceptedInputAdmission {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_thread_revision: ThreadRevision,
        draft_id: SyndicDraftId,
        expected_draft_revision: DraftRevision,
        expected_content: crate::ContentReference,
        expected_gate_revision: InputGateRevision,
        next_draft_id: SyndicDraftId,
        asset_reference_set: Option<SealedAssetReferenceSetProof>,
        admitted_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            expected_thread_revision,
            draft_id,
            expected_draft_revision,
            expected_content,
            expected_gate_revision,
            next_draft_id,
            asset_reference_set,
            admitted_at,
        }
    }

    #[must_use]
    pub const fn accepted_input_id(&self) -> beryl_model::SyndicAcceptedInputId {
        self.draft_id.accepted_input_id()
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn expected_thread_revision(&self) -> ThreadRevision {
        self.expected_thread_revision
    }

    #[must_use]
    pub const fn expected_draft_revision(&self) -> DraftRevision {
        self.expected_draft_revision
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn draft_id(&self) -> SyndicDraftId {
        self.draft_id
    }

    #[must_use]
    pub const fn expected_content(&self) -> crate::ContentReference {
        self.expected_content
    }

    #[must_use]
    pub const fn next_draft_id(&self) -> SyndicDraftId {
        self.next_draft_id
    }

    #[must_use]
    pub const fn asset_reference_set(&self) -> Option<SealedAssetReferenceSetProof> {
        self.asset_reference_set
    }

    #[must_use]
    pub const fn admitted_at(&self) -> SyndicTimestamp {
        self.admitted_at
    }

    /// Materializes the immutable receipt persisted by a successful admission.
    ///
    /// Returns an error when the caller supplied the same source and replacement draft identity.
    pub fn admission_proof(&self) -> Result<crate::AcceptedInputAdmissionProof, SyndicRecordError> {
        crate::AcceptedInputAdmissionProof::new(
            self.expected_thread_revision,
            self.draft_id,
            self.expected_draft_revision,
            self.expected_gate_revision,
            self.next_draft_id,
        )
    }
}

impl SyndicStorage {
    /// Atomically consumes one idle current draft into its submitted turn.
    #[must_use]
    pub fn submit_idle_draft(
        &self,
        expected_domain_revision: DomainRevision,
        submission: IdleSubmission,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            IdleSubmissionMutation { submission },
        )
    }

    /// Atomically consumes one non-idle current draft into accepted-input order.
    #[must_use]
    pub fn admit_accepted_input(
        &self,
        expected_domain_revision: DomainRevision,
        admission: AcceptedInputAdmission,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            AcceptedInputMutation { admission },
        )
    }
}

struct IdleSubmissionMutation {
    submission: IdleSubmission,
}

struct AcceptedInputMutation {
    admission: AcceptedInputAdmission,
}

impl DomainMutation<SyndicDomain> for IdleSubmissionMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftsCodec>(2)?;
        reservation.reserve_records::<ThreadsCodec>(1)?;
        reservation.reserve_records::<DraftByThreadCodec>(1)?;
        reservation.reserve_records::<TurnsCodec>(1)?;
        reservation.reserve_records::<TurnStatesCodec>(1)?;
        reservation.reserve_records::<TurnChildrenCodec>(1)?;
        reservation.reserve_records::<CanonicalItemsCodec>(1)?;
        reservation.reserve_records::<TurnItemsCodec>(1)?;
        reservation.reserve_records::<ImageLabelOriginSpansCodec>(1)?;
        reservation.reserve_records::<TranscriptHeadsCodec>(1)?;
        reservation.reserve_records::<TranscriptBuildsCodec>(1)?;
        reservation.reserve_records::<HistorySummariesCodec>(1)?;
        reservation.reserve_records::<InputGatesCodec>(1)?;
        reservation.reserve_records::<ActivityQueryHeadsCodec>(1)?;
        reservation.reserve_records::<ActivityQuerySourcesCodec>(1)?;
        reservation.reserve_records::<BindingsCodec>(1)?;
        reservation.reserve_records::<BindingHeadsCodec>(1)?;
        reservation.reserve_records::<ContextEnvelopesCodec>(2)?;
        reservation.reserve_records::<ThreadParentCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}

impl DomainMutation<SyndicDomain> for AcceptedInputMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftsCodec>(2)?;
        reservation.reserve_records::<ThreadsCodec>(1)?;
        reservation.reserve_records::<DraftByThreadCodec>(1)?;
        reservation.reserve_records::<AcceptedInputsCodec>(1)?;
        reservation.reserve_records::<AcceptedOrderCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteGenerationHeadsCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteGenerationsCodec>(1)?;
        reservation.reserve_records::<AcceptedRouteLeavesCodec>(1)?;
        reservation.reserve_records::<AcceptedReadySourcesCodec>(1)?;
        reservation.reserve_records::<AcceptedNextSourcesCodec>(1)?;
        reservation.reserve_records::<ImageLabelOriginSpansCodec>(1)?;
        reservation.reserve_records::<HistorySummariesCodec>(1)?;
        reservation.reserve_records::<InputGatesCodec>(1)?;
        reservation.reserve_records::<ThreadParentCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}

struct IdleSubmissionRecords {
    old_draft_id: SyndicDraftId,
    thread: ThreadRecord,
    draft: DraftRecord,
    draft_index: DraftByThreadRecord,
    turn: TurnRecord,
    turn_state: TurnStateRecord,
    child_index: Option<TurnChildIndexRecord>,
    item: CanonicalItemRecord,
    item_index: TurnItemIndexRecord,
    origin_span: Option<ImageLabelOriginSpanRecord>,
    transcript_head: TranscriptViewHeadRecord,
    transcript_build: Option<crate::TranscriptBuildRecord>,
    summary: HistorySummaryRecord,
    gate: InputGateRecord,
    activity_head: crate::ActivityQueryHeadRecord,
    activity_source: crate::ActivityQuerySourceRecord,
    binding: BindingRecord,
    binding_head: BindingHeadRecord,
    context_move: Option<ContextMove>,
    thread_parent_index: Option<ThreadParentIndexRecord>,
}

struct ContextMove {
    old_owner: DiscussionContextOwnerId,
    new_record: crate::ContextEnvelopeRecord,
}

struct AcceptedInputRecords {
    old_draft_id: SyndicDraftId,
    thread: ThreadRecord,
    draft: DraftRecord,
    draft_index: DraftByThreadRecord,
    input: crate::AcceptedInputRecord,
    order_index: crate::AcceptedOrderIndexRecord,
    route_head: Option<AcceptedRouteGenerationHeadRecord>,
    route_generation: AcceptedRouteGenerationRecord,
    route_leaf: AcceptedRouteLeafRecord,
    ready_source: Option<AcceptedReadySourceRecord>,
    next_source: Option<AcceptedNextSourceRecord>,
    origin_span: Option<ImageLabelOriginSpanRecord>,
    summary: HistorySummaryRecord,
    gate: InputGateRecord,
    thread_parent_index: Option<ThreadParentIndexRecord>,
}

struct AdmissionBase {
    thread: ThreadRecord,
    draft: DraftRecord,
    gate: InputGateRecord,
    summary: HistorySummaryRecord,
    empty_content: crate::ContentReference,
}

mod idle;
mod queued;
mod shared;

use shared::*;
pub(super) use shared::{
    canonical_empty_content, require_sealed_composer, thread_parent_index, turn_shape,
    validate_asset_reference_set, validate_replacement_intent,
};
