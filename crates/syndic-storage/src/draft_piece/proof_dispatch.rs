use super::{
    DraftReplacementErrorV1,
    admission::{DraftMarkerReadinessSourceErrorV1, PageProtocol, readiness_source},
    import::image_proof,
};
use crate::domain::SyndicDomain;
use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainReader, ProofCorrelationBytes, ProofDomain,
    ProofProtocolIdentity,
};

pub enum SourceInput {
    LabelReadiness(readiness_source::SourceInput),
    HistoricalInput(image_proof::HistoricalImportSource),
}
pub enum NoWitness {}

#[derive(Debug, thiserror::Error)]
pub enum SyndicProofError {
    #[error(transparent)]
    Readiness(#[from] DraftMarkerReadinessSourceErrorV1),
    #[error(transparent)]
    HistoricalInput(#[from] DraftReplacementErrorV1),
}
impl DomainCallbackError for SyndicProofError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Readiness(e) => e.into_callback_source().map_err(Self::Readiness),
            Self::HistoricalInput(e) => e.into_callback_source().map_err(Self::HistoricalInput),
        }
    }
}
impl ProofDomain for SyndicDomain {
    type SourceInput = SourceInput;
    type WitnessInput = NoWitness;
    type Error = SyndicProofError;
    fn source_protocol(input: &SourceInput) -> ProofProtocolIdentity {
        match input {
            SourceInput::LabelReadiness(_) => ProofProtocolIdentity::of::<PageProtocol>(),
            SourceInput::HistoricalInput(_) => {
                ProofProtocolIdentity::of::<image_proof::HistoricalInputMarkerProofProtocolV1>()
            }
        }
    }
    fn expected_source_correlation(input: &SourceInput) -> ProofCorrelationBytes {
        match input {
            SourceInput::LabelReadiness(p) => ProofCorrelationBytes::new(p.page.expected),
            SourceInput::HistoricalInput(p) => image_proof::expected(p),
        }
    }
    fn witness_protocol(input: &NoWitness) -> ProofProtocolIdentity {
        match *input {}
    }
    fn prove_source(
        input: &SourceInput,
        reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        match input {
            SourceInput::LabelReadiness(p) => Ok(readiness_source::prove_readiness(p, reader)?),
            SourceInput::HistoricalInput(p) => Ok(image_proof::prove(p, reader)?),
        }
    }
    fn prove_witness(
        input: &NoWitness,
        _: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        match *input {}
    }
}
