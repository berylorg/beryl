use beryl_model::{
    ContentRevision, SealedContentMarkerSummary, SequentialMarkerSummaryV1, SyndicContentDigest,
    SyndicContentId, SyndicItemId,
};
use sha2::{Digest, Sha256};

use crate::{ContentChunkOrdinal, ContentEncoding, ContentLifecycle, SyndicRecordError};

mod span;

pub use span::*;

/// Maximum encoded bytes carried by one physical content-chunk value.
pub const CONTENT_CHUNK_MAX_BYTES: usize = 65_536;

/// Final exact byte, chunk, and ordered render-piece summary of one content object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentSummary {
    chunk_count: u64,
    piece_count: u64,
    encoded_bytes: u64,
    logical_utf8_bytes: u64,
    atom_count: u64,
    image_marker_count: u64,
    marker_digest: [u8; 32],
    maximum_image_label: Option<crate::ImageLabelOrdinal>,
    digest: SyndicContentDigest,
}

impl ContentSummary {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        chunk_count: u64,
        piece_count: u64,
        encoded_bytes: u64,
        logical_utf8_bytes: u64,
        atom_count: u64,
        image_marker_count: u64,
        marker_digest: [u8; 32],
        maximum_image_label: Option<crate::ImageLabelOrdinal>,
        digest: SyndicContentDigest,
    ) -> Result<Self, SyndicRecordError> {
        if (image_marker_count == 0) != maximum_image_label.is_none() {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        Ok(Self {
            chunk_count,
            piece_count,
            encoded_bytes,
            logical_utf8_bytes,
            atom_count,
            image_marker_count,
            marker_digest,
            maximum_image_label,
            digest,
        })
    }

    #[must_use]
    pub const fn chunk_count(self) -> u64 {
        self.chunk_count
    }
    #[must_use]
    pub const fn piece_count(self) -> u64 {
        self.piece_count
    }
    #[must_use]
    pub const fn encoded_bytes(self) -> u64 {
        self.encoded_bytes
    }
    #[must_use]
    pub const fn logical_utf8_bytes(self) -> u64 {
        self.logical_utf8_bytes
    }
    #[must_use]
    pub const fn atom_count(self) -> u64 {
        self.atom_count
    }
    #[must_use]
    pub const fn image_marker_count(self) -> u64 {
        self.image_marker_count
    }
    #[must_use]
    pub const fn marker_digest(self) -> [u8; 32] {
        self.marker_digest
    }
    #[must_use]
    pub const fn maximum_image_label(self) -> Option<crate::ImageLabelOrdinal> {
        self.maximum_image_label
    }
    #[must_use]
    pub const fn digest(self) -> SyndicContentDigest {
        self.digest
    }
}

/// Immutable exact reference published by a logical content owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentReference {
    id: SyndicContentId,
    revision: ContentRevision,
    encoding: ContentEncoding,
    summary: ContentSummary,
}

impl ContentReference {
    #[must_use]
    pub const fn new(
        id: SyndicContentId,
        revision: ContentRevision,
        encoding: ContentEncoding,
        summary: ContentSummary,
    ) -> Self {
        Self {
            id,
            revision,
            encoding,
            summary,
        }
    }

    #[must_use]
    pub const fn id(self) -> SyndicContentId {
        self.id
    }
    #[must_use]
    pub const fn revision(self) -> ContentRevision {
        self.revision
    }
    #[must_use]
    pub const fn encoding(self) -> ContentEncoding {
        self.encoding
    }
    #[must_use]
    pub const fn summary(self) -> ContentSummary {
        self.summary
    }

    pub const fn sealed_marker_summary(
        self,
    ) -> Result<SealedContentMarkerSummary, beryl_model::AssetProofError> {
        match SequentialMarkerSummaryV1::new(
            self.summary.marker_digest(),
            self.summary.image_marker_count(),
            self.summary.maximum_image_label(),
        ) {
            Ok(sequential) => Ok(SealedContentMarkerSummary::new(
                self.id,
                self.summary.digest(),
                sequential,
            )),
            Err(error) => Err(error),
        }
    }
}

/// Durable construction frontier and expected final manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentManifestRecord {
    id: SyndicContentId,
    owner: Option<SyndicItemId>,
    revision: ContentRevision,
    encoding: ContentEncoding,
    lifecycle: ContentLifecycle,
    chunk_count: u64,
    encoded_bytes: u64,
    chain_digest: SyndicContentDigest,
    expected: ContentSummary,
}

impl ContentManifestRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        id: SyndicContentId,
        revision: ContentRevision,
        encoding: ContentEncoding,
        lifecycle: ContentLifecycle,
        chunk_count: u64,
        encoded_bytes: u64,
        chain_digest: SyndicContentDigest,
        expected: ContentSummary,
    ) -> Self {
        Self {
            id,
            owner: None,
            revision,
            encoding,
            lifecycle,
            chunk_count,
            encoded_bytes,
            chain_digest,
            expected,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn with_owner(
        id: SyndicContentId,
        owner: Option<SyndicItemId>,
        revision: ContentRevision,
        encoding: ContentEncoding,
        lifecycle: ContentLifecycle,
        chunk_count: u64,
        encoded_bytes: u64,
        chain_digest: SyndicContentDigest,
        expected: ContentSummary,
    ) -> Self {
        Self {
            id,
            owner,
            revision,
            encoding,
            lifecycle,
            chunk_count,
            encoded_bytes,
            chain_digest,
            expected,
        }
    }

    pub(crate) fn live(
        id: SyndicContentId,
        owner: SyndicItemId,
        revision: ContentRevision,
    ) -> Self {
        let digest = content_chain_seed(ContentEncoding::Utf8V1);
        let summary = ContentSummary::new(
            0,
            0,
            0,
            0,
            1,
            0,
            crate::content::input_marker_digest(std::iter::empty()),
            None,
            digest,
        )
        .expect("marker-free live content summary is valid");
        Self::with_owner(
            id,
            Some(owner),
            revision,
            ContentEncoding::Utf8V1,
            ContentLifecycle::Live,
            0,
            0,
            digest,
            summary,
        )
    }

    #[must_use]
    pub const fn id(&self) -> SyndicContentId {
        self.id
    }
    #[must_use]
    pub const fn owner(&self) -> Option<SyndicItemId> {
        self.owner
    }
    #[must_use]
    pub const fn revision(&self) -> ContentRevision {
        self.revision
    }
    #[must_use]
    pub const fn encoding(&self) -> ContentEncoding {
        self.encoding
    }
    #[must_use]
    pub const fn lifecycle(&self) -> ContentLifecycle {
        self.lifecycle
    }
    #[must_use]
    pub const fn chunk_count(&self) -> u64 {
        self.chunk_count
    }
    #[must_use]
    pub const fn encoded_bytes(&self) -> u64 {
        self.encoded_bytes
    }
    #[must_use]
    pub const fn chain_digest(&self) -> SyndicContentDigest {
        self.chain_digest
    }
    #[must_use]
    pub const fn expected(&self) -> ContentSummary {
        self.expected
    }

    #[must_use]
    pub const fn sealed_reference(&self) -> Option<ContentReference> {
        match self.lifecycle {
            ContentLifecycle::Building | ContentLifecycle::Live | ContentLifecycle::Finalized => {
                None
            }
            ContentLifecycle::Sealed => Some(ContentReference::new(
                self.id,
                self.revision,
                self.encoding,
                self.expected,
            )),
        }
    }

    #[must_use]
    pub const fn current_reference(&self) -> Option<ContentReference> {
        match self.lifecycle {
            ContentLifecycle::Building => None,
            ContentLifecycle::Sealed | ContentLifecycle::Live | ContentLifecycle::Finalized => {
                Some(ContentReference::new(
                    self.id,
                    self.revision,
                    self.encoding,
                    self.expected,
                ))
            }
        }
    }
}

/// One bounded exact encoded chunk under a content manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentChunkRecord {
    content_id: SyndicContentId,
    ordinal: ContentChunkOrdinal,
    digest: [u8; 32],
    bytes: Box<[u8]>,
}

impl ContentChunkRecord {
    pub fn new(
        content_id: SyndicContentId,
        ordinal: ContentChunkOrdinal,
        bytes: impl Into<Box<[u8]>>,
    ) -> Result<Self, SyndicRecordError> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(SyndicRecordError::Empty {
                kind: "content chunk",
            });
        }
        if bytes.len() > CONTENT_CHUNK_MAX_BYTES {
            return Err(SyndicRecordError::BytesTooLong {
                kind: "content chunk",
                maximum: CONTENT_CHUNK_MAX_BYTES,
                actual: bytes.len(),
            });
        }
        let digest = Sha256::digest(&bytes).into();
        Ok(Self {
            content_id,
            ordinal,
            digest,
            bytes,
        })
    }

    #[must_use]
    pub const fn content_id(&self) -> SyndicContentId {
        self.content_id
    }
    #[must_use]
    pub const fn ordinal(&self) -> ContentChunkOrdinal {
        self.ordinal
    }
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    #[must_use]
    pub const fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

pub(crate) fn content_chain_seed(encoding: ContentEncoding) -> SyndicContentDigest {
    let tag = match encoding {
        ContentEncoding::ComposerV1 => 1,
        ContentEncoding::Utf8V1 => 2,
        ContentEncoding::ProviderItemV1 => 3,
    };
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.content-chain.v1");
    hash.update([tag]);
    SyndicContentDigest::from_bytes(hash.finalize().into())
}

pub(crate) fn advance_content_chain(
    previous: SyndicContentDigest,
    chunk: &ContentChunkRecord,
) -> SyndicContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.content-chunk.v1");
    hash.update(previous.as_bytes());
    hash.update(chunk.ordinal().get().to_be_bytes());
    hash.update((chunk.bytes().len() as u64).to_be_bytes());
    hash.update(chunk.digest());
    SyndicContentDigest::from_bytes(hash.finalize().into())
}
