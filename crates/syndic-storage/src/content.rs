use std::convert::Infallible;

use beryl_model::{
    ContentRevision, SyndicContentId, SyndicDraftMarkerId, advance_sequential_marker_digest,
    sequential_marker_digest_seed,
};
use sha2::{Digest, Sha256};

use crate::{
    ComposerAtom, ComposerAtomOrdinal, ComposerPayload, ContentChunkOrdinal, ContentChunkRecord,
    ContentEncoding, ContentManifestRecord, ContentSummary, ImageLabelOrdinal, InputMarkerOrdinal,
    SyndicRecordError, advance_content_chain, content_chain_seed,
};

mod assembler;
mod chunks;
pub(crate) mod composer_v1;
mod decode;

pub use assembler::ComposerContentAssembler;
use chunks::{encoded_chunk_ranges, prepared_content_pieces};
use composer_v1::{
    ComposerV1AtomWriter, ComposerV1Fold, ComposerV1FoldError, ComposerV1RecordSink,
    drive_indexed_atoms,
};
pub(crate) use decode::decode_composer_content;

/// Exact identity, summary, and peak encoded buffer use derived by the discard pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerV1Plan {
    content_id: SyndicContentId,
    summary: ContentSummary,
    max_buffer_bytes: usize,
}

impl ComposerV1Plan {
    #[must_use]
    pub const fn content_id(self) -> SyndicContentId {
        self.content_id
    }

    #[must_use]
    pub const fn summary(self) -> ContentSummary {
        self.summary
    }

    #[must_use]
    pub const fn max_buffer_bytes(self) -> usize {
        self.max_buffer_bytes
    }
}

/// Exact verified result and peak encoded buffer use of the final canonical fold.
pub type ComposerV1FoldOutcome = ComposerV1Plan;

struct DiscardComposerV1Records;

impl ComposerV1RecordSink for DiscardComposerV1Records {
    type Error = Infallible;

    fn chunk(
        &mut self,
        _chunk: ContentChunkRecord,
        _span: crate::ContentByteSpanRecord,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn text_piece(
        &mut self,
        _span: crate::ContentTextSpanRecord,
        _piece: crate::ContentPieceRecord,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn image_piece(&mut self, _piece: crate::ContentPieceRecord) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Plans one canonical fold with a placeholder id while discarding emitted records.
pub fn plan_composer_v1(
    atom_count: u64,
    mut driver: impl FnMut(
        u64,
        &mut dyn ComposerV1AtomWriter<SinkError = Infallible>,
    ) -> Result<(), ComposerV1FoldError<Infallible>>,
) -> Result<ComposerV1Plan, ComposerV1FoldError<Infallible>> {
    let mut sink = DiscardComposerV1Records;
    let placeholder = SyndicContentId::from_bytes([0; 16]);
    let mut fold = ComposerV1Fold::new(placeholder, atom_count, &mut sink)?;
    drive_indexed_atoms(&mut fold, atom_count, &mut driver)?;
    let completed = fold.finish()?;
    let content_id = SyndicContentId::from_digest(*completed.summary.digest().as_bytes());
    Ok(ComposerV1Plan {
        content_id,
        summary: completed.summary,
        max_buffer_bytes: completed.max_buffer_bytes,
    })
}

/// Emits final canonical records under the planned id and verifies exact pass parity.
///
/// A sink must keep emitted records unpublished until this function returns success, or stage them
/// as unreachable records whose manifest is published only from the returned verified outcome.
pub fn fold_composer_v1<S: ComposerV1RecordSink>(
    plan: ComposerV1Plan,
    sink: &mut S,
    mut driver: impl FnMut(
        u64,
        &mut dyn ComposerV1AtomWriter<SinkError = S::Error>,
    ) -> Result<(), ComposerV1FoldError<S::Error>>,
) -> Result<ComposerV1FoldOutcome, ComposerV1FoldError<S::Error>> {
    let mut fold = ComposerV1Fold::new(plan.content_id, plan.summary.atom_count(), sink)?;
    drive_indexed_atoms(&mut fold, plan.summary.atom_count(), &mut driver)?;
    let completed = fold.finish()?;
    let derived_id = SyndicContentId::from_digest(*completed.summary.digest().as_bytes());
    if completed.summary != plan.summary
        || derived_id != plan.content_id
        || completed.max_buffer_bytes != plan.max_buffer_bytes
    {
        return Err(ComposerV1FoldError::PlanMismatch);
    }
    Ok(ComposerV1Plan {
        content_id: plan.content_id,
        summary: completed.summary,
        max_buffer_bytes: completed.max_buffer_bytes,
    })
}

#[derive(Default)]
struct PreparedComposerRecords {
    chunks: Vec<ContentChunkRecord>,
    text_spans: Vec<crate::ContentTextSpanRecord>,
    pieces: Vec<crate::ContentPieceRecord>,
}

impl ComposerV1RecordSink for PreparedComposerRecords {
    type Error = Infallible;

    fn chunk(
        &mut self,
        chunk: ContentChunkRecord,
        _span: crate::ContentByteSpanRecord,
    ) -> Result<(), Self::Error> {
        self.chunks.push(chunk);
        Ok(())
    }

    fn text_piece(
        &mut self,
        span: crate::ContentTextSpanRecord,
        piece: crate::ContentPieceRecord,
    ) -> Result<(), Self::Error> {
        self.text_spans.push(span);
        self.pieces.push(piece);
        Ok(())
    }

    fn image_piece(&mut self, piece: crate::ContentPieceRecord) -> Result<(), Self::Error> {
        self.pieces.push(piece);
        Ok(())
    }
}

/// Exact prepared bounded chunks for one immutable content object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedContent {
    id: SyndicContentId,
    encoding: ContentEncoding,
    summary: ContentSummary,
    chunks: Vec<ContentChunkRecord>,
    text_spans: Vec<crate::ContentTextSpanRecord>,
    pieces: Vec<crate::ContentPieceRecord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreparedTextRange {
    logical_start: u64,
    logical_end: u64,
    encoded_start: u64,
    encoded_end: u64,
    break_before: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreparedPiece {
    Text(PreparedTextRange),
    // The preexisting UTF-8 range helper still exhaustively names this shape; ComposerV1 never
    // constructs it now that every composer record is produced by `composer_v1`.
    #[allow(dead_code)]
    ImageMarker {
        atom_ordinal: ComposerAtomOrdinal,
        marker_ordinal: InputMarkerOrdinal,
        logical_offset: u64,
        encoded_start: u64,
        encoded_end: u64,
        marker_id: SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
    },
}

impl PreparedPiece {
    const fn encoded_range(&self) -> (u64, u64) {
        match self {
            Self::Text(range) => (range.encoded_start, range.encoded_end),
            Self::ImageMarker {
                encoded_start,
                encoded_end,
                ..
            } => (*encoded_start, *encoded_end),
        }
    }
}

impl PreparedContent {
    /// Encodes one complete logical composer payload without a whole-content ceiling.
    pub fn composer(payload: &ComposerPayload) -> Result<Self, SyndicRecordError> {
        let atom_count = u64::try_from(payload.atoms().len()).map_err(|_| {
            SyndicRecordError::LengthOverflow {
                kind: "composer atom count",
            }
        })?;
        let plan = plan_composer_v1(atom_count, |index, writer| {
            write_payload_atom(payload, index, writer)
        })
        .map_err(composer_record_error)?;
        let mut records = PreparedComposerRecords::default();
        let outcome = fold_composer_v1(plan, &mut records, |index, writer| {
            write_payload_atom(payload, index, writer)
        })
        .map_err(composer_record_error)?;
        debug_assert!(outcome.max_buffer_bytes() <= crate::CONTENT_CHUNK_MAX_BYTES);
        Ok(Self {
            id: outcome.content_id(),
            encoding: ContentEncoding::ComposerV1,
            summary: outcome.summary(),
            chunks: records.chunks,
            text_spans: records.text_spans,
            pieces: records.pieces,
        })
    }

    /// Encodes exact canonical UTF-8 text without a whole-content ceiling.
    pub fn utf8(text: &str) -> Result<Self, SyndicRecordError> {
        if let Some(index) = text.as_bytes().iter().position(|byte| *byte == 0) {
            return Err(SyndicRecordError::NulByte {
                kind: "canonical content",
                index,
            });
        }
        let length = u64::try_from(text.len()).map_err(|_| SyndicRecordError::LengthOverflow {
            kind: "canonical UTF-8 bytes",
        })?;
        let pieces = (length != 0).then_some(PreparedPiece::Text(PreparedTextRange {
            logical_start: 0,
            logical_end: length,
            encoded_start: 0,
            encoded_end: length,
            break_before: false,
        }));
        Self::from_encoded(
            ContentEncoding::Utf8V1,
            text.as_bytes().to_vec(),
            length,
            1,
            0,
            input_marker_digest(std::iter::empty()),
            pieces.into_iter().collect(),
        )
    }

    fn from_encoded(
        encoding: ContentEncoding,
        encoded: Vec<u8>,
        logical_utf8_bytes: u64,
        atom_count: u64,
        image_marker_count: u64,
        marker_digest: [u8; 32],
        prepared_pieces: Vec<PreparedPiece>,
    ) -> Result<Self, SyndicRecordError> {
        let chunk_ranges = encoded_chunk_ranges(&encoded, &prepared_pieces)?;
        let mut chain = content_chain_seed(encoding);
        let mut chunks = Vec::new();
        for (index, &(start, end)) in chunk_ranges.iter().enumerate() {
            let ordinal = ContentChunkOrdinal::new((index as u64) + 1).map_err(|_| {
                SyndicRecordError::LengthOverflow {
                    kind: "content chunks",
                }
            })?;
            let bytes = encoded
                .get(start..end)
                .ok_or(SyndicRecordError::InvalidContentEncoding)?;
            let chunk =
                ContentChunkRecord::new(SyndicContentId::from_bytes([0; 16]), ordinal, bytes)?;
            chain = advance_content_chain(chain, &chunk);
            chunks.push(chunk);
        }
        let id = SyndicContentId::from_digest(*chain.as_bytes());
        for chunk in &mut chunks {
            *chunk = ContentChunkRecord::new(id, chunk.ordinal(), chunk.bytes())?;
        }
        let (text_spans, pieces) =
            prepared_content_pieces(id, &encoded, &chunk_ranges, &prepared_pieces)?;
        let maximum_image_label = prepared_pieces
            .iter()
            .filter_map(|piece| match piece {
                PreparedPiece::ImageMarker { label, .. } => Some(*label),
                PreparedPiece::Text(_) => None,
            })
            .max();
        let summary = ContentSummary::new(
            u64::try_from(chunks.len()).map_err(|_| SyndicRecordError::LengthOverflow {
                kind: "content chunks",
            })?,
            u64::try_from(pieces.len()).map_err(|_| SyndicRecordError::LengthOverflow {
                kind: "content pieces",
            })?,
            u64::try_from(encoded.len()).map_err(|_| SyndicRecordError::LengthOverflow {
                kind: "encoded content bytes",
            })?,
            logical_utf8_bytes,
            atom_count,
            image_marker_count,
            marker_digest,
            maximum_image_label,
            chain,
        )?;
        Ok(Self {
            id,
            encoding,
            summary,
            chunks,
            text_spans,
            pieces,
        })
    }

    #[must_use]
    pub const fn id(&self) -> SyndicContentId {
        self.id
    }
    #[must_use]
    pub const fn encoding(&self) -> ContentEncoding {
        self.encoding
    }
    #[must_use]
    pub const fn summary(&self) -> ContentSummary {
        self.summary
    }
    #[must_use]
    pub fn chunks(&self) -> &[ContentChunkRecord] {
        &self.chunks
    }

    #[must_use]
    pub fn text_spans(&self) -> &[crate::ContentTextSpanRecord] {
        &self.text_spans
    }

    #[must_use]
    pub fn pieces(&self) -> &[crate::ContentPieceRecord] {
        &self.pieces
    }

    pub fn building_manifest(&self) -> ContentManifestRecord {
        ContentManifestRecord::new(
            self.id,
            ContentRevision::new(1).expect("first content revision"),
            self.encoding,
            crate::ContentLifecycle::Building,
            0,
            0,
            content_chain_seed(self.encoding),
            self.summary,
        )
    }

    pub fn sealed_manifest(&self, revision: ContentRevision) -> ContentManifestRecord {
        ContentManifestRecord::new(
            self.id,
            revision,
            self.encoding,
            crate::ContentLifecycle::Sealed,
            self.summary.chunk_count(),
            self.summary.encoded_bytes(),
            self.summary.digest(),
            self.summary,
        )
    }

    pub fn reference(&self, revision: ContentRevision) -> crate::ContentReference {
        crate::ContentReference::new(self.id, revision, self.encoding, self.summary)
    }
}

fn write_payload_atom<E>(
    payload: &ComposerPayload,
    index: u64,
    writer: &mut dyn ComposerV1AtomWriter<SinkError = E>,
) -> Result<(), ComposerV1FoldError<E>> {
    let index = usize::try_from(index).map_err(|_| {
        ComposerV1FoldError::Record(SyndicRecordError::LengthOverflow {
            kind: "composer atom index",
        })
    })?;
    match payload
        .atoms()
        .get(index)
        .ok_or(ComposerV1FoldError::AtomDriverContract)?
    {
        ComposerAtom::Text(text) => {
            let length = u64::try_from(text.len()).map_err(|_| {
                ComposerV1FoldError::Record(SyndicRecordError::LengthOverflow {
                    kind: "composer text",
                })
            })?;
            writer.begin_text(length)?;
            writer.text_fragment(text)?;
            writer.end_text()
        }
        ComposerAtom::ImageMarker(marker) => {
            writer.image_marker(marker.marker_id(), marker.label())
        }
    }
}

fn composer_record_error(error: ComposerV1FoldError<Infallible>) -> SyndicRecordError {
    match error {
        ComposerV1FoldError::Record(source) => source,
        ComposerV1FoldError::Sink(source) => match source {},
        ComposerV1FoldError::AtomDriverContract | ComposerV1FoldError::PlanMismatch => {
            SyndicRecordError::InvalidContentEncoding
        }
    }
}

pub(crate) fn input_marker_digest(
    markers: impl IntoIterator<Item = (SyndicDraftMarkerId, ImageLabelOrdinal)>,
) -> [u8; 32] {
    let mut digest = sequential_marker_digest_seed();
    for (marker_id, label) in markers {
        digest = advance_sequential_marker_digest(digest, marker_id, label);
    }
    digest
}

pub(crate) fn live_item_content_id(item: beryl_model::SyndicItemId) -> SyndicContentId {
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.live-item-content.v1");
    hash.update(item.as_bytes());
    SyndicContentId::from_digest(hash.finalize().into())
}
