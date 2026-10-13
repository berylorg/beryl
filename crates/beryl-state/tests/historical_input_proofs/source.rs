use std::convert::Infallible;

use beryl_home_store::{
    DomainReader, DomainSchemaVersion, FixedDigestHomeProofProtocol, KeyspaceSchemaVersion,
    ProofCorrelationBytes, ProofDomain, ProofProtocolIdentity, RecordCodec, RecordFamily,
    RecordVersion, StorageDomain,
};
use beryl_model::HistoricalInputMarkerPageV1;
use beryl_state::HistoricalInputMarkerProofProtocolV1;
use sha2::{Digest, Sha256};

pub struct SourceDomain;
struct SourceCodec;
pub enum NoWitness {}

pub struct SourceInput {
    pub correlation: [u8; 32],
    pub wrong_protocol: bool,
}

impl RecordCodec<SourceDomain> for SourceCodec {
    type Key = u8;
    type Value = ();
    type Error = Infallible;
    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = 0;

    fn encode_key(key: &u8) -> Result<Vec<u8>, Infallible> {
        Ok(vec![*key])
    }
    fn decode_key(encoded: &[u8]) -> Result<u8, Infallible> {
        Ok(encoded.first().copied().unwrap_or_default())
    }
    fn encode_value(_: &()) -> Result<Vec<u8>, Infallible> {
        Ok(Vec::new())
    }
    fn decode_value(_: &[u8]) -> Result<(), Infallible> {
        Ok(())
    }
}

impl StorageDomain for SourceDomain {
    const NAME: &'static str = "historical-input-witness-probe";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<SourceCodec>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment(
        _: &beryl_home_store::DomainRegistrationReader<'_, Self>,
    ) -> Result<(), Infallible> {
        Ok(())
    }
    fn validate(_: &DomainReader<'_, Self>) -> Result<(), Infallible> {
        Ok(())
    }
}

// This trusted probe qualifies only the Asset witness, not Syndic's source or consumer.
impl ProofDomain for SourceDomain {
    type SourceInput = SourceInput;
    type WitnessInput = NoWitness;
    type Error = Infallible;

    fn source_protocol(input: &SourceInput) -> ProofProtocolIdentity {
        if input.wrong_protocol {
            ProofProtocolIdentity::of::<FixedDigestHomeProofProtocol<1, 1>>()
        } else {
            ProofProtocolIdentity::of::<HistoricalInputMarkerProofProtocolV1>()
        }
    }
    fn expected_source_correlation(input: &SourceInput) -> ProofCorrelationBytes {
        ProofCorrelationBytes::new(input.correlation)
    }
    fn witness_protocol(input: &NoWitness) -> ProofProtocolIdentity {
        match *input {}
    }
    fn prove_source(
        input: &SourceInput,
        _: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Infallible> {
        Ok(ProofCorrelationBytes::new(input.correlation))
    }
    fn prove_witness(
        input: &NoWitness,
        _: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Infallible> {
        match *input {}
    }
}

pub fn correlation(page: &HistoricalInputMarkerPageV1) -> [u8; 32] {
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
    let proof = page.proof();
    hash.update(proof.set_id().as_bytes());
    hash.update(proof.sequential().marker_digest());
    hash.update(proof.sequential().marker_count().to_be_bytes());
    hash.update(
        proof
            .sequential()
            .maximum_image_label()
            .map_or(0, |v| v.get())
            .to_be_bytes(),
    );
    hash.update(proof.ordered_assets().marker_asset_digest());
    hash.update(proof.ordered_assets().marker_count().to_be_bytes());
    hash.update(proof.entry_frontier().to_be_bytes());
    hash.update(proof.asset_chain_digest().as_bytes());
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
