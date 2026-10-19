use beryl_home_store::{
    DomainReader, FixedDigestHomeProofProtocol, HomeStore, ProofCorrelationBytes,
    ProofWitnessContribution, ReadError, SidecarError, VerifiedSidecar,
};
use beryl_model::{
    ComposerRecallMarkerPageV1, ComposerRecallSourceOwnerV1, composer_recall_marker_correlation,
};

use super::{AssetDomain, AssetOwner, AssetState, asset_sidecar_address, marker_source};

pub type ComposerRecallMarkerProofProtocolV1 =
    FixedDigestHomeProofProtocol<0x5343524d50525631, 0x524543414c4c5631>;

#[derive(Debug)]
pub enum AssetComposerRecallError {
    Read(ReadError),
    Sidecar(SidecarError),
    Rejected,
}

impl std::fmt::Display for AssetComposerRecallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => std::fmt::Display::fmt(error, formatter),
            Self::Sidecar(error) => std::fmt::Display::fmt(error, formatter),
            Self::Rejected => {
                formatter.write_str("composer recall image authority is unavailable or changed")
            }
        }
    }
}

impl std::error::Error for AssetComposerRecallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            Self::Sidecar(error) => Some(error),
            Self::Rejected => None,
        }
    }
}

impl From<ReadError> for AssetComposerRecallError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

impl From<SidecarError> for AssetComposerRecallError {
    fn from(error: SidecarError) -> Self {
        Self::Sidecar(error)
    }
}

pub(crate) struct ComposerRecallWitness {
    page: ComposerRecallMarkerPageV1,
    available: std::sync::Arc<VerifiedSidecar>,
}

impl AssetState {
    pub fn composer_recall_marker_witness(
        &self,
        store: &HomeStore,
        page: ComposerRecallMarkerPageV1,
    ) -> Result<
        ProofWitnessContribution<ComposerRecallMarkerProofProtocolV1>,
        AssetComposerRecallError,
    > {
        let available =
            std::sync::Arc::new(self.verify_sidecar(store, page.association().asset_id())?);
        self.composer_recall_marker_witness_with_verified_sidecar(store, page, available)
    }

    pub fn composer_recall_marker_witness_with_verified_sidecar(
        &self,
        store: &HomeStore,
        page: ComposerRecallMarkerPageV1,
        available: std::sync::Arc<VerifiedSidecar>,
    ) -> Result<
        ProofWitnessContribution<ComposerRecallMarkerProofProtocolV1>,
        AssetComposerRecallError,
    > {
        if !store.verified_sidecar_matches_current_scope(&available)?
            || available.address() != &asset_sidecar_address(page.association().asset_id())
        {
            return Err(AssetComposerRecallError::Rejected);
        }
        let revision = self.revision(store)?;
        Ok(self
            .handle
            .proof_witness::<ComposerRecallMarkerProofProtocolV1>(
                revision,
                super::draft_marker_label_readiness::WitnessInput::ComposerRecall(
                    ComposerRecallWitness { page, available },
                ),
            ))
    }
}

pub(crate) fn prove_composer_recall(
    input: &ComposerRecallWitness,
    reader: &DomainReader<'_, AssetDomain>,
) -> Result<ProofCorrelationBytes, AssetComposerRecallError> {
    let owner = match input.page.source_owner() {
        ComposerRecallSourceOwnerV1::AcceptedInput(id) => AssetOwner::AcceptedInput(id),
        ComposerRecallSourceOwnerV1::SubmittedTurnItem { item_id, .. } => {
            AssetOwner::SubmittedTurnItem(item_id)
        }
    };
    marker_source::prove_marker_source(
        reader,
        owner,
        input.page.proof(),
        input.page.association().into(),
        &input.available,
    )
    .map_err(|error| match error {
        marker_source::MarkerSourceError::Read(error) => AssetComposerRecallError::Read(error),
        marker_source::MarkerSourceError::Rejected => AssetComposerRecallError::Rejected,
    })?;
    Ok(ProofCorrelationBytes::new(
        composer_recall_marker_correlation(&input.page),
    ))
}
