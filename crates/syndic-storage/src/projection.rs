use beryl_model::{
    ProjectionRevision, SyndicItemId, SyndicProjectionId, SyndicResourceId, SyndicThreadId,
};
use sha2::{Digest, Sha256};

use crate::{
    MarkdownBlockId, MarkdownBlockKind, ProjectionFormatVersion, ProjectionPayload,
    ProjectionSourceRange, ProjectionTextSource, ResourceKind, ResourceOrdinal,
    TranscriptGeneration, TranscriptPosition,
};

const BLOCK_ID_V1: &[u8] = b"beryl/syndic/markdown-block/v1\0";
const PROJECTION_ID_V1: &[u8] = b"beryl/syndic/projection/v1\0";
const RESOURCE_ID_V1: &[u8] = b"beryl/syndic/resource/v1\0";
const RESOURCE_CONTENT_DIGEST_V1: &[u8] = b"beryl/syndic/resource-content/v1\0";
const ITEM_SET_DIGEST_V1: &[u8] = b"beryl/syndic/item-projection-set/v1\0";
const TRANSCRIPT_DIGEST_V1: &[u8] = b"beryl/syndic/transcript-entries/v1\0";

pub(crate) fn markdown_block_id(
    format: ProjectionFormatVersion,
    item: SyndicItemId,
    block_start: u64,
    kind: MarkdownBlockKind,
) -> MarkdownBlockId {
    let mut hash = Sha256::new();
    hash.update(BLOCK_ID_V1);
    hash.update([format_tag(format)]);
    hash.update(item.as_bytes());
    hash.update(block_start.to_be_bytes());
    hash.update(block_kind_bytes(kind));
    MarkdownBlockId::from_bytes(hash.finalize().into())
}

pub(crate) fn projection_identity(
    format: ProjectionFormatVersion,
    item: SyndicItemId,
    block_start: u64,
    ordinal: crate::ProjectionOrdinal,
    payload: &ProjectionPayload,
) -> (SyndicProjectionId, ProjectionRevision) {
    let mut hash = Sha256::new();
    hash.update(PROJECTION_ID_V1);
    hash.update([format_tag(format)]);
    hash.update(item.as_bytes());
    hash.update(block_start.to_be_bytes());
    hash.update(ordinal.get().to_be_bytes());
    hash_projection_payload(&mut hash, payload);
    let digest: [u8; 32] = hash.finalize().into();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest[..16]);
    (
        SyndicProjectionId::from_bytes(id),
        ProjectionRevision::new(1).expect("immutable projection revision is nonzero"),
    )
}

fn hash_projection_payload(hash: &mut Sha256, payload: &ProjectionPayload) {
    match payload {
        ProjectionPayload::Empty => hash.update([0]),
        ProjectionPayload::InlineMarkdown {
            block_id,
            block_kind,
            span_ordinal,
            source_range,
            source,
        } => {
            hash.update([1]);
            hash.update(block_id.as_bytes());
            hash.update(block_kind_bytes(*block_kind));
            hash.update(span_ordinal.to_be_bytes());
            hash_range(hash, *source_range);
            hash.update(Sha256::digest(source.as_bytes()));
        }
        ProjectionPayload::ResourceReference {
            block_id,
            block_kind,
            source_range,
            resource_id,
            preview,
        } => {
            hash.update([2]);
            hash.update(block_id.as_bytes());
            hash.update(block_kind_bytes(*block_kind));
            hash_range(hash, *source_range);
            hash.update(resource_id.as_bytes());
            hash.update(Sha256::digest(preview.as_bytes()));
        }
        ProjectionPayload::ImageMarker {
            atom_ordinal,
            marker_ordinal,
            source_offset,
            marker,
        } => {
            hash.update([3]);
            hash.update(atom_ordinal.get().to_be_bytes());
            hash.update(marker_ordinal.get().to_be_bytes());
            hash.update(source_offset.to_be_bytes());
            hash.update(marker.marker_id().as_bytes());
            hash.update(marker.label().get().to_be_bytes());
        }
    }
}

pub(crate) fn resource_identity(
    format: ProjectionFormatVersion,
    item: SyndicItemId,
    ordinal: ResourceOrdinal,
    kind: ResourceKind,
    source: ProjectionTextSource,
    range: ProjectionSourceRange,
) -> (SyndicResourceId, ProjectionRevision) {
    let mut hash = Sha256::new();
    hash.update(RESOURCE_ID_V1);
    hash.update([format_tag(format)]);
    hash.update(item.as_bytes());
    hash.update(ordinal.get().to_be_bytes());
    hash.update([resource_kind_tag(kind)]);
    hash_projection_source_identity(&mut hash, source);
    hash_range(&mut hash, range);
    let digest: [u8; 32] = hash.finalize().into();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest[..16]);
    (
        SyndicResourceId::from_bytes(id),
        ProjectionRevision::new(1).expect("immutable resource revision is nonzero"),
    )
}

fn hash_projection_source_identity(hash: &mut Sha256, source: ProjectionTextSource) {
    match source {
        ProjectionTextSource::Composer(content) => {
            hash.update([0]);
            hash.update(content.id().as_bytes());
        }
        ProjectionTextSource::ProviderNarrative(narrative) => {
            hash.update([1]);
            hash.update(narrative.content_id().as_bytes());
            hash.update(narrative.generation().get().to_be_bytes());
        }
    }
}

pub(crate) fn item_set_digest_seed() -> [u8; 32] {
    Sha256::digest(ITEM_SET_DIGEST_V1).into()
}

pub(crate) fn resource_content_digest_seed(kind: ResourceKind) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(RESOURCE_CONTENT_DIGEST_V1);
    hash.update([resource_kind_tag(kind)]);
    hash.finalize().into()
}

pub(crate) fn advance_resource_content_digest(current: [u8; 32], bytes: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(RESOURCE_CONTENT_DIGEST_V1);
    hash.update(current);
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
    hash.finalize().into()
}

pub(crate) fn advance_item_set_digest(
    current: [u8; 32],
    projection: SyndicProjectionId,
    revision: ProjectionRevision,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(ITEM_SET_DIGEST_V1);
    hash.update(current);
    hash.update([0]);
    hash.update(projection.as_bytes());
    hash.update(revision.get().to_be_bytes());
    hash.finalize().into()
}

pub(crate) fn advance_item_set_resource_digest(
    current: [u8; 32],
    resource: SyndicResourceId,
    revision: ProjectionRevision,
    digest: [u8; 32],
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(ITEM_SET_DIGEST_V1);
    hash.update(current);
    hash.update([1]);
    hash.update(resource.as_bytes());
    hash.update(revision.get().to_be_bytes());
    hash.update(digest);
    hash.finalize().into()
}

pub(crate) fn transcript_entry_digest_seed() -> [u8; 32] {
    Sha256::digest(TRANSCRIPT_DIGEST_V1).into()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn advance_transcript_entry_digest(
    current: [u8; 32],
    thread: SyndicThreadId,
    generation: TranscriptGeneration,
    position: TranscriptPosition,
    item: SyndicItemId,
    item_revision: ProjectionRevision,
    item_generation: crate::ItemProjectionGeneration,
    projection: SyndicProjectionId,
    revision: ProjectionRevision,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(TRANSCRIPT_DIGEST_V1);
    hash.update(current);
    hash.update(thread.as_bytes());
    hash.update(generation.get().to_be_bytes());
    hash.update(position.get().to_be_bytes());
    hash.update(item.as_bytes());
    hash.update(item_revision.get().to_be_bytes());
    hash.update(item_generation.get().to_be_bytes());
    hash.update(projection.as_bytes());
    hash.update(revision.get().to_be_bytes());
    hash.finalize().into()
}

const fn format_tag(format: ProjectionFormatVersion) -> u8 {
    match format {
        ProjectionFormatVersion::V1 => 1,
    }
}

fn block_kind_bytes(kind: MarkdownBlockKind) -> [u8; 2] {
    match kind {
        MarkdownBlockKind::Paragraph => [0, 0],
        MarkdownBlockKind::Heading(level) => [1, level],
        MarkdownBlockKind::BlockQuote => [2, 0],
        MarkdownBlockKind::List => [3, 0],
        MarkdownBlockKind::ThematicBreak => [4, 0],
        MarkdownBlockKind::FencedCode => [5, 0],
        MarkdownBlockKind::Table => [6, 0],
        MarkdownBlockKind::Fallback => [7, 0],
    }
}

const fn resource_kind_tag(kind: ResourceKind) -> u8 {
    match kind {
        ResourceKind::Code => 0,
        ResourceKind::Table => 1,
        ResourceKind::Image => 2,
        ResourceKind::Attachment => 3,
        ResourceKind::Log => 4,
        ResourceKind::Other => 5,
    }
}

fn hash_range(hash: &mut Sha256, range: ProjectionSourceRange) {
    hash.update(range.start().to_be_bytes());
    hash.update(range.end().to_be_bytes());
}
