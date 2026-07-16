use std::collections::{BTreeMap, BTreeSet};

use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, MutationContribution};
use beryl_model::{
    AcceptedInputRevision, DiscussionContextOwnerId, DomainRevision, DraftRevision,
    InputGateRevision, ProjectionRevision, SyndicDraftId, SyndicItemId, SyndicThreadId,
    SyndicTurnId, ThreadRevision,
};

use crate::{
    AcceptedInputDisposition, AcceptedInputLifecycle, AcceptedInputOrdinal, BindingHeadRecord,
    BindingLifecycle, BindingRecord, BindingState, CanonicalItemRecord, ContentEncoding,
    ConversationParent, DraftByThreadRecord, DraftRecord, HistorySummaryRecord, InputGateRecord,
    InputGateState, InputMarkerOrdinal, InputMarkerOwner, InputMarkerResolutionRecord,
    PreparedContent, ProjectionLifecycle, ResolvedImageMarker, SelectedPathProof,
    SyndicMutationError, SyndicRecordError, SyndicStorage, SyndicTimestamp,
    ThreadParentIndexRecord, ThreadRecord, TranscriptViewHeadRecord, TurnChildIndexRecord,
    TurnDepth, TurnItemIndexRecord, TurnItemOrdinal, TurnKind, TurnLifecycle, TurnRecord,
    TurnStateRecord, TurnStateRevision, child_turn_chain_digest, codec::*,
    content::input_marker_digest, domain::SyndicDomain, root_turn_chain_digest,
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

/// One bounded exact marker set supplied to an atomic input admission.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AdmissionMarkers {
    markers: Vec<ResolvedImageMarker>,
}

impl AdmissionMarkers {
    pub fn new(markers: Vec<ResolvedImageMarker>) -> Result<Self, SyndicRecordError> {
        if markers.len() > crate::record::MAX_COMPOSER_IMAGE_MARKERS {
            return Err(SyndicRecordError::TooManyImageMarkers {
                kind: "admission marker resolutions",
                maximum: crate::record::MAX_COMPOSER_IMAGE_MARKERS,
                actual: markers.len(),
            });
        }
        let mut identities = BTreeSet::new();
        let mut labels = BTreeMap::new();
        for marker in &markers {
            if !identities.insert(marker.marker_id()) {
                return Err(SyndicRecordError::DuplicateImageMarker {
                    kind: "admission marker resolutions",
                    marker_id: marker.marker_id(),
                });
            }
            if labels
                .insert(marker.label(), marker.asset_id())
                .is_some_and(|asset| asset != marker.asset_id())
            {
                return Err(SyndicRecordError::LabelAssetMismatch {
                    label: marker.label(),
                });
            }
        }
        Ok(Self { markers })
    }

    #[must_use]
    pub fn markers(&self) -> &[ResolvedImageMarker] {
        &self.markers
    }

    pub(super) fn validate_content(
        &self,
        content: crate::ContentReference,
    ) -> Result<(), SyndicMutationError> {
        let count =
            u64::try_from(self.markers.len()).map_err(|_| SyndicRecordError::LengthOverflow {
                kind: "admission marker resolutions",
            })?;
        let digest = input_marker_digest(
            self.markers
                .iter()
                .map(|marker| (marker.marker_id(), marker.label())),
        );
        if count != content.summary().image_marker_count()
            || digest != content.summary().marker_digest()
        {
            return Err(SyndicMutationError::MarkerResolutionConflict);
        }
        Ok(())
    }
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
    markers: AdmissionMarkers,
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
        markers: AdmissionMarkers,
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
            markers,
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
    pub const fn markers(&self) -> &AdmissionMarkers {
        &self.markers
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
    markers: AdmissionMarkers,
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
        markers: AdmissionMarkers,
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
            markers,
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
    pub const fn markers(&self) -> &AdmissionMarkers {
        &self.markers
    }

    #[must_use]
    pub const fn admitted_at(&self) -> SyndicTimestamp {
        self.admitted_at
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
    marker_records: Vec<InputMarkerResolutionRecord>,
    transcript_head: TranscriptViewHeadRecord,
    transcript_build: Option<crate::TranscriptBuildRecord>,
    summary: HistorySummaryRecord,
    gate: InputGateRecord,
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
    steering_index: Option<crate::AcceptedSteeringIndexRecord>,
    next_index: Option<crate::AcceptedNextTurnIndexRecord>,
    marker_records: Vec<InputMarkerResolutionRecord>,
    summary: HistorySummaryRecord,
    gate: InputGateRecord,
    thread_parent_index: Option<ThreadParentIndexRecord>,
}

struct AdmissionBase {
    thread: ThreadRecord,
    draft: DraftRecord,
    gate: InputGateRecord,
    empty_content: crate::ContentReference,
}

mod idle;
mod queued;
mod shared;

use shared::*;
pub(super) use shared::{
    canonical_empty_content, require_sealed_composer, validate_replacement_intent,
};
