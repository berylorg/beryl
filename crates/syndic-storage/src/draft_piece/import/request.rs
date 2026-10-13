use beryl_home_store::{DomainCallbackError, DomainCallbackSource, ReadError};
use beryl_model::{
    InputGateRevision, ProjectionRevision, SealedAssetReferenceSetProof, SyndicItemId,
};

use super::super::*;
use crate::{ReplacementEditIntent, SyndicMutationError, SyndicReadError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftReplacementImportRequestV1 {
    pub(crate) selector: DraftEditorCurrentSelectorV1,
    pub(crate) candidate: DraftEditorCandidateActivationBindingV1,
    pub(crate) operation: DraftPieceOperationIdV1,
    pub(crate) intent: ReplacementEditIntent,
    pub(crate) item: SyndicItemId,
    pub(crate) revision: ProjectionRevision,
    pub(crate) content: crate::ContentReference,
    pub(crate) gate_revision: InputGateRevision,
    pub(crate) assets: Option<SealedAssetReferenceSetProof>,
    pub(crate) predecessor_caret: DraftCompositePositionV1,
    pub(crate) predecessor_selection: DraftCompositePositionV1,
    pub(crate) caret: DraftCompositePositionV1,
    pub(crate) selection: DraftCompositePositionV1,
}

impl DraftReplacementImportRequestV1 {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        selector: DraftEditorCurrentSelectorV1,
        candidate: DraftEditorCandidateActivationBindingV1,
        operation: DraftPieceOperationIdV1,
        intent: ReplacementEditIntent,
        item: SyndicItemId,
        revision: ProjectionRevision,
        content: crate::ContentReference,
        gate_revision: InputGateRevision,
        assets: Option<SealedAssetReferenceSetProof>,
        predecessor_caret: DraftCompositePositionV1,
        predecessor_selection: DraftCompositePositionV1,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
    ) -> Self {
        Self {
            selector,
            candidate,
            operation,
            intent,
            item,
            revision,
            content,
            gate_revision,
            assets,
            predecessor_caret,
            predecessor_selection,
            caret,
            selection,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DraftReplacementErrorV1 {
    #[error(transparent)]
    Read(#[from] SyndicReadError),
    #[error(transparent)]
    Mutation(#[from] SyndicMutationError),
    #[error(transparent)]
    Build(#[from] DraftPiecePrepareErrorV1),
    #[error(transparent)]
    HomeRead(#[from] ReadError),
    #[error(transparent)]
    Reconciliation(#[from] beryl_home_store::ReconciliationFailure),
    #[error("replacement draft source or exact command closure changed")]
    Conflict,
    #[error("replacement draft requires an empty idle quiescent ordinary editor")]
    Ineligible,
    #[error("replacement draft history baseline does not fit the configured history budget")]
    CapacityUnavailable,
    #[error("replacement draft outcome is unavailable")]
    Unavailable,
    #[error("replacement draft proof is missing, obsolete or already consumed")]
    Proof,
}

impl DomainCallbackError for DraftReplacementErrorV1 {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::HomeRead(error) => Ok(DomainCallbackSource::Read(error)),
            Self::Read(SyndicReadError::Read(error)) => Ok(DomainCallbackSource::Read(error)),
            other => Err(other),
        }
    }
}

impl From<beryl_home_store::MutationBuildError> for DraftReplacementErrorV1 {
    fn from(value: beryl_home_store::MutationBuildError) -> Self {
        Self::Mutation(value.into())
    }
}

#[derive(Clone)]
pub struct PreparedDraftReplacementImportV1 {
    pub(crate) scope: ImportCommandScope,
    pub(crate) home_generation: beryl_home_store::HomeGeneration,
    pub(crate) request: DraftReplacementImportRequestV1,
    pub(crate) source_session: Box<DraftEditorCandidateSessionV1>,
    pub(crate) basis: DraftSealedImportBasisV1,
    pub(crate) header: DraftPieceEditHeaderV1,
    pub(crate) canonical_header: Vec<u8>,
}

#[derive(Clone)]
pub(crate) struct ImportCommandScope {
    pub(crate) handle: beryl_home_store::DomainHandle<crate::domain::SyndicDomain>,
    pub(crate) identity: beryl_home_store::HomeGenerationIdentity,
    pub(crate) home_id: beryl_model::BerylHomeId,
}

impl ImportCommandScope {
    pub(crate) fn capture(
        storage: &crate::SyndicStorage,
        store: &beryl_home_store::HomeStore,
    ) -> Result<Self, DraftReplacementErrorV1> {
        store.domain_revision(&storage.handle)?;
        Ok(Self {
            handle: storage.handle.clone(),
            identity: store.generation_identity()?,
            home_id: store.home_id(),
        })
    }
    pub(crate) fn require_current(
        &self,
        store: &beryl_home_store::HomeStore,
    ) -> Result<(), DraftReplacementErrorV1> {
        if store.generation_identity()? != self.identity {
            return Err(DraftReplacementErrorV1::Proof);
        }
        store.domain_revision(&self.handle)?;
        Ok(())
    }
}

impl PreparedDraftReplacementImportV1 {
    pub const fn basis(&self) -> DraftSealedImportBasisV1 {
        self.basis
    }
    pub fn canonical_header(&self) -> &[u8] {
        &self.canonical_header
    }
    pub const fn operation_id(&self) -> DraftPieceOperationIdV1 {
        self.request.operation
    }
    pub const fn session_id(&self) -> DraftEditorCandidateSessionIdV1 {
        self.request.candidate.session_id()
    }
}
