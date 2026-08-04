use super::*;

/// Immutable natural-identity and revision receipt for one accepted-input admission.
///
/// The receipt preserves the source authority checked by the admission and the distinct
/// replacement draft created by the same atomic commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedInputAdmissionProof {
    expected_thread_revision: ThreadRevision,
    source_draft_id: SyndicDraftId,
    expected_draft_revision: DraftRevision,
    expected_gate_revision: InputGateRevision,
    replacement_draft_id: SyndicDraftId,
}

impl AcceptedInputAdmissionProof {
    /// Constructs a complete immutable admission receipt.
    ///
    /// Returns an error when the source and replacement draft identities collide.
    pub fn new(
        expected_thread_revision: ThreadRevision,
        source_draft_id: SyndicDraftId,
        expected_draft_revision: DraftRevision,
        expected_gate_revision: InputGateRevision,
        replacement_draft_id: SyndicDraftId,
    ) -> Result<Self, crate::SyndicRecordError> {
        if source_draft_id == replacement_draft_id {
            return Err(crate::SyndicRecordError::AcceptedInputAdmissionDraftCollision);
        }
        Ok(Self {
            expected_thread_revision,
            source_draft_id,
            expected_draft_revision,
            expected_gate_revision,
            replacement_draft_id,
        })
    }

    #[must_use]
    pub const fn expected_thread_revision(self) -> ThreadRevision {
        self.expected_thread_revision
    }

    #[must_use]
    pub const fn source_draft_id(self) -> SyndicDraftId {
        self.source_draft_id
    }

    #[must_use]
    pub const fn expected_draft_revision(self) -> DraftRevision {
        self.expected_draft_revision
    }

    #[must_use]
    pub const fn expected_gate_revision(self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn replacement_draft_id(self) -> SyndicDraftId {
        self.replacement_draft_id
    }
}

/// One identity-preserving input fragment accepted during an active or queued lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedInputRecord {
    id: SyndicAcceptedInputId,
    thread_id: SyndicThreadId,
    ordinal: AcceptedInputOrdinal,
    admission: AcceptedInputAdmissionProof,
    route_generation: AcceptedRouteGeneration,
    content: ContentReference,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
    admitted_at: SyndicTimestamp,
}

impl AcceptedInputRecord {
    /// Constructs an accepted input bound to its admission receipt.
    ///
    /// Returns an error when `id` is not the accepted-input identity derived from the receipt's
    /// source draft.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: SyndicAcceptedInputId,
        thread_id: SyndicThreadId,
        ordinal: AcceptedInputOrdinal,
        admission: AcceptedInputAdmissionProof,
        route_generation: AcceptedRouteGeneration,
        content: ContentReference,
        asset_reference_set: Option<SealedAssetReferenceSetProof>,
        admitted_at: SyndicTimestamp,
    ) -> Result<Self, crate::SyndicRecordError> {
        if id != admission.source_draft_id().accepted_input_id() {
            return Err(crate::SyndicRecordError::AcceptedInputIdentityMismatch);
        }
        Ok(Self {
            id,
            thread_id,
            ordinal,
            admission,
            route_generation,
            content,
            asset_reference_set,
            admitted_at,
        })
    }
    #[must_use]
    pub const fn id(&self) -> SyndicAcceptedInputId {
        self.id
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn ordinal(&self) -> AcceptedInputOrdinal {
        self.ordinal
    }
    #[must_use]
    pub const fn admission(&self) -> AcceptedInputAdmissionProof {
        self.admission
    }
    #[must_use]
    pub const fn admission_gate_revision(&self) -> InputGateRevision {
        self.admission.expected_gate_revision()
    }
    #[must_use]
    pub const fn route_generation(&self) -> AcceptedRouteGeneration {
        self.route_generation
    }
    #[must_use]
    pub const fn content(&self) -> ContentReference {
        self.content
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
