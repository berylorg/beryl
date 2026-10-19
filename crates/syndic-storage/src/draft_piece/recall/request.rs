use super::super::*;
use super::*;
use crate::{SyndicMutationError, SyndicReadError, SyndicStorage};
use beryl_home_store::{DomainCallbackError, DomainCallbackSource, HomeStore, ReadError};

#[derive(Debug, thiserror::Error)]
pub enum DraftComposerRecallErrorV1 {
    #[error(transparent)]
    Read(#[from] SyndicReadError),
    #[error(transparent)]
    Range(#[from] DraftPieceRangeSourceErrorV1),
    #[error(transparent)]
    Mutation(#[from] SyndicMutationError),
    #[error(transparent)]
    Build(#[from] DraftPiecePrepareErrorV1),
    #[error(transparent)]
    HomeRead(#[from] ReadError),
    #[error(transparent)]
    Reconciliation(#[from] beryl_home_store::ReconciliationFailure),
    #[error("composer recall exact authority changed")]
    Conflict,
    #[error("composer recall is unavailable for this editor")]
    Ineligible,
    #[error("composer recall history capacity unavailable")]
    CapacityUnavailable,
    #[error("composer recall outcome is unavailable")]
    Unavailable,
    #[error("composer recall proof is missing, obsolete or consumed")]
    Proof,
}
impl DomainCallbackError for DraftComposerRecallErrorV1 {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::HomeRead(e) => Ok(DomainCallbackSource::Read(e)),
            Self::Read(SyndicReadError::Read(e)) => Ok(DomainCallbackSource::Read(e)),
            other => Err(other),
        }
    }
}
impl From<beryl_home_store::MutationBuildError> for DraftComposerRecallErrorV1 {
    fn from(e: beryl_home_store::MutationBuildError) -> Self {
        Self::Mutation(e.into())
    }
}

#[derive(Clone)]
pub(crate) struct RecallScope {
    pub(crate) handle: beryl_home_store::DomainHandle<crate::domain::SyndicDomain>,
    pub(crate) generation: beryl_home_store::HomeGenerationIdentity,
    pub(crate) home: beryl_model::BerylHomeId,
}
impl RecallScope {
    pub(crate) fn capture(
        storage: &SyndicStorage,
        store: &HomeStore,
    ) -> Result<Self, DraftComposerRecallErrorV1> {
        store.domain_revision(&storage.handle)?;
        Ok(Self {
            handle: storage.handle.clone(),
            generation: store.generation_identity()?,
            home: store.home_id(),
        })
    }
    pub(crate) fn require(&self, store: &HomeStore) -> Result<(), DraftComposerRecallErrorV1> {
        if store.home_id() != self.home || store.generation_identity()? != self.generation {
            return Err(DraftComposerRecallErrorV1::Proof);
        }
        store.domain_revision(&self.handle)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct DraftComposerRecallRequestV1 {
    pub(crate) activation: DraftEditorCandidateActivationBindingV1,
    pub(crate) operation: DraftPieceOperationIdV1,
    pub(crate) source: ComposerAcceptedInputReferenceV1,
    pub(crate) before_caret: DraftCompositePositionV1,
    pub(crate) before_selection: DraftCompositePositionV1,
}
impl DraftComposerRecallRequestV1 {
    pub fn new(
        activation: DraftEditorCandidateActivationBindingV1,
        operation: DraftPieceOperationIdV1,
        source: ComposerAcceptedInputReferenceV1,
        before_caret: DraftCompositePositionV1,
        before_selection: DraftCompositePositionV1,
    ) -> Self {
        Self {
            activation,
            operation,
            source,
            before_caret,
            before_selection,
        }
    }
}

#[derive(Clone)]
pub struct PreparedDraftComposerRecallV1 {
    pub(crate) home_generation: beryl_home_store::HomeGeneration,
    pub(crate) scope: RecallScope,
    pub(crate) before: Box<DraftEditorCandidateSessionV1>,
    pub(crate) basis: DraftComposerRecallBasisV1,
    pub(crate) source: ComposerAcceptedInputReferenceV1,
    pub(crate) header: DraftPieceEditHeaderV1,
    pub(crate) canonical_header: Vec<u8>,
}
impl PreparedDraftComposerRecallV1 {
    pub fn key(&self) -> DraftPieceSettlementKeyV1 {
        DraftPieceSettlementKeyV1::new(
            self.header.draft_id(),
            self.header.session_id(),
            self.header.operation_id(),
        )
    }
    pub fn source(&self) -> &ComposerAcceptedInputReferenceV1 {
        &self.source
    }
    pub fn canonical_header(&self) -> &[u8] {
        &self.canonical_header
    }
    pub fn operation_id(&self) -> DraftPieceOperationIdV1 {
        self.header.operation_id()
    }
}
