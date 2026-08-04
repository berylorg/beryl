use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::ImageDetail;

const DIGEST_DOMAIN: &[u8] = b"beryl-streamed-user-input-descriptor-sequence-v1\0";

/// Exact identity of one replayable submitted-input descriptor source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StreamedInputSourceIdentity([u8; 32]);

impl StreamedInputSourceIdentity {
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Immutable revision of one replayable descriptor source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StreamedInputSourceRevision(u64);

impl StreamedInputSourceRevision {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Canonical V1 digest of one submitted-input descriptor sequence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StreamedInputSequenceDigest([u8; 32]);

impl StreamedInputSequenceDigest {
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Immutable compact declaration for all passes over one source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamedInputHeader {
    source_identity: StreamedInputSourceIdentity,
    source_revision: StreamedInputSourceRevision,
    item_count: u64,
    sequence_digest: StreamedInputSequenceDigest,
}

impl StreamedInputHeader {
    #[must_use]
    pub const fn new(
        source_identity: StreamedInputSourceIdentity,
        source_revision: StreamedInputSourceRevision,
        item_count: u64,
        sequence_digest: StreamedInputSequenceDigest,
    ) -> Self {
        Self {
            source_identity,
            source_revision,
            item_count,
            sequence_digest,
        }
    }

    #[must_use]
    pub const fn source_identity(self) -> StreamedInputSourceIdentity {
        self.source_identity
    }

    #[must_use]
    pub const fn source_revision(self) -> StreamedInputSourceRevision {
        self.source_revision
    }

    #[must_use]
    pub const fn item_count(self) -> u64 {
        self.item_count
    }

    #[must_use]
    pub const fn sequence_digest(self) -> StreamedInputSequenceDigest {
        self.sequence_digest
    }
}

/// Immutable proof for one exact logical text byte sequence and provenance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextSourceProof([u8; 32]);

impl TextSourceProof {
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Request-local routing identity for one current text descriptor.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StreamedTextSourceId([u8; 32]);

impl StreamedTextSourceId {
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Structural failure while constructing the canonical V1 sequence digest.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum StreamedInputSequenceDigestError {
    #[error("descriptor ordinal {actual} did not match expected ordinal {expected}")]
    ItemOrdinalMismatch { expected: u64, actual: u64 },
    #[error("descriptor sequence exceeded its declared item count {declared}")]
    TooManyItems { declared: u64 },
    #[error("descriptor sequence contained {actual} items, expected {expected}")]
    ItemCountMismatch { expected: u64, actual: u64 },
    #[error("text descriptor {item_ordinal} declared zero UTF-8 bytes")]
    EmptyTextDescriptor { item_ordinal: u64 },
    #[error("local-image descriptor {item_ordinal} path length exceeded u64")]
    ImagePathLengthOverflow { item_ordinal: u64 },
}

/// Single authoritative builder for the canonical V1 descriptor digest.
pub struct StreamedInputSequenceDigestAccumulator {
    hasher: Sha256,
    declared_item_count: u64,
    observed_item_count: u64,
}

impl StreamedInputSequenceDigestAccumulator {
    #[must_use]
    pub fn new(declared_item_count: u64) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(DIGEST_DOMAIN);
        hasher.update(declared_item_count.to_be_bytes());
        Self {
            hasher,
            declared_item_count,
            observed_item_count: 0,
        }
    }

    pub fn push_text(
        &mut self,
        item_ordinal: u64,
        proof: TextSourceProof,
        utf8_len: u64,
    ) -> Result<(), StreamedInputSequenceDigestError> {
        self.begin_item(item_ordinal)?;
        if utf8_len == 0 {
            return Err(StreamedInputSequenceDigestError::EmptyTextDescriptor { item_ordinal });
        }
        self.hasher.update(item_ordinal.to_be_bytes());
        self.hasher.update([0x01]);
        self.hasher.update(proof.as_bytes());
        self.hasher.update(utf8_len.to_be_bytes());
        self.observed_item_count += 1;
        Ok(())
    }

    pub fn push_local_image(
        &mut self,
        item_ordinal: u64,
        detail: Option<ImageDetail>,
        path: &str,
    ) -> Result<(), StreamedInputSequenceDigestError> {
        self.begin_item(item_ordinal)?;
        let path_len = u64::try_from(path.len()).map_err(|_| {
            StreamedInputSequenceDigestError::ImagePathLengthOverflow { item_ordinal }
        })?;
        self.hasher.update(item_ordinal.to_be_bytes());
        self.hasher.update([0x02, image_detail_byte(detail)]);
        self.hasher.update(path_len.to_be_bytes());
        self.hasher.update(path.as_bytes());
        self.observed_item_count += 1;
        Ok(())
    }

    pub fn finish(self) -> Result<StreamedInputSequenceDigest, StreamedInputSequenceDigestError> {
        if self.observed_item_count != self.declared_item_count {
            return Err(StreamedInputSequenceDigestError::ItemCountMismatch {
                expected: self.declared_item_count,
                actual: self.observed_item_count,
            });
        }
        Ok(StreamedInputSequenceDigest::new(
            self.hasher.finalize().into(),
        ))
    }

    fn begin_item(&self, item_ordinal: u64) -> Result<(), StreamedInputSequenceDigestError> {
        if self.observed_item_count == self.declared_item_count {
            return Err(StreamedInputSequenceDigestError::TooManyItems {
                declared: self.declared_item_count,
            });
        }
        let expected = self.observed_item_count + 1;
        if item_ordinal != expected {
            return Err(StreamedInputSequenceDigestError::ItemOrdinalMismatch {
                expected,
                actual: item_ordinal,
            });
        }
        Ok(())
    }
}

const fn image_detail_byte(detail: Option<ImageDetail>) -> u8 {
    match detail {
        None => 0x00,
        Some(ImageDetail::Auto) => 0x01,
        Some(ImageDetail::Low) => 0x02,
        Some(ImageDetail::High) => 0x03,
        Some(ImageDetail::Original) => 0x04,
    }
}
