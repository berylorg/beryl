use std::{collections::BTreeSet, error::Error, fmt};

#[cfg(feature = "test-faults")]
use std::cell::Cell;

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainReader, FixedDigestHomeProofProtocol,
    HomeStore, PointReadLimit, ProofCorrelationBytes, ProofDomain, ProofProtocolIdentity,
    ProofWitnessContribution, ReadError,
};
use beryl_model::{AssetId, ImageLabelOrdinal, SealedAssetReferenceSetProof};
use sha2::{Digest, Sha256};

use super::{
    ASSET_COMPLETION_EVIDENCE_LIMIT, AssetDomain, AssetEntryKey, AssetLabelFirstKey,
    AssetMetadataCodec, AssetReferenceCompletionEvidenceCodec, AssetReferenceEntryCodec,
    AssetReferenceLabelFirstCodec, AssetReferenceManifestCodec, AssetState, entry_point_limit,
    index_point_limit, manifest_point_limit, metadata_point_limit, read,
};

const PAGE_DOMAIN: &[u8] = b"syndic/draft-marker-label-readiness-page/v1";
const PAGE_MAX_ASSOCIATIONS: usize = 256;
const PAGE_MAX_CANONICAL_BYTES: usize = 65_536;
const ACCEPTED_ASSOCIATION_CANONICAL_BYTES: usize = 194;
const ACCEPTED_EVIDENCE_TAG: u8 = 0x01;

#[cfg(feature = "test-faults")]
thread_local! {
    static WITNESS_VALIDATION_READ_SETS: Cell<usize> = const { Cell::new(0) };
}

type DraftMarkerLabelReadinessPageProtocol =
    FixedDigestHomeProofProtocol<0x53444d5244595631, 0x5244595041474531>;

pub struct AssetDraftMarkerLabelReadinessError {
    kind: ErrorKind,
}

enum ErrorKind {
    Read(ReadError),
    Rejected,
}

impl AssetDraftMarkerLabelReadinessError {
    fn rejected() -> Self {
        Self {
            kind: ErrorKind::Rejected,
        }
    }
}

impl fmt::Debug for AssetDraftMarkerLabelReadinessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::Read(error) => formatter
                .debug_tuple("AssetDraftMarkerLabelReadinessError::Read")
                .field(error)
                .finish(),
            ErrorKind::Rejected => {
                formatter.write_str("AssetDraftMarkerLabelReadinessError::Rejected")
            }
        }
    }
}

impl fmt::Display for AssetDraftMarkerLabelReadinessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::Read(error) => {
                write!(formatter, "asset readiness witness access failed: {error}")
            }
            ErrorKind::Rejected => {
                formatter.write_str("asset readiness witness rejected the page authority")
            }
        }
    }
}

impl Error for AssetDraftMarkerLabelReadinessError {}

impl DomainCallbackError for AssetDraftMarkerLabelReadinessError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self.kind {
            ErrorKind::Read(error) => Ok(DomainCallbackSource::Read(error)),
            ErrorKind::Rejected => Err(Self::rejected()),
        }
    }
}

impl From<ReadError> for AssetDraftMarkerLabelReadinessError {
    fn from(value: ReadError) -> Self {
        Self {
            kind: ErrorKind::Read(value),
        }
    }
}

pub(crate) struct WitnessInput {
    ordinal: u64,
    eof: bool,
    associations: Vec<(SealedAssetReferenceSetProof, ImageLabelOrdinal, AssetId)>,
}

impl WitnessInput {
    fn new(
        ordinal: u64,
        eof: bool,
        associations: Vec<(SealedAssetReferenceSetProof, ImageLabelOrdinal, AssetId)>,
    ) -> Result<Self, AssetDraftMarkerLabelReadinessError> {
        let input = Self {
            ordinal,
            eof,
            associations,
        };
        input.validate_shape()?;
        Ok(input)
    }

    fn validate_shape(&self) -> Result<(), AssetDraftMarkerLabelReadinessError> {
        let canonical_bytes = self
            .associations
            .len()
            .checked_mul(ACCEPTED_ASSOCIATION_CANONICAL_BYTES)
            .ok_or_else(AssetDraftMarkerLabelReadinessError::rejected)?;
        if self.ordinal == 0
            || self.associations.len() > PAGE_MAX_ASSOCIATIONS
            || canonical_bytes > PAGE_MAX_CANONICAL_BYTES
            || (self.associations.is_empty() && !self.eof)
            || self.associations.windows(2).any(|pair| {
                let (prior_proof, prior_label, prior_asset) = pair[0];
                let (next_proof, next_label, next_asset) = pair[1];
                prior_label > next_label
                    || (prior_label == next_label
                        && (prior_asset != next_asset
                            || sealed_proof_evidence(prior_proof)
                                > sealed_proof_evidence(next_proof)))
            })
        {
            return Err(AssetDraftMarkerLabelReadinessError::rejected());
        }
        Ok(())
    }
}

pub(crate) enum SourceInput {}

impl ProofDomain for AssetDomain {
    type SourceInput = SourceInput;
    type WitnessInput = WitnessInput;
    type Error = AssetDraftMarkerLabelReadinessError;

    fn source_protocol(input: &Self::SourceInput) -> ProofProtocolIdentity {
        match *input {}
    }

    fn expected_source_correlation(input: &Self::SourceInput) -> ProofCorrelationBytes {
        match *input {}
    }

    fn witness_protocol(_input: &Self::WitnessInput) -> ProofProtocolIdentity {
        ProofProtocolIdentity::of::<DraftMarkerLabelReadinessPageProtocol>()
    }

    fn prove_source(
        input: &Self::SourceInput,
        _reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        match *input {}
    }

    fn prove_witness(
        input: &Self::WitnessInput,
        reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        let mut validated = BTreeSet::new();
        for (proof, label, asset_id) in input.associations.iter().copied() {
            if validated.insert(association_evidence(proof, label, asset_id)) {
                note_validation_read_set();
                validate_association(reader, proof, label, asset_id)?;
            }
        }
        Ok(ProofCorrelationBytes::new(page_correlation(input)))
    }
}

#[cfg(feature = "test-faults")]
pub(super) fn reset_validation_read_sets_for_test() {
    WITNESS_VALIDATION_READ_SETS.with(|count| count.set(0));
}

#[cfg(feature = "test-faults")]
pub(super) fn validation_read_sets_for_test() -> usize {
    WITNESS_VALIDATION_READ_SETS.with(Cell::get)
}

#[cfg(feature = "test-faults")]
fn note_validation_read_set() {
    WITNESS_VALIDATION_READ_SETS.with(|count| count.set(count.get() + 1));
}

#[cfg(not(feature = "test-faults"))]
fn note_validation_read_set() {}

impl AssetState {
    pub fn draft_marker_label_readiness_witness_factory(
        &self,
    ) -> impl FnOnce(
        &HomeStore,
        u64,
        bool,
        Vec<(SealedAssetReferenceSetProof, ImageLabelOrdinal, AssetId)>,
    ) -> Result<
        ProofWitnessContribution<
            FixedDigestHomeProofProtocol<0x53444d5244595631, 0x5244595041474531>,
        >,
        AssetDraftMarkerLabelReadinessError,
    > + Send
    + 'static {
        let handle = self.handle.clone();
        move |store, ordinal, eof, associations| {
            let revision = store.domain_revision(&handle)?;
            let input = WitnessInput::new(ordinal, eof, associations)?;
            Ok(handle.proof_witness::<DraftMarkerLabelReadinessPageProtocol>(revision, input))
        }
    }
}

fn validate_association(
    reader: &DomainReader<'_, AssetDomain>,
    proof: SealedAssetReferenceSetProof,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
) -> Result<(), AssetDraftMarkerLabelReadinessError> {
    let manifest = reader
        .point::<AssetReferenceManifestCodec>(&proof.set_id(), manifest_point_limit())?
        .ok_or_else(AssetDraftMarkerLabelReadinessError::rejected)?;
    let evidence = reader
        .point::<AssetReferenceCompletionEvidenceCodec>(
            &proof.set_id(),
            completion_evidence_point_limit(),
        )?
        .ok_or_else(AssetDraftMarkerLabelReadinessError::rejected)?;
    read::verify_sealed_manifest(&manifest, &evidence, proof)
        .map_err(|_| AssetDraftMarkerLabelReadinessError::rejected())?;

    let key = AssetLabelFirstKey {
        set_id: proof.set_id(),
        label,
    };
    let first = reader
        .point::<AssetReferenceLabelFirstCodec>(&key, index_point_limit())?
        .ok_or_else(AssetDraftMarkerLabelReadinessError::rejected)?;
    if first.asset_id != asset_id {
        return Err(AssetDraftMarkerLabelReadinessError::rejected());
    }
    let entry = reader
        .point::<AssetReferenceEntryCodec>(
            &AssetEntryKey {
                set_id: proof.set_id(),
                ordinal: first.first_ordinal,
            },
            entry_point_limit(),
        )?
        .ok_or_else(AssetDraftMarkerLabelReadinessError::rejected)?;
    if entry.set_id() != proof.set_id()
        || entry.ordinal() != first.first_ordinal
        || entry.label() != label
        || entry.asset_id() != asset_id
    {
        return Err(AssetDraftMarkerLabelReadinessError::rejected());
    }
    let metadata = reader
        .point::<AssetMetadataCodec>(&asset_id, metadata_point_limit())?
        .ok_or_else(AssetDraftMarkerLabelReadinessError::rejected)?;
    if metadata.asset_id() != asset_id {
        return Err(AssetDraftMarkerLabelReadinessError::rejected());
    }
    Ok(())
}

fn completion_evidence_point_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_COMPLETION_EVIDENCE_LIMIT + 4)
        .expect("completion evidence point bound is nonzero")
}

fn page_correlation(input: &WitnessInput) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(PAGE_DOMAIN);
    hasher.update(input.ordinal.to_le_bytes());
    hasher.update([u8::from(input.eof)]);
    hasher.update((input.associations.len() as u64).to_le_bytes());
    for (proof, label, asset_id) in input.associations.iter().copied() {
        hasher.update([ACCEPTED_EVIDENCE_TAG]);
        hasher.update(proof.set_id().as_bytes());
        hasher.update(proof.sequential().marker_digest());
        hasher.update(proof.sequential().marker_count().to_le_bytes());
        hasher.update(
            proof
                .sequential()
                .maximum_image_label()
                .map_or(0, ImageLabelOrdinal::get)
                .to_le_bytes(),
        );
        hasher.update(proof.ordered_assets().marker_asset_digest());
        hasher.update(proof.ordered_assets().marker_count().to_le_bytes());
        hasher.update(proof.entry_frontier().to_le_bytes());
        hasher.update(proof.asset_chain_digest().as_bytes());
        hasher.update(label.get().to_le_bytes());
        hasher.update([asset_id.version() as u8]);
        hasher.update(asset_id.digest());
        hasher.update(asset_id.length().get().to_le_bytes());
    }
    hasher.finalize().into()
}

fn association_evidence(
    proof: SealedAssetReferenceSetProof,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
) -> [u8; ACCEPTED_ASSOCIATION_CANONICAL_BYTES] {
    let mut bytes = [0_u8; ACCEPTED_ASSOCIATION_CANONICAL_BYTES];
    let mut offset = 0;
    for part in [
        [ACCEPTED_EVIDENCE_TAG].as_slice(),
        sealed_proof_evidence(proof).as_slice(),
        label.get().to_le_bytes().as_slice(),
        [asset_id.version() as u8].as_slice(),
        asset_id.digest().as_slice(),
        asset_id.length().get().to_le_bytes().as_slice(),
    ] {
        let end = offset + part.len();
        bytes[offset..end].copy_from_slice(part);
        offset = end;
    }
    debug_assert_eq!(offset, bytes.len());
    bytes
}

fn sealed_proof_evidence(proof: SealedAssetReferenceSetProof) -> [u8; 144] {
    let sequential = proof.sequential();
    let ordered_assets = proof.ordered_assets();
    let mut bytes = [0_u8; 144];
    let mut offset = 0;
    for part in [
        proof.set_id().as_bytes().as_slice(),
        sequential.marker_digest().as_slice(),
        sequential.marker_count().to_le_bytes().as_slice(),
        sequential
            .maximum_image_label()
            .map_or(0, ImageLabelOrdinal::get)
            .to_le_bytes()
            .as_slice(),
        ordered_assets.marker_asset_digest().as_slice(),
        ordered_assets.marker_count().to_le_bytes().as_slice(),
        proof.entry_frontier().to_le_bytes().as_slice(),
        proof.asset_chain_digest().as_bytes().as_slice(),
    ] {
        let end = offset + part.len();
        bytes[offset..end].copy_from_slice(part);
        offset = end;
    }
    debug_assert_eq!(offset, bytes.len());
    bytes
}
