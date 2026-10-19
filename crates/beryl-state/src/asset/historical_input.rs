use beryl_home_store::{
    DomainReader, FixedDigestHomeProofProtocol, HomeStore, ProofCorrelationBytes,
    ProofWitnessContribution, ReadError, SidecarError, VerifiedSidecar,
};
use beryl_model::HistoricalInputMarkerPageV1;
use sha2::{Digest, Sha256};

use super::{AssetDomain, AssetOwner, AssetState, asset_sidecar_address, marker_source};

pub type HistoricalInputMarkerProofProtocolV1 =
    FixedDigestHomeProofProtocol<0x5348494d50525631, 0x494d504147455631>;

#[derive(Debug)]
pub enum AssetHistoricalInputError {
    Read(ReadError),
    Sidecar(SidecarError),
    Rejected,
}
impl std::fmt::Display for AssetHistoricalInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(e) => std::fmt::Display::fmt(e, f),
            Self::Sidecar(e) => std::fmt::Display::fmt(e, f),
            Self::Rejected => {
                f.write_str("historical input image authority is unavailable or changed")
            }
        }
    }
}
impl std::error::Error for AssetHistoricalInputError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(e) => Some(e),
            Self::Sidecar(e) => Some(e),
            Self::Rejected => None,
        }
    }
}
impl From<ReadError> for AssetHistoricalInputError {
    fn from(e: ReadError) -> Self {
        Self::Read(e)
    }
}
impl From<SidecarError> for AssetHistoricalInputError {
    fn from(e: SidecarError) -> Self {
        Self::Sidecar(e)
    }
}

pub(crate) struct HistoricalInputWitness {
    page: HistoricalInputMarkerPageV1,
    available: std::sync::Arc<VerifiedSidecar>,
}

impl AssetState {
    pub fn historical_input_marker_witness(
        &self,
        store: &HomeStore,
        page: HistoricalInputMarkerPageV1,
    ) -> Result<
        ProofWitnessContribution<HistoricalInputMarkerProofProtocolV1>,
        AssetHistoricalInputError,
    > {
        let available =
            std::sync::Arc::new(self.verify_sidecar(store, page.association().asset_id())?);
        self.historical_input_marker_witness_with_verified_sidecar(store, page, available)
    }
    pub fn historical_input_marker_witness_with_verified_sidecar(
        &self,
        store: &HomeStore,
        page: HistoricalInputMarkerPageV1,
        available: std::sync::Arc<VerifiedSidecar>,
    ) -> Result<
        ProofWitnessContribution<HistoricalInputMarkerProofProtocolV1>,
        AssetHistoricalInputError,
    > {
        if !store.verified_sidecar_matches_current_scope(&available)?
            || available.address() != &asset_sidecar_address(page.association().asset_id())
        {
            return Err(AssetHistoricalInputError::Rejected);
        }
        let revision = self.revision(store)?;
        Ok(self
            .handle
            .proof_witness::<HistoricalInputMarkerProofProtocolV1>(
                revision,
                super::draft_marker_label_readiness::WitnessInput::HistoricalInput(
                    HistoricalInputWitness { page, available },
                ),
            ))
    }
}

pub(crate) fn prove_historical_input(
    input: &HistoricalInputWitness,
    reader: &DomainReader<'_, AssetDomain>,
) -> Result<ProofCorrelationBytes, AssetHistoricalInputError> {
    let page = &input.page;
    marker_source::prove_marker_source(
        reader,
        AssetOwner::SubmittedTurnItem(page.item_id()),
        page.proof(),
        page.association().into(),
        &input.available,
    )
    .map_err(|error| match error {
        marker_source::MarkerSourceError::Read(error) => AssetHistoricalInputError::Read(error),
        marker_source::MarkerSourceError::Rejected => AssetHistoricalInputError::Rejected,
    })?;
    Ok(ProofCorrelationBytes::new(historical_input_correlation(
        page,
    )))
}

fn historical_input_correlation(page: &HistoricalInputMarkerPageV1) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"syndic/historical-input-marker-window/v1");
    hash.update(page.draft_id().as_bytes());
    hash.update(page.session_id());
    hash.update(page.operation_id());
    hash.update(page.receipt_ordinal().to_be_bytes());
    hash.update(page.receipt_digest());
    hash.update((page.canonical_header().len() as u64).to_be_bytes());
    hash.update(page.canonical_header());
    hash.update(page.item_id().as_bytes());
    hash.update(page.proof().set_id().as_bytes());
    hash.update(page.proof().sequential().marker_digest());
    hash.update(page.proof().sequential().marker_count().to_be_bytes());
    hash.update(
        page.proof()
            .sequential()
            .maximum_image_label()
            .map_or(0, |v| v.get())
            .to_be_bytes(),
    );
    hash.update(page.proof().ordered_assets().marker_asset_digest());
    hash.update(page.proof().ordered_assets().marker_count().to_be_bytes());
    hash.update(page.proof().entry_frontier().to_be_bytes());
    hash.update(page.proof().asset_chain_digest().as_bytes());
    hash.update(page.encoded_start().to_be_bytes());
    hash.update(page.source_piece().to_be_bytes());
    let association = page.association();
    hash.update(association.ordinal().to_be_bytes());
    hash.update(association.marker_id().as_bytes());
    hash.update(association.label().get().to_be_bytes());
    hash.update([association.asset_id().version() as u8]);
    hash.update(association.asset_id().digest());
    hash.update(association.asset_id().length().get().to_be_bytes());
    hash.finalize().into()
}
