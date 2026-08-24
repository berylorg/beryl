use beryl_backend::{StreamedInputSourceIdentity, StreamedTextSourceId, TextSourceProof};
use beryl_home_store::HomeGeneration;
use beryl_model::{BerylHomeId, ImageLabelOrdinal, RuntimeMode, SealedAssetReferenceSetProof};
use beryl_state::{AssetLabelDisposition, AssetReferenceEntryRecord, RecordRevision};
use sha2::{Digest, Sha256};
use syndic_storage::{
    ContentEncoding, ContentReference, SyndicContentTextSegment, SyndicContentTextSegmentBoundary,
};

use super::{error::MarkerReplayError, source::MarkerSource};
use crate::cas_projection::input_replay::InputReplayRecord;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GeneratedLabelKind {
    First,
    Repeated,
}

impl GeneratedLabelKind {
    pub(super) const fn from_disposition(disposition: AssetLabelDisposition) -> Self {
        match disposition {
            AssetLabelDisposition::First => Self::First,
            AssetLabelDisposition::Repeated { .. } => Self::Repeated,
        }
    }
}

pub(super) fn generated_label(kind: GeneratedLabelKind, label: ImageLabelOrdinal) -> Box<str> {
    match kind {
        GeneratedLabelKind::First => format!("Image {label}:").into_boxed_str(),
        GeneratedLabelKind::Repeated => format!("[Image {label}]").into_boxed_str(),
    }
}

pub(super) struct TextRunBlueprint {
    pub(super) descriptor_ordinal: u64,
    pub(super) start_boundary: Option<SyndicContentTextSegmentBoundary>,
    pub(super) end_boundary: Option<SyndicContentTextSegmentBoundary>,
    pub(super) source_id: StreamedTextSourceId,
    pub(super) proof: TextSourceProof,
    pub(super) utf8_len: u64,
}

pub(super) struct TextRunBuilder {
    hasher: Sha256,
    descriptor_ordinal: u64,
    start_boundary: Option<SyndicContentTextSegmentBoundary>,
    utf8_len: u64,
}

impl TextRunBuilder {
    pub(super) fn new(
        source: &MarkerSource,
        descriptor_ordinal: u64,
        run_ordinal: u64,
        start_boundary: Option<SyndicContentTextSegmentBoundary>,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"beryl.app.input-replay-marker-text-proof.v1\0");
        hasher.update(source.source_identity().as_bytes());
        hasher.update(source.source_revision().get().to_be_bytes());
        source.record().hash_into(&mut hasher);
        hasher.update(source.content().id().as_bytes());
        hasher.update(source.content().summary().digest().as_bytes());
        hasher.update(source.asset_proof().set_id().as_bytes());
        hasher.update(source.asset_proof().asset_chain_digest().as_bytes());
        hasher.update(source.owner_revision().get().to_be_bytes());
        hasher.update(descriptor_ordinal.to_be_bytes());
        hasher.update(run_ordinal.to_be_bytes());
        hash_boundary(&mut hasher, start_boundary);
        Self {
            hasher,
            descriptor_ordinal,
            start_boundary,
            utf8_len: 0,
        }
    }

    pub(super) fn push_segment(
        &mut self,
        segment: &SyndicContentTextSegment,
        marker_entry: Option<&AssetReferenceEntryRecord>,
    ) -> Result<(), MarkerReplayError> {
        self.hasher.update(b"segment\0");
        self.hasher.update(segment.start().to_be_bytes());
        self.hasher.update(segment.end().to_be_bytes());
        hash_boundary(&mut self.hasher, segment.preceding_marker());
        hash_boundary(&mut self.hasher, segment.following_marker());
        let authored_len = segment
            .end()
            .checked_sub(segment.start())
            .ok_or(MarkerReplayError::InvalidDescriptor)?;
        self.utf8_len = self
            .utf8_len
            .checked_add(authored_len)
            .ok_or(MarkerReplayError::InvalidDescriptor)?;

        if let Some(entry) = marker_entry {
            let kind = GeneratedLabelKind::from_disposition(entry.label_disposition());
            let generated = generated_label(kind, entry.label());
            self.hasher.update(b"generated\0");
            self.hasher.update(entry.ordinal().get().to_be_bytes());
            self.hasher.update(entry.marker_id().as_bytes());
            self.hasher.update(entry.label().get().to_be_bytes());
            match entry.label_disposition() {
                AssetLabelDisposition::First => self.hasher.update([0_u8]),
                AssetLabelDisposition::Repeated { first_ordinal } => {
                    self.hasher.update([1_u8]);
                    self.hasher.update(first_ordinal.get().to_be_bytes());
                }
            }
            self.hasher.update((generated.len() as u64).to_be_bytes());
            self.hasher.update(generated.as_bytes());
            self.utf8_len = self
                .utf8_len
                .checked_add(generated.len() as u64)
                .ok_or(MarkerReplayError::InvalidDescriptor)?;
        }
        Ok(())
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.utf8_len == 0
    }

    pub(super) fn finish(
        mut self,
        source: &MarkerSource,
        end_boundary: Option<SyndicContentTextSegmentBoundary>,
    ) -> Result<TextRunBlueprint, MarkerReplayError> {
        if self.utf8_len == 0 {
            return Err(MarkerReplayError::InvalidDescriptor);
        }
        hash_boundary(&mut self.hasher, end_boundary);
        self.hasher.update(self.utf8_len.to_be_bytes());
        let proof = TextSourceProof::new(self.hasher.finalize().into());
        Ok(TextRunBlueprint {
            descriptor_ordinal: self.descriptor_ordinal,
            start_boundary: self.start_boundary,
            end_boundary,
            source_id: text_source_id(source.source_identity(), self.descriptor_ordinal),
            proof,
            utf8_len: self.utf8_len,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn marker_source_identity(
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    record: &InputReplayRecord,
    content: ContentReference,
    asset_proof: SealedAssetReferenceSetProof,
    owner_revision: RecordRevision,
    runtime_mode: &RuntimeMode,
) -> StreamedInputSourceIdentity {
    let summary = content.summary();
    let proof_summary = asset_proof.summary();
    let mut hasher = Sha256::new();
    hasher.update(b"beryl.app.input-replay-marker-source.v1\0");
    hasher.update(home_id.as_bytes());
    hasher.update(home_generation.get().to_be_bytes());
    record.hash_into(&mut hasher);
    hasher.update(content.id().as_bytes());
    hasher.update(content.revision().get().to_be_bytes());
    hasher.update([content_encoding_tag(content.encoding())]);
    hasher.update(summary.chunk_count().to_be_bytes());
    hasher.update(summary.piece_count().to_be_bytes());
    hasher.update(summary.encoded_bytes().to_be_bytes());
    hasher.update(summary.logical_utf8_bytes().to_be_bytes());
    hasher.update(summary.atom_count().to_be_bytes());
    hasher.update(summary.image_marker_count().to_be_bytes());
    hasher.update(summary.marker_digest());
    hash_optional_label(&mut hasher, summary.maximum_image_label());
    hasher.update(summary.digest().as_bytes());
    hasher.update(asset_proof.set_id().as_bytes());
    hasher.update(proof_summary.marker_digest());
    hasher.update(proof_summary.marker_count().to_be_bytes());
    hash_optional_label(&mut hasher, proof_summary.maximum_image_label());
    hasher.update(asset_proof.entry_frontier().to_be_bytes());
    hasher.update(asset_proof.asset_chain_digest().as_bytes());
    hasher.update(owner_revision.get().to_be_bytes());
    hash_runtime_mode(&mut hasher, runtime_mode);
    StreamedInputSourceIdentity::new(hasher.finalize().into())
}

fn text_source_id(
    source_identity: StreamedInputSourceIdentity,
    descriptor_ordinal: u64,
) -> StreamedTextSourceId {
    let mut hasher = Sha256::new();
    hasher.update(b"beryl.app.input-replay-marker-text-source.v1\0");
    hasher.update(source_identity.as_bytes());
    hasher.update(descriptor_ordinal.to_be_bytes());
    StreamedTextSourceId::new(hasher.finalize().into())
}

fn hash_boundary(hasher: &mut Sha256, boundary: Option<SyndicContentTextSegmentBoundary>) {
    let Some(boundary) = boundary else {
        hasher.update([0_u8]);
        return;
    };
    hasher.update([1_u8]);
    hasher.update(boundary.piece_ordinal().get().to_be_bytes());
    hasher.update(boundary.marker_ordinal().get().to_be_bytes());
    hasher.update(boundary.logical_offset().to_be_bytes());
    hasher.update(boundary.marker_id().as_bytes());
    hasher.update(boundary.label().get().to_be_bytes());
}

fn hash_optional_label(hasher: &mut Sha256, label: Option<ImageLabelOrdinal>) {
    match label {
        None => hasher.update([0_u8]),
        Some(label) => {
            hasher.update([1_u8]);
            hasher.update(label.get().to_be_bytes());
        }
    }
}

fn hash_runtime_mode(hasher: &mut Sha256, runtime_mode: &RuntimeMode) {
    match runtime_mode {
        RuntimeMode::Host => hasher.update([0_u8]),
        RuntimeMode::Wsl(distribution) => {
            hasher.update([1_u8]);
            hasher.update((distribution.as_str().len() as u64).to_be_bytes());
            hasher.update(distribution.as_str().as_bytes());
        }
    }
}

const fn content_encoding_tag(encoding: ContentEncoding) -> u8 {
    match encoding {
        ContentEncoding::ComposerV1 => 1,
        ContentEncoding::Utf8V1 => 2,
        ContentEncoding::ProviderItemV1 => 3,
    }
}
