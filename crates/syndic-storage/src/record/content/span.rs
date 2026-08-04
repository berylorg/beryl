use beryl_model::{SyndicContentId, SyndicDraftMarkerId};

use crate::{
    ComposerAtomOrdinal, ContentChunkOrdinal, ContentChunkRecord, ContentPieceOrdinal,
    ImageLabelOrdinal, InputMarkerOrdinal, SyndicRecordError,
};

/// Exact encoded byte placement of one bounded content chunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentByteSpanRecord {
    content_id: SyndicContentId,
    ordinal: ContentChunkOrdinal,
    start: u64,
    end: u64,
    chunk_digest: [u8; 32],
}

impl ContentByteSpanRecord {
    /// Constructs the exact byte span for `chunk` at `start`.
    pub fn for_chunk(chunk: &ContentChunkRecord, start: u64) -> Result<Self, SyndicRecordError> {
        let length =
            u64::try_from(chunk.bytes().len()).map_err(|_| SyndicRecordError::LengthOverflow {
                kind: "content byte span",
            })?;
        let end = start
            .checked_add(length)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "content byte span",
            })?;
        Self::new(
            chunk.content_id(),
            chunk.ordinal(),
            start,
            end,
            *chunk.digest(),
        )
    }

    /// Reconstructs one persisted exact span.
    pub fn new(
        content_id: SyndicContentId,
        ordinal: ContentChunkOrdinal,
        start: u64,
        end: u64,
        chunk_digest: [u8; 32],
    ) -> Result<Self, SyndicRecordError> {
        if start >= end {
            return Err(SyndicRecordError::InvalidByteRange {
                kind: "content byte span",
                start,
                end,
            });
        }
        Ok(Self {
            content_id,
            ordinal,
            start,
            end,
            chunk_digest,
        })
    }

    #[must_use]
    pub const fn content_id(self) -> SyndicContentId {
        self.content_id
    }

    #[must_use]
    pub const fn ordinal(self) -> ContentChunkOrdinal {
        self.ordinal
    }

    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> u64 {
        self.end
    }

    #[must_use]
    pub const fn len(self) -> u64 {
        self.end - self.start
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        false
    }

    #[must_use]
    pub const fn chunk_digest(self) -> [u8; 32] {
        self.chunk_digest
    }
}

pub(crate) fn content_byte_spans(
    chunks: &[ContentChunkRecord],
    mut start: u64,
) -> Result<Vec<ContentByteSpanRecord>, SyndicRecordError> {
    let mut spans = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let span = ContentByteSpanRecord::for_chunk(chunk, start)?;
        start = span.end();
        spans.push(span);
    }
    Ok(spans)
}

/// One bounded logical UTF-8 segment mapped into one physical content chunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentTextSpanRecord {
    content_id: SyndicContentId,
    piece_ordinal: ContentPieceOrdinal,
    chunk_ordinal: ContentChunkOrdinal,
    chunk_start: u64,
    logical_start: u64,
    logical_end: u64,
    encoded_start: u64,
    encoded_end: u64,
    break_before: bool,
    digest: [u8; 32],
}

impl ContentTextSpanRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        content_id: SyndicContentId,
        piece_ordinal: ContentPieceOrdinal,
        chunk_ordinal: ContentChunkOrdinal,
        chunk_start: u64,
        logical_start: u64,
        logical_end: u64,
        encoded_start: u64,
        encoded_end: u64,
        break_before: bool,
        digest: [u8; 32],
    ) -> Result<Self, SyndicRecordError> {
        if logical_start >= logical_end {
            return Err(SyndicRecordError::InvalidByteRange {
                kind: "content text logical range",
                start: logical_start,
                end: logical_end,
            });
        }
        if encoded_start >= encoded_end {
            return Err(SyndicRecordError::InvalidByteRange {
                kind: "content text encoded range",
                start: encoded_start,
                end: encoded_end,
            });
        }
        if chunk_start > encoded_start {
            return Err(SyndicRecordError::InvalidByteRange {
                kind: "content text chunk range",
                start: chunk_start,
                end: encoded_start,
            });
        }
        if logical_end - logical_start != encoded_end - encoded_start {
            return Err(SyndicRecordError::MappedByteLengthMismatch {
                kind: "content text span",
                logical_bytes: logical_end - logical_start,
                encoded_bytes: encoded_end - encoded_start,
            });
        }
        Ok(Self {
            content_id,
            piece_ordinal,
            chunk_ordinal,
            chunk_start,
            logical_start,
            logical_end,
            encoded_start,
            encoded_end,
            break_before,
            digest,
        })
    }

    #[must_use]
    pub const fn content_id(self) -> SyndicContentId {
        self.content_id
    }

    #[must_use]
    pub const fn piece_ordinal(self) -> ContentPieceOrdinal {
        self.piece_ordinal
    }

    #[must_use]
    pub const fn chunk_ordinal(self) -> ContentChunkOrdinal {
        self.chunk_ordinal
    }

    #[must_use]
    pub const fn chunk_start(self) -> u64 {
        self.chunk_start
    }

    #[must_use]
    pub const fn logical_start(self) -> u64 {
        self.logical_start
    }

    #[must_use]
    pub const fn logical_end(self) -> u64 {
        self.logical_end
    }

    #[must_use]
    pub const fn encoded_start(self) -> u64 {
        self.encoded_start
    }

    #[must_use]
    pub const fn encoded_end(self) -> u64 {
        self.encoded_end
    }

    #[must_use]
    pub const fn len(self) -> u64 {
        self.logical_end - self.logical_start
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        false
    }

    #[must_use]
    pub const fn break_before(self) -> bool {
        self.break_before
    }

    #[must_use]
    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }
}

/// One bounded render-significant piece in exact canonical content order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentPieceRecord {
    Text(ContentTextSpanRecord),
    ImageMarker {
        content_id: SyndicContentId,
        ordinal: ContentPieceOrdinal,
        atom_ordinal: ComposerAtomOrdinal,
        marker_ordinal: InputMarkerOrdinal,
        logical_offset: u64,
        encoded_start: u64,
        encoded_end: u64,
        marker_id: SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
        digest: [u8; 32],
    },
}

impl ContentPieceRecord {
    #[must_use]
    pub const fn text(span: ContentTextSpanRecord) -> Self {
        Self::Text(span)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn image_marker(
        content_id: SyndicContentId,
        ordinal: ContentPieceOrdinal,
        atom_ordinal: ComposerAtomOrdinal,
        marker_ordinal: InputMarkerOrdinal,
        logical_offset: u64,
        encoded_start: u64,
        encoded_end: u64,
        marker_id: SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
        digest: [u8; 32],
    ) -> Result<Self, SyndicRecordError> {
        if encoded_start >= encoded_end {
            return Err(SyndicRecordError::InvalidByteRange {
                kind: "content marker encoded range",
                start: encoded_start,
                end: encoded_end,
            });
        }
        Ok(Self::ImageMarker {
            content_id,
            ordinal,
            atom_ordinal,
            marker_ordinal,
            logical_offset,
            encoded_start,
            encoded_end,
            marker_id,
            label,
            digest,
        })
    }

    #[must_use]
    pub const fn content_id(self) -> SyndicContentId {
        match self {
            Self::Text(span) => span.content_id(),
            Self::ImageMarker { content_id, .. } => content_id,
        }
    }

    #[must_use]
    pub const fn ordinal(self) -> ContentPieceOrdinal {
        match self {
            Self::Text(span) => span.piece_ordinal(),
            Self::ImageMarker { ordinal, .. } => ordinal,
        }
    }

    #[must_use]
    pub const fn logical_offset(self) -> u64 {
        match self {
            Self::Text(span) => span.logical_start(),
            Self::ImageMarker { logical_offset, .. } => logical_offset,
        }
    }

    #[must_use]
    pub const fn encoded_end(self) -> u64 {
        match self {
            Self::Text(span) => span.encoded_end(),
            Self::ImageMarker { encoded_end, .. } => encoded_end,
        }
    }
}
