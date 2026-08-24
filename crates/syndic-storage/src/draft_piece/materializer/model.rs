use std::{error::Error, fmt};

use beryl_home_store::ReadError;
use beryl_model::{ContentRevision, SyndicContentDigest};

use crate::{
    ContentReference, DraftPieceDigestV1, DraftPiecePrepareErrorV1, DraftPieceRootReferenceV1,
    ImageLabelOrdinal, SyndicReadError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftComposerFormatV1 {
    ComposerV1,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DraftComposerMaterializationOperationIdV1([u8; 16]);

impl DraftComposerMaterializationOperationIdV1 {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftComposerBuildKeyV1 {
    source: DraftPieceRootReferenceV1,
    format: DraftComposerFormatV1,
    operation: DraftComposerMaterializationOperationIdV1,
}

impl DraftComposerBuildKeyV1 {
    #[must_use]
    pub const fn new(
        source: DraftPieceRootReferenceV1,
        format: DraftComposerFormatV1,
        operation: DraftComposerMaterializationOperationIdV1,
    ) -> Self {
        Self {
            source,
            format,
            operation,
        }
    }

    #[must_use]
    pub const fn source(self) -> DraftPieceRootReferenceV1 {
        self.source
    }

    #[must_use]
    pub const fn format(self) -> DraftComposerFormatV1 {
        self.format
    }

    #[must_use]
    pub const fn operation(self) -> DraftComposerMaterializationOperationIdV1 {
        self.operation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftComposerMaterializationKeyV1 {
    source: DraftPieceRootReferenceV1,
    format: DraftComposerFormatV1,
}

impl DraftComposerMaterializationKeyV1 {
    #[must_use]
    pub const fn new(source: DraftPieceRootReferenceV1, format: DraftComposerFormatV1) -> Self {
        Self { source, format }
    }

    #[must_use]
    pub const fn source(self) -> DraftPieceRootReferenceV1 {
        self.source
    }

    #[must_use]
    pub const fn format(self) -> DraftComposerFormatV1 {
        self.format
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftComposerSourceCursorV1 {
    piece_index: u64,
    atom_encoded_offset: u64,
}

impl DraftComposerSourceCursorV1 {
    #[must_use]
    pub const fn new(piece_index: u64, atom_encoded_offset: u64) -> Self {
        Self {
            piece_index,
            atom_encoded_offset,
        }
    }

    #[must_use]
    pub const fn piece_index(self) -> u64 {
        self.piece_index
    }

    #[must_use]
    pub const fn atom_encoded_offset(self) -> u64 {
        self.atom_encoded_offset
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftComposerEncoderStateV1 {
    cursor: DraftComposerSourceCursorV1,
    source_piece_count: u64,
    encoded_bytes: u64,
    logical_utf8_bytes: u64,
    chunk_count: u64,
    piece_count: u64,
    marker_count: u64,
    marker_digest: [u8; 32],
    maximum_image_label: Option<ImageLabelOrdinal>,
    chain_digest: SyndicContentDigest,
    carry: Vec<u8>,
    break_before: bool,
    active_text_span_encoded_start: Option<u64>,
    active_text_span_logical_start: Option<u64>,
}

impl DraftComposerEncoderStateV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        cursor: DraftComposerSourceCursorV1,
        source_piece_count: u64,
        encoded_bytes: u64,
        logical_utf8_bytes: u64,
        chunk_count: u64,
        piece_count: u64,
        marker_count: u64,
        marker_digest: [u8; 32],
        maximum_image_label: Option<ImageLabelOrdinal>,
        chain_digest: SyndicContentDigest,
        carry: Vec<u8>,
        break_before: bool,
        active_text_span_encoded_start: Option<u64>,
        active_text_span_logical_start: Option<u64>,
    ) -> Self {
        Self {
            cursor,
            source_piece_count,
            encoded_bytes,
            logical_utf8_bytes,
            chunk_count,
            piece_count,
            marker_count,
            marker_digest,
            maximum_image_label,
            chain_digest,
            carry,
            break_before,
            active_text_span_encoded_start,
            active_text_span_logical_start,
        }
    }

    #[must_use]
    pub const fn cursor(&self) -> DraftComposerSourceCursorV1 {
        self.cursor
    }

    #[must_use]
    pub const fn source_piece_count(&self) -> u64 {
        self.source_piece_count
    }

    #[must_use]
    pub const fn encoded_bytes(&self) -> u64 {
        self.encoded_bytes
    }

    #[must_use]
    pub const fn logical_utf8_bytes(&self) -> u64 {
        self.logical_utf8_bytes
    }

    #[must_use]
    pub const fn chunk_count(&self) -> u64 {
        self.chunk_count
    }

    #[must_use]
    pub const fn piece_count(&self) -> u64 {
        self.piece_count
    }

    #[must_use]
    pub const fn marker_count(&self) -> u64 {
        self.marker_count
    }

    #[must_use]
    pub const fn marker_digest(&self) -> [u8; 32] {
        self.marker_digest
    }

    #[must_use]
    pub const fn maximum_image_label(&self) -> Option<ImageLabelOrdinal> {
        self.maximum_image_label
    }

    #[must_use]
    pub const fn chain_digest(&self) -> SyndicContentDigest {
        self.chain_digest
    }

    #[must_use]
    pub fn carry(&self) -> &[u8] {
        &self.carry
    }

    #[must_use]
    pub const fn break_before(&self) -> bool {
        self.break_before
    }

    #[must_use]
    pub const fn active_text_span_encoded_start(&self) -> Option<u64> {
        self.active_text_span_encoded_start
    }

    #[must_use]
    pub const fn active_text_span_logical_start(&self) -> Option<u64> {
        self.active_text_span_logical_start
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftComposerRecordFrontierV1 {
    cursor: DraftComposerSourceCursorV1,
    encoded_bytes: u64,
    logical_utf8_bytes: u64,
    piece_count: u64,
    marker_count: u64,
    marker_digest: [u8; 32],
    maximum_image_label: Option<ImageLabelOrdinal>,
    chunk_start: u64,
    chunk_ordinal: u64,
    break_before: bool,
}

impl DraftComposerRecordFrontierV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        cursor: DraftComposerSourceCursorV1,
        encoded_bytes: u64,
        logical_utf8_bytes: u64,
        piece_count: u64,
        marker_count: u64,
        marker_digest: [u8; 32],
        maximum_image_label: Option<ImageLabelOrdinal>,
        chunk_start: u64,
        chunk_ordinal: u64,
        break_before: bool,
    ) -> Self {
        Self {
            cursor,
            encoded_bytes,
            logical_utf8_bytes,
            piece_count,
            marker_count,
            marker_digest,
            maximum_image_label,
            chunk_start,
            chunk_ordinal,
            break_before,
        }
    }

    #[must_use]
    pub const fn cursor(self) -> DraftComposerSourceCursorV1 {
        self.cursor
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
    pub const fn piece_count(self) -> u64 {
        self.piece_count
    }

    #[must_use]
    pub const fn marker_count(self) -> u64 {
        self.marker_count
    }

    #[must_use]
    pub const fn marker_digest(self) -> [u8; 32] {
        self.marker_digest
    }

    #[must_use]
    pub const fn maximum_image_label(self) -> Option<ImageLabelOrdinal> {
        self.maximum_image_label
    }

    #[must_use]
    pub const fn chunk_start(self) -> u64 {
        self.chunk_start
    }

    #[must_use]
    pub const fn chunk_ordinal(self) -> u64 {
        self.chunk_ordinal
    }

    #[must_use]
    pub const fn break_before(self) -> bool {
        self.break_before
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftComposerBuildPhaseV1 {
    Planning,
    Writing,
    Draining { final_chunk: bool },
    ReadyToSeal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftComposerFailureReasonV1 {
    Operational,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftComposerBuildLifecycleV1 {
    Open(DraftComposerBuildPhaseV1),
    Cancelled,
    Failed(DraftComposerFailureReasonV1),
    Superseded(DraftComposerMaterializationOperationIdV1),
    Sealed(ContentReference),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DraftComposerBuildRecordV1 {
    key: DraftComposerBuildKeyV1,
    encoder: DraftComposerEncoderStateV1,
    records: DraftComposerRecordFrontierV1,
    output: Option<ContentReference>,
    output_revision: Option<ContentRevision>,
    output_chunk_count: u64,
    output_encoded_bytes: u64,
    output_chain_digest: SyndicContentDigest,
    lifecycle: DraftComposerBuildLifecycleV1,
}

impl DraftComposerBuildRecordV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        key: DraftComposerBuildKeyV1,
        encoder: DraftComposerEncoderStateV1,
        records: DraftComposerRecordFrontierV1,
        output: Option<ContentReference>,
        output_revision: Option<ContentRevision>,
        output_chunk_count: u64,
        output_encoded_bytes: u64,
        output_chain_digest: SyndicContentDigest,
        lifecycle: DraftComposerBuildLifecycleV1,
    ) -> Self {
        Self {
            key,
            encoder,
            records,
            output,
            output_revision,
            output_chunk_count,
            output_encoded_bytes,
            output_chain_digest,
            lifecycle,
        }
    }

    #[must_use]
    pub const fn key(&self) -> DraftComposerBuildKeyV1 {
        self.key
    }

    #[must_use]
    pub const fn encoder(&self) -> &DraftComposerEncoderStateV1 {
        &self.encoder
    }

    #[must_use]
    pub const fn records(&self) -> DraftComposerRecordFrontierV1 {
        self.records
    }

    #[must_use]
    pub(crate) const fn output(&self) -> Option<ContentReference> {
        self.output
    }

    #[must_use]
    pub const fn output_revision(&self) -> Option<ContentRevision> {
        self.output_revision
    }

    #[must_use]
    pub const fn output_chunk_count(&self) -> u64 {
        self.output_chunk_count
    }

    #[must_use]
    pub const fn output_encoded_bytes(&self) -> u64 {
        self.output_encoded_bytes
    }

    #[must_use]
    pub const fn output_chain_digest(&self) -> SyndicContentDigest {
        self.output_chain_digest
    }

    #[must_use]
    pub const fn lifecycle(&self) -> &DraftComposerBuildLifecycleV1 {
        &self.lifecycle
    }

    pub(crate) fn local_shape_error(&self) -> Option<&'static str> {
        let encoder = self.encoder();
        let records = self.records();
        let source = self.key().source().summary();
        let encoder_cursor = encoder.cursor();
        let record_cursor = records.cursor();
        let encoder_at_eof = encoder_cursor.piece_index() == source.piece_count()
            && encoder_cursor.atom_encoded_offset() == 0;
        let record_at_eof = record_cursor.piece_index() == source.piece_count()
            && record_cursor.atom_encoded_offset() == 0;
        let cursor_ordered = record_cursor.piece_index() < encoder_cursor.piece_index()
            || (record_cursor.piece_index() == encoder_cursor.piece_index()
                && record_cursor.atom_encoded_offset() <= encoder_cursor.atom_encoded_offset());
        let marker_shape = |count, digest, maximum: Option<ImageLabelOrdinal>| {
            if count == 0 {
                digest == beryl_model::sequential_marker_digest_seed() && maximum.is_none()
            } else {
                maximum.is_some()
            }
        };
        let active_text_shape = match (
            encoder.active_text_span_encoded_start(),
            encoder.active_text_span_logical_start(),
        ) {
            (Some(encoded_start), Some(logical_start)) => {
                encoded_start <= encoder.encoded_bytes()
                    && logical_start <= encoder.logical_utf8_bytes()
                    && logical_start < encoder.logical_utf8_bytes()
                    && encoder.source_piece_count() < source.piece_count()
                    && encoder_cursor.atom_encoded_offset() > 9
                    && encoder.encoded_bytes() - encoded_start
                        == encoder.logical_utf8_bytes() - logical_start
                    && !encoder.break_before()
            }
            (None, None) => true,
            _ => false,
        };
        if encoder.carry().len() > DRAFT_COMPOSER_CARRY_MAX_BYTES
            || encoder.encoded_bytes() < 9
            || records.encoded_bytes() < 9
            || encoder_cursor.piece_index() > source.piece_count()
            || (encoder_cursor.piece_index() == source.piece_count()
                && encoder_cursor.atom_encoded_offset() != 0)
            || encoder.source_piece_count() > source.piece_count()
            || encoder.source_piece_count() != encoder_cursor.piece_index()
            || record_cursor.piece_index() > source.piece_count()
            || (record_cursor.piece_index() == source.piece_count()
                && record_cursor.atom_encoded_offset() != 0)
            || !cursor_ordered
            || encoder.logical_utf8_bytes() > source.logical_utf8_bytes()
            || records.logical_utf8_bytes() > encoder.logical_utf8_bytes()
            || encoder.marker_count() > source.marker_count()
            || encoder.marker_count() > encoder.source_piece_count()
            || records.marker_count() > encoder.marker_count()
            || encoder.marker_count() > encoder.piece_count()
            || records.marker_count() > records.piece_count()
            || records.piece_count() > encoder.piece_count()
            || !marker_shape(
                encoder.marker_count(),
                encoder.marker_digest(),
                encoder.maximum_image_label(),
            )
            || !marker_shape(
                records.marker_count(),
                records.marker_digest(),
                records.maximum_image_label(),
            )
            || records.maximum_image_label() > encoder.maximum_image_label()
            || (records.marker_count() == encoder.marker_count()
                && (records.marker_digest() != encoder.marker_digest()
                    || records.maximum_image_label() != encoder.maximum_image_label()))
            || !active_text_shape
            || records.encoded_bytes() > encoder.encoded_bytes()
            || records.chunk_start() > self.output_encoded_bytes()
            || records.chunk_ordinal() == 0
        {
            return Some("draft composer build frontier closure");
        }
        match (self.output(), self.output_revision()) {
            (Some(output), Some(revision)) => {
                if output.revision() != revision
                    || output.encoding() != crate::ContentEncoding::ComposerV1
                    || output.summary().atom_count() != source.piece_count()
                    || output.summary().logical_utf8_bytes() != source.logical_utf8_bytes()
                    || output.summary().image_marker_count() != source.marker_count()
                    || encoder.chunk_count() != self.output_chunk_count()
                    || encoder.encoded_bytes() < self.output_encoded_bytes()
                    || encoder.chain_digest() != self.output_chain_digest()
                    || self.output_chunk_count() > output.summary().chunk_count()
                    || self.output_encoded_bytes() > output.summary().encoded_bytes()
                    || encoder.logical_utf8_bytes() > output.summary().logical_utf8_bytes()
                    || encoder.piece_count() > output.summary().piece_count()
                    || encoder.marker_count() > output.summary().image_marker_count()
                    || records.logical_utf8_bytes() > output.summary().logical_utf8_bytes()
                    || records.piece_count() > output.summary().piece_count()
                    || records.marker_count() > output.summary().image_marker_count()
                    || !marker_shape(
                        output.summary().image_marker_count(),
                        output.summary().marker_digest(),
                        output.summary().maximum_image_label(),
                    )
                    || encoder.maximum_image_label() > output.summary().maximum_image_label()
                    || records.maximum_image_label() > output.summary().maximum_image_label()
                    || (encoder.marker_count() == output.summary().image_marker_count()
                        && (encoder.marker_digest() != output.summary().marker_digest()
                            || encoder.maximum_image_label()
                                != output.summary().maximum_image_label()))
                    || records.chunk_ordinal() > self.output_chunk_count().saturating_add(1)
                {
                    return Some("draft composer build output closure");
                }
            }
            (None, None) => {
                if self.output_chunk_count() != 0
                    || self.output_encoded_bytes() != 0
                    || self.output_chain_digest()
                        != crate::content_chain_seed(crate::ContentEncoding::ComposerV1)
                    || records.chunk_start() != 0
                    || records.chunk_ordinal() != 1
                {
                    return Some("draft composer build absent-output closure");
                }
            }
            _ => return Some("draft composer build output option closure"),
        }
        let records_are_empty = record_cursor == DraftComposerSourceCursorV1::new(0, 0)
            && records.encoded_bytes() == 9
            && records.logical_utf8_bytes() == 0
            && records.piece_count() == 0
            && records.marker_count() == 0
            && records.marker_digest() == beryl_model::sequential_marker_digest_seed()
            && records.maximum_image_label().is_none()
            && records.chunk_start() == 0
            && records.chunk_ordinal() == 1
            && !records.break_before();
        let mut initial_bytes = [0_u8; 9];
        initial_bytes[0] = 1;
        initial_bytes[1..].copy_from_slice(&source.piece_count().to_be_bytes());
        let initial_encoder = encoder_cursor == DraftComposerSourceCursorV1::new(0, 0)
            && encoder.source_piece_count() == 0
            && encoder.encoded_bytes() == 9
            && encoder.logical_utf8_bytes() == 0
            && encoder.chunk_count() == 0
            && encoder.piece_count() == 0
            && encoder.marker_count() == 0
            && encoder.marker_digest() == beryl_model::sequential_marker_digest_seed()
            && encoder.maximum_image_label().is_none()
            && encoder.chain_digest()
                == crate::content_chain_seed(crate::ContentEncoding::ComposerV1)
            && encoder.carry() == initial_bytes
            && !encoder.break_before()
            && encoder.active_text_span_encoded_start().is_none();
        let flushed_bytes = encoder
            .encoded_bytes()
            .checked_sub(encoder.carry().len() as u64);
        let maximum_flushed_bytes = encoder
            .chunk_count()
            .checked_mul(DRAFT_COMPOSER_CARRY_MAX_BYTES as u64);
        let minimum_encoded_bytes = encoder
            .marker_count()
            .checked_mul(25)
            .and_then(|markers| markers.checked_add(encoder.logical_utf8_bytes()))
            .and_then(|payload| payload.checked_add(9));
        let maximum_encoder_pieces = encoder
            .logical_utf8_bytes()
            .checked_add(encoder.marker_count());
        let maximum_record_pieces = records
            .logical_utf8_bytes()
            .checked_add(records.marker_count());
        let planning_prefix = maximum_encoder_pieces
            .is_some_and(|maximum| encoder.piece_count() <= maximum)
            && maximum_record_pieces.is_some_and(|maximum| records.piece_count() <= maximum)
            && encoder.chunk_count() <= encoder.piece_count()
            && minimum_encoded_bytes.is_some_and(|minimum| encoder.encoded_bytes() >= minimum)
            && match (encoder.chunk_count(), flushed_bytes, maximum_flushed_bytes) {
                (0, Some(0), Some(0)) => {
                    encoder.chain_digest()
                        == crate::content_chain_seed(crate::ContentEncoding::ComposerV1)
                        && !encoder.carry().is_empty()
                }
                (count, Some(flushed), Some(maximum)) if count != 0 => {
                    flushed >= count
                        && flushed <= maximum
                        && encoder.chain_digest()
                            != crate::content_chain_seed(crate::ContentEncoding::ComposerV1)
                }
                _ => false,
            };
        let planning = self.output().is_none()
            && records_are_empty
            && planning_prefix
            && (!encoder_at_eof || initial_encoder)
            && (encoder_cursor != DraftComposerSourceCursorV1::new(0, 0) || initial_encoder);
        let writing = self.output().is_some()
            && encoder
                .encoded_bytes()
                .checked_sub(self.output_encoded_bytes())
                == Some(encoder.carry().len() as u64)
            && encoder
                .active_text_span_encoded_start()
                .is_none_or(|start| start >= self.output_encoded_bytes())
            && records.encoded_bytes() == self.output_encoded_bytes().max(9)
            && records.chunk_start() == self.output_encoded_bytes()
            && records.chunk_ordinal() == self.output_chunk_count().saturating_add(1)
            && (encoder_cursor != DraftComposerSourceCursorV1::new(0, 0) || initial_encoder);
        let draining = |final_chunk: bool| {
            self.output().is_some()
                && encoder.carry().is_empty()
                && encoder.active_text_span_encoded_start().is_none()
                && encoder.encoded_bytes() == self.output_encoded_bytes()
                && records.encoded_bytes() <= self.output_encoded_bytes()
                && (records.encoded_bytes() < self.output_encoded_bytes() || record_at_eof)
                && records.chunk_start() < self.output_encoded_bytes()
                && records.chunk_ordinal() == self.output_chunk_count()
                && final_chunk == encoder_at_eof
        };
        let ready = self.output().is_some()
            && encoder_at_eof
            && record_at_eof
            && encoder.carry().is_empty()
            && encoder.active_text_span_encoded_start().is_none()
            && encoder.encoded_bytes() == self.output_encoded_bytes()
            && records.encoded_bytes() == self.output_encoded_bytes()
            && records.chunk_start() < self.output_encoded_bytes()
            && records.chunk_ordinal() == self.output_chunk_count();
        let phase_valid = match self.lifecycle() {
            DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Planning) => planning,
            DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Writing) => writing,
            DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Draining {
                final_chunk,
            }) => draining(*final_chunk),
            DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::ReadyToSeal) => ready,
            DraftComposerBuildLifecycleV1::Sealed(reference) => {
                self.output() == Some(*reference) && ready
            }
            DraftComposerBuildLifecycleV1::Cancelled
            | DraftComposerBuildLifecycleV1::Failed(_)
            | DraftComposerBuildLifecycleV1::Superseded(_) => {
                planning || writing || draining(false) || draining(true) || ready
            }
        };
        if phase_valid {
            None
        } else {
            Some("draft composer build phase closure")
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftComposerMaterializationRecordV1 {
    key: DraftComposerMaterializationKeyV1,
    sealing_operation: DraftComposerMaterializationOperationIdV1,
    source_digest: DraftPieceDigestV1,
    source_piece_count: u64,
    source_utf8_bytes: u64,
    source_marker_count: u64,
    content: ContentReference,
}

impl DraftComposerMaterializationRecordV1 {
    #[must_use]
    pub const fn new(
        key: DraftComposerMaterializationKeyV1,
        sealing_operation: DraftComposerMaterializationOperationIdV1,
        content: ContentReference,
    ) -> Self {
        let source = key.source();
        Self {
            key,
            sealing_operation,
            source_digest: source.combined_digest(),
            source_piece_count: source.summary().piece_count(),
            source_utf8_bytes: source.summary().logical_utf8_bytes(),
            source_marker_count: source.summary().marker_count(),
            content,
        }
    }

    #[must_use]
    pub const fn key(self) -> DraftComposerMaterializationKeyV1 {
        self.key
    }

    #[must_use]
    pub const fn sealing_operation(self) -> DraftComposerMaterializationOperationIdV1 {
        self.sealing_operation
    }

    #[must_use]
    pub const fn source_digest(self) -> DraftPieceDigestV1 {
        self.source_digest
    }

    #[must_use]
    pub const fn source_piece_count(self) -> u64 {
        self.source_piece_count
    }

    #[must_use]
    pub const fn source_utf8_bytes(self) -> u64 {
        self.source_utf8_bytes
    }

    #[must_use]
    pub const fn source_marker_count(self) -> u64 {
        self.source_marker_count
    }

    #[must_use]
    pub const fn content(self) -> ContentReference {
        self.content
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftComposerMaterializationStatusV1 {
    Absent,
    Building(DraftComposerBuildPhaseV1),
    Cancelled,
    Failed(DraftComposerFailureReasonV1),
    Superseded(DraftComposerMaterializationOperationIdV1),
    Sealed(DraftComposerMaterializationRecordV1),
}

#[derive(Debug)]
pub enum DraftComposerMaterializationErrorV1 {
    Read(ReadError),
    SyndicRead(SyndicReadError),
    Source(DraftPiecePrepareErrorV1),
    MissingBuild,
    BuildCollision,
    MappingCollision,
    InvalidBuild,
    InvalidOutput,
    LengthOverflow,
}

impl fmt::Display for DraftComposerMaterializationErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(formatter),
            Self::SyndicRead(error) => error.fmt(formatter),
            Self::Source(error) => error.fmt(formatter),
            Self::MissingBuild => formatter.write_str("draft composer build is missing"),
            Self::BuildCollision => formatter.write_str("draft composer build identity collided"),
            Self::MappingCollision => {
                formatter.write_str("draft composer materialization mapping collided")
            }
            Self::InvalidBuild => formatter.write_str("draft composer build is invalid"),
            Self::InvalidOutput => formatter.write_str("draft composer output is invalid"),
            Self::LengthOverflow => formatter.write_str("draft composer frontier overflowed"),
        }
    }
}

impl Error for DraftComposerMaterializationErrorV1 {}

impl From<ReadError> for DraftComposerMaterializationErrorV1 {
    fn from(value: ReadError) -> Self {
        Self::Read(value)
    }
}

impl From<SyndicReadError> for DraftComposerMaterializationErrorV1 {
    fn from(value: SyndicReadError) -> Self {
        Self::SyndicRead(value)
    }
}

impl From<DraftPiecePrepareErrorV1> for DraftComposerMaterializationErrorV1 {
    fn from(value: DraftPiecePrepareErrorV1) -> Self {
        Self::Source(value)
    }
}

pub const DRAFT_COMPOSER_INPUT_MAX_RECORDS: usize = 256;
pub const DRAFT_COMPOSER_READ_MAX_RECORDS: u64 = 1_024;
pub const DRAFT_COMPOSER_INPUT_MAX_BYTES: usize = 65_536;
pub const DRAFT_COMPOSER_CARRY_MAX_BYTES: usize = 64_512;
pub const DRAFT_COMPOSER_WRITE_MAX_RECORDS: usize = 5;
pub const DRAFT_COMPOSER_RESIDENT_MAX_BYTES: usize = 196_608;
