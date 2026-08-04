use thiserror::Error;

use crate::ImageDetail;

use super::digest::{
    StreamedInputHeader, StreamedInputSequenceDigest, StreamedInputSequenceDigestError,
    StreamedInputSourceIdentity, StreamedInputSourceRevision, StreamedTextSourceId,
    TextSourceProof,
};

/// Metadata for one nonempty logical submitted-text run.
#[derive(Debug, Eq, PartialEq)]
pub struct StreamedTextDescriptor {
    source_id: StreamedTextSourceId,
    proof: TextSourceProof,
    utf8_len: u64,
}

impl StreamedTextDescriptor {
    #[must_use]
    pub const fn new(
        source_id: StreamedTextSourceId,
        proof: TextSourceProof,
        utf8_len: u64,
    ) -> Self {
        Self {
            source_id,
            proof,
            utf8_len,
        }
    }

    #[must_use]
    pub const fn source_id(&self) -> StreamedTextSourceId {
        self.source_id
    }

    #[must_use]
    pub const fn proof(&self) -> TextSourceProof {
        self.proof
    }

    #[must_use]
    pub const fn utf8_len(&self) -> u64 {
        self.utf8_len
    }
}

/// Exact runtime-local image descriptor for the current source position.
#[derive(Debug)]
pub struct StreamedLocalImageDescriptor {
    path: Box<str>,
    detail: Option<ImageDetail>,
}

impl StreamedLocalImageDescriptor {
    #[must_use]
    pub fn new(path: impl Into<Box<str>>, detail: Option<ImageDetail>) -> Self {
        Self {
            path: path.into(),
            detail,
        }
    }

    #[must_use]
    pub const fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub const fn detail(&self) -> Option<ImageDetail> {
        self.detail
    }
}

impl PartialEq for StreamedLocalImageDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.detail == other.detail
    }
}

impl Eq for StreamedLocalImageDescriptor {}

/// Closed value carried by one descriptor event.
#[derive(Debug, Eq, PartialEq)]
pub enum StreamedInputDescriptorKind {
    Text(StreamedTextDescriptor),
    LocalImage(StreamedLocalImageDescriptor),
}

/// One source-position-bound submitted-input descriptor event.
#[derive(Debug, Eq, PartialEq)]
pub struct StreamedInputDescriptor {
    source_identity: StreamedInputSourceIdentity,
    source_revision: StreamedInputSourceRevision,
    item_ordinal: u64,
    kind: StreamedInputDescriptorKind,
}

impl StreamedInputDescriptor {
    #[must_use]
    pub const fn new(
        source_identity: StreamedInputSourceIdentity,
        source_revision: StreamedInputSourceRevision,
        item_ordinal: u64,
        kind: StreamedInputDescriptorKind,
    ) -> Self {
        Self {
            source_identity,
            source_revision,
            item_ordinal,
            kind,
        }
    }

    #[must_use]
    pub const fn source_identity(&self) -> StreamedInputSourceIdentity {
        self.source_identity
    }

    #[must_use]
    pub const fn source_revision(&self) -> StreamedInputSourceRevision {
        self.source_revision
    }

    #[must_use]
    pub const fn item_ordinal(&self) -> u64 {
        self.item_ordinal
    }

    #[must_use]
    pub const fn kind(&self) -> &StreamedInputDescriptorKind {
        &self.kind
    }

    #[must_use]
    pub fn into_kind(self) -> StreamedInputDescriptorKind {
        self.kind
    }
}

/// One owned bounded valid-UTF-8 page for the current text descriptor.
#[derive(Debug)]
pub struct StreamedTextPage {
    source_identity: StreamedInputSourceIdentity,
    source_revision: StreamedInputSourceRevision,
    source_id: StreamedTextSourceId,
    proof: TextSourceProof,
    start: u64,
    text: Box<str>,
    next_offset: Option<u64>,
}

impl StreamedTextPage {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        source_identity: StreamedInputSourceIdentity,
        source_revision: StreamedInputSourceRevision,
        source_id: StreamedTextSourceId,
        proof: TextSourceProof,
        start: u64,
        text: impl Into<Box<str>>,
        next_offset: Option<u64>,
    ) -> Self {
        Self {
            source_identity,
            source_revision,
            source_id,
            proof,
            start,
            text: text.into(),
            next_offset,
        }
    }

    #[must_use]
    pub const fn source_identity(&self) -> StreamedInputSourceIdentity {
        self.source_identity
    }

    #[must_use]
    pub const fn source_revision(&self) -> StreamedInputSourceRevision {
        self.source_revision
    }

    #[must_use]
    pub const fn source_id(&self) -> StreamedTextSourceId {
        self.source_id
    }

    #[must_use]
    pub const fn proof(&self) -> TextSourceProof {
        self.proof
    }

    #[must_use]
    pub const fn start(&self) -> u64 {
        self.start
    }

    #[must_use]
    pub const fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn next_offset(&self) -> Option<u64> {
        self.next_offset
    }
}

impl PartialEq for StreamedTextPage {
    fn eq(&self, other: &Self) -> bool {
        self.source_identity == other.source_identity
            && self.source_revision == other.source_revision
            && self.source_id == other.source_id
            && self.proof == other.proof
            && self.start == other.start
            && self.text == other.text
            && self.next_offset == other.next_offset
    }
}

impl Eq for StreamedTextPage {}

/// One owned mutable descriptor source, replayed sequentially for each pass.
pub trait StreamedInputSource: Send {
    /// Returns the immutable declaration captured before any source replay.
    fn header(&self) -> StreamedInputHeader;

    /// Opens an independent pass and returns its current declaration.
    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError>;

    /// Advances to the next complete descriptor, or returns exact sequence EOF.
    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError>;

    /// Reads one page at the exact absolute text-source offset.
    fn read_text_page(
        &mut self,
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError>;
}

/// Exact source, replay, or structural failure for submitted input.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum StreamedInputSourceError {
    #[error("submitted-input source operation was cancelled")]
    Cancelled,
    #[error("submitted-input source broker was lost")]
    BrokerLost,
    #[error("submitted-input source read failed")]
    ReadFailed,
    #[error("submitted-input durable source was invalid")]
    InvalidSource,
    #[error("submitted-input verifier proof state was unavailable")]
    VerifierUnavailable,
    #[error("submitted-input source identity changed")]
    SourceIdentityMismatch {
        expected: StreamedInputSourceIdentity,
        actual: StreamedInputSourceIdentity,
    },
    #[error("submitted-input source revision changed")]
    RevisionDrift {
        expected: StreamedInputSourceRevision,
        actual: StreamedInputSourceRevision,
    },
    #[error("submitted-input pass declared {actual} items, expected {expected}")]
    DeclaredItemCountMismatch { expected: u64, actual: u64 },
    #[error("submitted-input pass declared the wrong sequence digest")]
    DeclaredSequenceDigestMismatch {
        expected: StreamedInputSequenceDigest,
        actual: StreamedInputSequenceDigest,
    },
    #[error("descriptor ordinal {actual} did not match expected ordinal {expected}")]
    DescriptorOrdinalMismatch { expected: u64, actual: u64 },
    #[error("descriptor sequence ended after {actual} items, expected {expected}")]
    DescriptorCountMismatch { expected: u64, actual: u64 },
    #[error("descriptor source returned an item after declared item {declared}")]
    UnexpectedDescriptor { declared: u64, actual_ordinal: u64 },
    #[error("text descriptor {item_ordinal} was not one maximal nonempty run")]
    MalformedTextSegmentation { item_ordinal: u64 },
    #[error("text page source id did not match descriptor {item_ordinal}")]
    TextSourceIdMismatch { item_ordinal: u64 },
    #[error("text page proof did not match descriptor {item_ordinal}")]
    TextProofMismatch { item_ordinal: u64 },
    #[error("text page began at {actual}, expected absolute offset {expected}")]
    PageStartMismatch { expected: u64, actual: u64 },
    #[error("text page contained {actual} bytes, exceeding requested maximum {maximum}")]
    PageTooLarge { maximum: usize, actual: usize },
    #[error("text page at absolute offset {start} was empty")]
    EmptyPage { start: u64 },
    #[error("text page end overflowed from offset {start} with {page_bytes} bytes")]
    PageEndOverflow { start: u64, page_bytes: usize },
    #[error("text page ended at {end}, beyond declared text length {utf8_len}")]
    PagePastEnd { end: u64, utf8_len: u64 },
    #[error("text source ended at {end}, before declared text length {utf8_len}")]
    PrematureEof { end: u64, utf8_len: u64 },
    #[error("text page continuation {next_offset} disagreed with end {end} and length {utf8_len}")]
    InvalidNextOffset {
        end: u64,
        next_offset: u64,
        utf8_len: u64,
    },
    #[error("submitted-input descriptor sequence digest disagreed with its header")]
    SequenceDigestMismatch {
        expected: StreamedInputSequenceDigest,
        actual: StreamedInputSequenceDigest,
    },
    #[error("submitted-input descriptor digest construction failed")]
    Digest {
        #[from]
        source: StreamedInputSequenceDigestError,
    },
}
