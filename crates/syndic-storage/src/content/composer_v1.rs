use sha2::{Digest, Sha256};

use crate::{
    ComposerAtomOrdinal, ContentByteSpanRecord, ContentChunkOrdinal, ContentChunkRecord,
    ContentEncoding, ContentPieceOrdinal, ContentPieceRecord, ContentSummary,
    ContentTextSpanRecord, ImageLabelOrdinal, InputMarkerOrdinal, SyndicRecordError,
    advance_content_chain, content_chain_seed,
};

/// Receives canonical ComposerV1 records as one bounded fold produces them.
pub trait ComposerV1RecordSink {
    type Error;

    fn chunk(
        &mut self,
        chunk: ContentChunkRecord,
        span: ContentByteSpanRecord,
    ) -> Result<(), Self::Error>;

    fn text_piece(
        &mut self,
        span: ContentTextSpanRecord,
        piece: ContentPieceRecord,
    ) -> Result<(), Self::Error>;

    fn image_piece(&mut self, piece: ContentPieceRecord) -> Result<(), Self::Error>;
}

/// Writes exactly one indexed atom into a canonical ComposerV1 fold.
pub trait ComposerV1AtomWriter {
    type SinkError;

    fn begin_text(&mut self, utf8_bytes: u64) -> Result<(), ComposerV1FoldError<Self::SinkError>>;

    fn text_fragment(&mut self, text: &str) -> Result<(), ComposerV1FoldError<Self::SinkError>>;

    fn end_text(&mut self) -> Result<(), ComposerV1FoldError<Self::SinkError>>;

    fn image_marker(
        &mut self,
        marker_id: beryl_model::SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
    ) -> Result<(), ComposerV1FoldError<Self::SinkError>>;
}

/// Failure from canonical record construction or the selected record sink.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposerV1FoldError<E> {
    Record(SyndicRecordError),
    Sink(E),
    AtomDriverContract,
    PlanMismatch,
}

pub(super) struct CompletedFold {
    pub(super) summary: ContentSummary,
    pub(super) max_buffer_bytes: usize,
}

struct ActiveText {
    declared_bytes: u64,
    consumed_bytes: u64,
}

struct ActiveTextSpan {
    logical_start: u64,
    encoded_start: u64,
    break_before: bool,
    digest: Sha256,
}

/// Shared production canonical ComposerV1 serializer and record fold.
pub(super) struct ComposerV1Fold<'a, S: ComposerV1RecordSink> {
    content_id: beryl_model::SyndicContentId,
    sink: &'a mut S,
    declared_atoms: u64,
    atom_count: u64,
    marker_count: u64,
    piece_count: u64,
    chunk_count: u64,
    encoded_bytes: u64,
    logical_utf8_bytes: u64,
    marker_digest: [u8; 32],
    maximum_image_label: Option<ImageLabelOrdinal>,
    chain: beryl_model::SyndicContentDigest,
    buffer: Vec<u8>,
    chunk_start: u64,
    max_buffer_bytes: usize,
    break_before: bool,
    active_text: Option<ActiveText>,
    active_span: Option<ActiveTextSpan>,
}

impl<'a, S: ComposerV1RecordSink> ComposerV1Fold<'a, S> {
    pub(super) fn new(
        content_id: beryl_model::SyndicContentId,
        atom_count: u64,
        sink: &'a mut S,
    ) -> Result<Self, ComposerV1FoldError<S::Error>> {
        let mut fold = Self {
            content_id,
            sink,
            declared_atoms: atom_count,
            atom_count: 0,
            marker_count: 0,
            piece_count: 0,
            chunk_count: 0,
            encoded_bytes: 0,
            logical_utf8_bytes: 0,
            marker_digest: beryl_model::sequential_marker_digest_seed(),
            maximum_image_label: None,
            chain: content_chain_seed(ContentEncoding::ComposerV1),
            buffer: Vec::with_capacity(crate::CONTENT_CHUNK_MAX_BYTES),
            chunk_start: 0,
            max_buffer_bytes: 0,
            break_before: false,
            active_text: None,
            active_span: None,
        };
        fold.write_raw(&[1])?;
        fold.write_raw(&atom_count.to_be_bytes())?;
        Ok(fold)
    }

    fn begin_text(&mut self, utf8_bytes: u64) -> Result<(), ComposerV1FoldError<S::Error>> {
        self.require_no_active_text()?;
        self.advance_atom()?;
        self.write_raw(&[0])?;
        self.write_raw(&utf8_bytes.to_be_bytes())?;
        self.active_text = Some(ActiveText {
            declared_bytes: utf8_bytes,
            consumed_bytes: 0,
        });
        Ok(())
    }

    fn text_fragment(&mut self, text: &str) -> Result<(), ComposerV1FoldError<S::Error>> {
        if self.active_text.is_none() {
            return Err(Self::invalid());
        }
        if let Some(index) = text.as_bytes().iter().position(|byte| *byte == 0) {
            return Err(ComposerV1FoldError::Record(SyndicRecordError::NulByte {
                kind: "composer text atom",
                index,
            }));
        }
        let fragment_bytes = u64::try_from(text.len()).map_err(|_| {
            ComposerV1FoldError::Record(SyndicRecordError::LengthOverflow {
                kind: "composer text",
            })
        })?;
        let active = self.active_text.as_ref().expect("checked above");
        if active
            .consumed_bytes
            .checked_add(fragment_bytes)
            .is_none_or(|next| next > active.declared_bytes)
        {
            return Err(Self::invalid());
        }

        let mut remaining = text;
        while !remaining.is_empty() {
            self.ensure_buffer_capacity();
            let available = crate::CONTENT_CHUNK_MAX_BYTES - self.buffer.len();
            let mut take = available.min(remaining.len());
            while take != 0 && !remaining.is_char_boundary(take) {
                take -= 1;
            }
            if take == 0 {
                self.finalize_text_span()?;
                self.flush_chunk()?;
                continue;
            }
            self.start_text_span();
            let (part, rest) = remaining.split_at(take);
            self.buffer.extend_from_slice(part.as_bytes());
            self.max_buffer_bytes = self.max_buffer_bytes.max(self.buffer.len());
            self.encoded_bytes = self
                .encoded_bytes
                .checked_add(take as u64)
                .ok_or_else(Self::length_overflow)?;
            self.logical_utf8_bytes = self
                .logical_utf8_bytes
                .checked_add(take as u64)
                .ok_or_else(Self::length_overflow)?;
            let active = self.active_text.as_mut().expect("checked above");
            active.consumed_bytes = active
                .consumed_bytes
                .checked_add(take as u64)
                .ok_or_else(Self::length_overflow)?;
            self.active_span
                .as_mut()
                .expect("started above")
                .digest
                .update(part.as_bytes());
            remaining = rest;
            if self.buffer.len() == crate::CONTENT_CHUNK_MAX_BYTES {
                self.finalize_text_span()?;
                self.flush_chunk()?;
            }
        }
        Ok(())
    }

    fn end_text(&mut self) -> Result<(), ComposerV1FoldError<S::Error>> {
        let Some(active) = self.active_text.take() else {
            return Err(Self::invalid());
        };
        if active.consumed_bytes != active.declared_bytes {
            return Err(Self::invalid());
        }
        self.finalize_text_span()
    }

    fn image_marker(
        &mut self,
        marker_id: beryl_model::SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
    ) -> Result<(), ComposerV1FoldError<S::Error>> {
        self.require_no_active_text()?;
        let atom_ordinal = self.advance_atom()?;
        let encoded_start = self.encoded_bytes;
        let mut encoded = [0_u8; 25];
        encoded[0] = 1;
        encoded[1..17].copy_from_slice(marker_id.as_bytes());
        encoded[17..].copy_from_slice(&label.get().to_be_bytes());
        self.write_raw(&encoded)?;
        let encoded_end = self.encoded_bytes;

        let marker_count = self
            .marker_count
            .checked_add(1)
            .ok_or_else(Self::length_overflow)?;
        let marker_ordinal =
            InputMarkerOrdinal::new(marker_count).map_err(|_| Self::length_overflow())?;
        let piece_count = self
            .piece_count
            .checked_add(1)
            .ok_or_else(Self::length_overflow)?;
        let piece_ordinal =
            ContentPieceOrdinal::new(piece_count).map_err(|_| Self::length_overflow())?;
        let piece = ContentPieceRecord::image_marker(
            self.content_id,
            piece_ordinal,
            atom_ordinal,
            marker_ordinal,
            self.logical_utf8_bytes,
            encoded_start,
            encoded_end,
            marker_id,
            label,
            Sha256::digest(encoded).into(),
        )
        .map_err(ComposerV1FoldError::Record)?;

        self.marker_count = marker_count;
        self.piece_count = piece_count;
        self.marker_digest =
            beryl_model::advance_sequential_marker_digest(self.marker_digest, marker_id, label);
        self.maximum_image_label = Some(
            self.maximum_image_label
                .map_or(label, |current| current.max(label)),
        );
        self.break_before = true;
        self.sink
            .image_piece(piece)
            .map_err(ComposerV1FoldError::Sink)
    }

    pub(super) fn finish(mut self) -> Result<CompletedFold, ComposerV1FoldError<S::Error>> {
        self.require_no_active_text()?;
        if self.atom_count != self.declared_atoms || self.active_span.is_some() {
            return Err(ComposerV1FoldError::AtomDriverContract);
        }
        self.flush_chunk()?;
        let summary = ContentSummary::new(
            self.chunk_count,
            self.piece_count,
            self.encoded_bytes,
            self.logical_utf8_bytes,
            self.atom_count,
            self.marker_count,
            self.marker_digest,
            self.maximum_image_label,
            self.chain,
        )
        .map_err(ComposerV1FoldError::Record)?;
        Ok(CompletedFold {
            summary,
            max_buffer_bytes: self.max_buffer_bytes,
        })
    }

    fn require_no_active_text(&self) -> Result<(), ComposerV1FoldError<S::Error>> {
        if self.active_text.is_some() || self.active_span.is_some() {
            return Err(ComposerV1FoldError::AtomDriverContract);
        }
        Ok(())
    }

    fn advance_atom(&mut self) -> Result<ComposerAtomOrdinal, ComposerV1FoldError<S::Error>> {
        if self.atom_count == self.declared_atoms {
            return Err(ComposerV1FoldError::AtomDriverContract);
        }
        let atom_count = self
            .atom_count
            .checked_add(1)
            .ok_or_else(Self::length_overflow)?;
        let ordinal = ComposerAtomOrdinal::new(atom_count).map_err(|_| Self::length_overflow())?;
        self.atom_count = atom_count;
        Ok(ordinal)
    }

    fn start_text_span(&mut self) {
        if self.active_span.is_none() {
            self.active_span = Some(ActiveTextSpan {
                logical_start: self.logical_utf8_bytes,
                encoded_start: self.encoded_bytes,
                break_before: std::mem::take(&mut self.break_before),
                digest: Sha256::new(),
            });
        }
    }

    fn finalize_text_span(&mut self) -> Result<(), ComposerV1FoldError<S::Error>> {
        let Some(active) = self.active_span.take() else {
            return Ok(());
        };
        let piece_count = self
            .piece_count
            .checked_add(1)
            .ok_or_else(Self::length_overflow)?;
        let piece_ordinal =
            ContentPieceOrdinal::new(piece_count).map_err(|_| Self::length_overflow())?;
        let chunk_count = self
            .chunk_count
            .checked_add(1)
            .ok_or_else(Self::length_overflow)?;
        let chunk_ordinal =
            ContentChunkOrdinal::new(chunk_count).map_err(|_| Self::length_overflow())?;
        let span = ContentTextSpanRecord::new(
            self.content_id,
            piece_ordinal,
            chunk_ordinal,
            self.chunk_start,
            active.logical_start,
            self.logical_utf8_bytes,
            active.encoded_start,
            self.encoded_bytes,
            active.break_before,
            active.digest.finalize().into(),
        )
        .map_err(ComposerV1FoldError::Record)?;
        let piece = ContentPieceRecord::text(span);
        self.piece_count = piece_count;
        self.sink
            .text_piece(span, piece)
            .map_err(ComposerV1FoldError::Sink)
    }

    fn write_raw(&mut self, mut bytes: &[u8]) -> Result<(), ComposerV1FoldError<S::Error>> {
        while !bytes.is_empty() {
            self.ensure_buffer_capacity();
            let available = crate::CONTENT_CHUNK_MAX_BYTES - self.buffer.len();
            let take = available.min(bytes.len());
            let take_bytes = u64::try_from(take).map_err(|_| Self::length_overflow())?;
            let encoded_bytes = self
                .encoded_bytes
                .checked_add(take_bytes)
                .ok_or_else(Self::length_overflow)?;
            self.buffer.extend_from_slice(&bytes[..take]);
            self.encoded_bytes = encoded_bytes;
            self.max_buffer_bytes = self.max_buffer_bytes.max(self.buffer.len());
            bytes = &bytes[take..];
            if self.buffer.len() == crate::CONTENT_CHUNK_MAX_BYTES {
                self.flush_chunk()?;
            }
        }
        Ok(())
    }

    fn ensure_buffer_capacity(&mut self) {
        if self.buffer.capacity() < crate::CONTENT_CHUNK_MAX_BYTES {
            self.buffer
                .reserve_exact(crate::CONTENT_CHUNK_MAX_BYTES - self.buffer.len());
        }
    }

    fn flush_chunk(&mut self) -> Result<(), ComposerV1FoldError<S::Error>> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let chunk_count = self
            .chunk_count
            .checked_add(1)
            .ok_or_else(Self::length_overflow)?;
        let ordinal = ContentChunkOrdinal::new(chunk_count).map_err(|_| Self::length_overflow())?;
        let bytes = std::mem::take(&mut self.buffer);
        let chunk = ContentChunkRecord::new(self.content_id, ordinal, bytes)
            .map_err(ComposerV1FoldError::Record)?;
        let span = ContentByteSpanRecord::for_chunk(&chunk, self.chunk_start)
            .map_err(ComposerV1FoldError::Record)?;
        let chain = advance_content_chain(self.chain, &chunk);
        self.chunk_count = chunk_count;
        self.chunk_start = span.end();
        self.chain = chain;
        self.sink
            .chunk(chunk, span)
            .map_err(ComposerV1FoldError::Sink)
    }

    fn invalid() -> ComposerV1FoldError<S::Error> {
        ComposerV1FoldError::AtomDriverContract
    }

    fn length_overflow() -> ComposerV1FoldError<S::Error> {
        ComposerV1FoldError::Record(SyndicRecordError::LengthOverflow {
            kind: "composer V1 fold",
        })
    }
}

impl<S: ComposerV1RecordSink> ComposerV1AtomWriter for ComposerV1Fold<'_, S> {
    type SinkError = S::Error;

    fn begin_text(&mut self, utf8_bytes: u64) -> Result<(), ComposerV1FoldError<Self::SinkError>> {
        Self::begin_text(self, utf8_bytes)
    }

    fn text_fragment(&mut self, text: &str) -> Result<(), ComposerV1FoldError<Self::SinkError>> {
        Self::text_fragment(self, text)
    }

    fn end_text(&mut self) -> Result<(), ComposerV1FoldError<Self::SinkError>> {
        Self::end_text(self)
    }

    fn image_marker(
        &mut self,
        marker_id: beryl_model::SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
    ) -> Result<(), ComposerV1FoldError<Self::SinkError>> {
        Self::image_marker(self, marker_id, label)
    }
}

pub(super) fn drive_indexed_atoms<S: ComposerV1RecordSink>(
    fold: &mut ComposerV1Fold<'_, S>,
    atom_count: u64,
    driver: &mut impl FnMut(
        u64,
        &mut dyn ComposerV1AtomWriter<SinkError = S::Error>,
    ) -> Result<(), ComposerV1FoldError<S::Error>>,
) -> Result<(), ComposerV1FoldError<S::Error>> {
    for index in 0..atom_count {
        if fold.atom_count != index || fold.active_text.is_some() || fold.active_span.is_some() {
            return Err(ComposerV1FoldError::AtomDriverContract);
        }
        driver(index, fold)?;
        let expected = index
            .checked_add(1)
            .ok_or(ComposerV1FoldError::AtomDriverContract)?;
        if fold.atom_count != expected || fold.active_text.is_some() || fold.active_span.is_some() {
            return Err(ComposerV1FoldError::AtomDriverContract);
        }
    }
    Ok(())
}
