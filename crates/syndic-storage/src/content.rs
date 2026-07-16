use beryl_model::{ContentRevision, SyndicContentId, SyndicDraftMarkerId};
use sha2::{Digest, Sha256};

use crate::{
    ComposerAtom, ComposerAtomOrdinal, ComposerPayload, ContentChunkOrdinal, ContentChunkRecord,
    ContentEncoding, ContentManifestRecord, ContentSummary, ImageLabelOrdinal, InputMarkerOrdinal,
    SyndicRecordError, advance_content_chain, content_chain_seed,
};

pub(crate) fn utf8_chunks(text: &str) -> impl Iterator<Item = &[u8]> {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start == text.len() {
            return None;
        }
        let mut end = start
            .saturating_add(crate::CONTENT_CHUNK_MAX_BYTES)
            .min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let bytes = &text.as_bytes()[start..end];
        start = end;
        Some(bytes)
    })
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

/// Incremental bounded-page assembler for one exact sealed composer content object.
pub struct ComposerContentAssembler {
    reference: crate::ContentReference,
    next_ordinal: u64,
    encoded: Vec<u8>,
    chain: beryl_model::SyndicContentDigest,
}

impl ComposerContentAssembler {
    pub fn new(reference: crate::ContentReference) -> Result<Self, SyndicRecordError> {
        if reference.encoding() != ContentEncoding::ComposerV1 {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        let capacity = usize::try_from(reference.summary().encoded_bytes()).map_err(|_| {
            SyndicRecordError::LengthOverflow {
                kind: "composer content",
            }
        })?;
        Ok(Self {
            reference,
            next_ordinal: 1,
            encoded: Vec::with_capacity(capacity),
            chain: content_chain_seed(ContentEncoding::ComposerV1),
        })
    }

    pub fn push(&mut self, chunk: &ContentChunkRecord) -> Result<(), SyndicRecordError> {
        if chunk.content_id() != self.reference.id() || chunk.ordinal().get() != self.next_ordinal {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        self.encoded
            .len()
            .checked_add(chunk.bytes().len())
            .filter(|length| {
                u64::try_from(*length)
                    .ok()
                    .is_some_and(|length| length <= self.reference.summary().encoded_bytes())
            })
            .ok_or(SyndicRecordError::InvalidContentEncoding)?;
        self.encoded.extend_from_slice(chunk.bytes());
        self.chain = advance_content_chain(self.chain, chunk);
        self.next_ordinal =
            self.next_ordinal
                .checked_add(1)
                .ok_or(SyndicRecordError::LengthOverflow {
                    kind: "content chunks",
                })?;
        Ok(())
    }

    pub fn finish(self) -> Result<ComposerPayload, SyndicRecordError> {
        let summary = self.reference.summary();
        if self.next_ordinal.saturating_sub(1) != summary.chunk_count()
            || u64::try_from(self.encoded.len()).ok() != Some(summary.encoded_bytes())
            || self.chain != summary.digest()
        {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        let payload = decode_composer_content(&self.encoded)?;
        if u64::try_from(payload.utf8_bytes()).ok() != Some(summary.logical_utf8_bytes())
            || u64::try_from(payload.atoms().len()).ok() != Some(summary.atom_count())
            || u64::try_from(payload.image_marker_count()).ok()
                != Some(summary.image_marker_count())
            || input_marker_digest(
                payload
                    .atoms()
                    .iter()
                    .filter_map(ComposerAtom::image_marker_value)
                    .map(|marker| (marker.marker_id(), marker.label())),
            ) != summary.marker_digest()
        {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        Ok(payload)
    }
}

impl PreparedContent {
    /// Encodes one complete logical composer payload without a whole-content ceiling.
    pub fn composer(payload: &ComposerPayload) -> Result<Self, SyndicRecordError> {
        let marker_digest = input_marker_digest(
            payload
                .atoms()
                .iter()
                .filter_map(ComposerAtom::image_marker_value)
                .map(|marker| (marker.marker_id(), marker.label())),
        );
        let mut encoded = Vec::new();
        let mut pieces = Vec::new();
        let mut logical_start = 0_u64;
        let mut break_before = false;
        let mut marker_count = 0_u64;
        encoded.push(1);
        put_u64(
            &mut encoded,
            u64::try_from(payload.atoms().len()).map_err(|_| {
                SyndicRecordError::LengthOverflow {
                    kind: "composer atom count",
                }
            })?,
        );
        for (atom_index, atom) in payload.atoms().iter().enumerate() {
            let atom_ordinal = ComposerAtomOrdinal::new(
                u64::try_from(atom_index)
                    .ok()
                    .and_then(|value| value.checked_add(1))
                    .ok_or(SyndicRecordError::LengthOverflow {
                        kind: "composer atom ordinal",
                    })?,
            )
            .map_err(|_| SyndicRecordError::LengthOverflow {
                kind: "composer atom ordinal",
            })?;
            match atom {
                ComposerAtom::Text(text) => {
                    encoded.push(0);
                    put_u64(
                        &mut encoded,
                        u64::try_from(text.len()).map_err(|_| {
                            SyndicRecordError::LengthOverflow {
                                kind: "composer text",
                            }
                        })?,
                    );
                    let encoded_start = u64::try_from(encoded.len()).map_err(|_| {
                        SyndicRecordError::LengthOverflow {
                            kind: "composer text range",
                        }
                    })?;
                    encoded.extend_from_slice(text.as_bytes());
                    let text_bytes = u64::try_from(text.len()).map_err(|_| {
                        SyndicRecordError::LengthOverflow {
                            kind: "composer text range",
                        }
                    })?;
                    let logical_end = logical_start.checked_add(text_bytes).ok_or(
                        SyndicRecordError::LengthOverflow {
                            kind: "composer logical text",
                        },
                    )?;
                    if text_bytes != 0 {
                        let encoded_end = encoded_start.checked_add(text_bytes).ok_or(
                            SyndicRecordError::LengthOverflow {
                                kind: "composer text range",
                            },
                        )?;
                        pieces.push(PreparedPiece::Text(PreparedTextRange {
                            logical_start,
                            logical_end,
                            encoded_start,
                            encoded_end,
                            break_before,
                        }));
                        break_before = false;
                    }
                    logical_start = logical_end;
                }
                ComposerAtom::ImageMarker(marker) => {
                    let encoded_start = u64::try_from(encoded.len()).map_err(|_| {
                        SyndicRecordError::LengthOverflow {
                            kind: "composer marker range",
                        }
                    })?;
                    encoded.push(1);
                    encoded.extend_from_slice(marker.marker_id().as_bytes());
                    put_u64(&mut encoded, marker.label().get());
                    let encoded_end = u64::try_from(encoded.len()).map_err(|_| {
                        SyndicRecordError::LengthOverflow {
                            kind: "composer marker range",
                        }
                    })?;
                    marker_count =
                        marker_count
                            .checked_add(1)
                            .ok_or(SyndicRecordError::LengthOverflow {
                                kind: "composer marker ordinal",
                            })?;
                    let marker_ordinal = InputMarkerOrdinal::new(marker_count).map_err(|_| {
                        SyndicRecordError::LengthOverflow {
                            kind: "composer marker ordinal",
                        }
                    })?;
                    pieces.push(PreparedPiece::ImageMarker {
                        atom_ordinal,
                        marker_ordinal,
                        logical_offset: logical_start,
                        encoded_start,
                        encoded_end,
                        marker_id: marker.marker_id(),
                        label: marker.label(),
                    });
                    break_before = true;
                }
            }
        }
        Self::from_encoded(
            ContentEncoding::ComposerV1,
            encoded,
            u64::try_from(payload.utf8_bytes()).map_err(|_| SyndicRecordError::LengthOverflow {
                kind: "composer UTF-8 bytes",
            })?,
            u64::try_from(payload.atoms().len()).map_err(|_| {
                SyndicRecordError::LengthOverflow {
                    kind: "composer atom count",
                }
            })?,
            u64::try_from(payload.image_marker_count()).map_err(|_| {
                SyndicRecordError::LengthOverflow {
                    kind: "composer marker count",
                }
            })?,
            marker_digest,
            pieces,
        )
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
            chain,
        );
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

pub(crate) fn input_marker_digest(
    markers: impl IntoIterator<Item = (SyndicDraftMarkerId, ImageLabelOrdinal)>,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.input-markers.v1");
    let mut count = 0_u64;
    for (marker_id, label) in markers {
        hash.update(marker_id.as_bytes());
        hash.update(label.get().to_be_bytes());
        count = count
            .checked_add(1)
            .expect("composer marker count is bounded below u64::MAX");
    }
    hash.update(count.to_be_bytes());
    hash.finalize().into()
}

pub(crate) fn live_item_content_id(item: beryl_model::SyndicItemId) -> SyndicContentId {
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.live-item-content.v1");
    hash.update(item.as_bytes());
    SyndicContentId::from_digest(hash.finalize().into())
}

fn put_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn encoded_chunk_ranges(
    encoded: &[u8],
    prepared: &[PreparedPiece],
) -> Result<Vec<(usize, usize)>, SyndicRecordError> {
    let mut ranges = Vec::new();
    let mut start = 0_usize;
    let mut piece_index = 0_usize;
    while start < encoded.len() {
        let mut end = start
            .checked_add(crate::CONTENT_CHUNK_MAX_BYTES)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "content chunk range",
            })?
            .min(encoded.len());
        if end < encoded.len() {
            while let Some(piece) = prepared.get(piece_index) {
                let (_, piece_end) = piece.encoded_range();
                if piece_end > end as u64 {
                    break;
                }
                piece_index += 1;
            }
            if let Some(PreparedPiece::Text(text)) = prepared.get(piece_index)
                && text.encoded_start < end as u64
                && (end as u64) < text.encoded_end
            {
                let text_start = usize::try_from(text.encoded_start).map_err(|_| {
                    SyndicRecordError::LengthOverflow {
                        kind: "content text chunk boundary",
                    }
                })?;
                let text_end = usize::try_from(text.encoded_end).map_err(|_| {
                    SyndicRecordError::LengthOverflow {
                        kind: "content text chunk boundary",
                    }
                })?;
                let source = std::str::from_utf8(
                    encoded
                        .get(text_start..text_end)
                        .ok_or(SyndicRecordError::InvalidContentEncoding)?,
                )
                .map_err(|_| SyndicRecordError::InvalidContentEncoding)?;
                let mut local_end = end - text_start;
                while !source.is_char_boundary(local_end) {
                    end -= 1;
                    local_end -= 1;
                }
            }
        }
        if end <= start {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        ranges.push((start, end));
        start = end;
    }
    Ok(ranges)
}

fn prepared_content_pieces(
    content_id: SyndicContentId,
    encoded: &[u8],
    chunk_ranges: &[(usize, usize)],
    prepared: &[PreparedPiece],
) -> Result<
    (
        Vec<crate::ContentTextSpanRecord>,
        Vec<crate::ContentPieceRecord>,
    ),
    SyndicRecordError,
> {
    let mut text_spans = Vec::new();
    let mut pieces = Vec::new();
    for prepared_piece in prepared {
        let PreparedPiece::Text(range) = prepared_piece else {
            let PreparedPiece::ImageMarker {
                atom_ordinal,
                marker_ordinal,
                logical_offset,
                encoded_start,
                encoded_end,
                marker_id,
                label,
            } = *prepared_piece
            else {
                unreachable!("prepared piece variants are exhaustive")
            };
            let start =
                usize::try_from(encoded_start).map_err(|_| SyndicRecordError::LengthOverflow {
                    kind: "composer marker range",
                })?;
            let end =
                usize::try_from(encoded_end).map_err(|_| SyndicRecordError::LengthOverflow {
                    kind: "composer marker range",
                })?;
            let source = encoded
                .get(start..end)
                .ok_or(SyndicRecordError::InvalidContentEncoding)?;
            let ordinal = next_content_piece_ordinal(pieces.len())?;
            pieces.push(crate::ContentPieceRecord::image_marker(
                content_id,
                ordinal,
                atom_ordinal,
                marker_ordinal,
                logical_offset,
                encoded_start,
                encoded_end,
                marker_id,
                label,
                Sha256::digest(source).into(),
            )?);
            continue;
        };
        let range_start = usize::try_from(range.encoded_start).map_err(|_| {
            SyndicRecordError::LengthOverflow {
                kind: "content text encoded range",
            }
        })?;
        let range_end =
            usize::try_from(range.encoded_end).map_err(|_| SyndicRecordError::LengthOverflow {
                kind: "content text encoded range",
            })?;
        let mut chunk_index = chunk_ranges.partition_point(|(_, end)| *end <= range_start);
        let mut logical_start = range.logical_start;
        let mut first = true;
        let mut encoded_start = range_start;
        while encoded_start < range_end {
            let &(chunk_start, chunk_end) = chunk_ranges
                .get(chunk_index)
                .ok_or(SyndicRecordError::InvalidContentEncoding)?;
            if encoded_start < chunk_start || encoded_start >= chunk_end {
                return Err(SyndicRecordError::InvalidContentEncoding);
            }
            let segment_end = chunk_end.min(range_end);
            let source = encoded
                .get(encoded_start..segment_end)
                .ok_or(SyndicRecordError::InvalidContentEncoding)?;
            std::str::from_utf8(source).map_err(|_| SyndicRecordError::InvalidContentEncoding)?;
            let logical_end = logical_start.checked_add(source.len() as u64).ok_or(
                SyndicRecordError::LengthOverflow {
                    kind: "content logical text range",
                },
            )?;
            let ordinal = next_content_piece_ordinal(pieces.len())?;
            let chunk_ordinal = ContentChunkOrdinal::new(
                u64::try_from(chunk_index)
                    .ok()
                    .and_then(|index| index.checked_add(1))
                    .ok_or(SyndicRecordError::LengthOverflow {
                        kind: "content text chunk ordinal",
                    })?,
            )
            .map_err(|_| SyndicRecordError::LengthOverflow {
                kind: "content text chunk ordinal",
            })?;
            let span = crate::ContentTextSpanRecord::new(
                content_id,
                ordinal,
                chunk_ordinal,
                chunk_start as u64,
                logical_start,
                logical_end,
                encoded_start as u64,
                segment_end as u64,
                first && range.break_before,
                Sha256::digest(source).into(),
            )?;
            text_spans.push(span);
            pieces.push(crate::ContentPieceRecord::text(span));
            encoded_start = segment_end;
            logical_start = logical_end;
            first = false;
            chunk_index += 1;
        }
        if logical_start != range.logical_end {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
    }
    Ok((text_spans, pieces))
}

fn next_content_piece_ordinal(
    current_count: usize,
) -> Result<crate::ContentPieceOrdinal, SyndicRecordError> {
    let ordinal = u64::try_from(current_count)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(SyndicRecordError::LengthOverflow {
            kind: "content pieces",
        })?;
    crate::ContentPieceOrdinal::new(ordinal).map_err(|_| SyndicRecordError::LengthOverflow {
        kind: "content pieces",
    })
}

pub(crate) fn decode_composer_content(bytes: &[u8]) -> Result<ComposerPayload, SyndicRecordError> {
    let mut decoder = ContentDecoder::new(bytes);
    if decoder.u8()? != 1 {
        return Err(SyndicRecordError::InvalidContentEncoding);
    }
    let count = decoder.u64()?;
    if count > u64::try_from(bytes.len()).unwrap_or(u64::MAX) {
        return Err(SyndicRecordError::InvalidContentEncoding);
    }
    let mut atoms = Vec::new();
    for _ in 0..count {
        atoms.push(match decoder.u8()? {
            0 => {
                let length = usize::try_from(decoder.u64()?)
                    .map_err(|_| SyndicRecordError::InvalidContentEncoding)?;
                let text = std::str::from_utf8(decoder.take(length)?)
                    .map_err(|_| SyndicRecordError::InvalidContentEncoding)?;
                ComposerAtom::text(text)?
            }
            1 => {
                let marker_id = SyndicDraftMarkerId::from_bytes(
                    decoder
                        .take(16)?
                        .try_into()
                        .map_err(|_| SyndicRecordError::InvalidContentEncoding)?,
                );
                let label = ImageLabelOrdinal::new(decoder.u64()?)
                    .map_err(|_| SyndicRecordError::InvalidContentEncoding)?;
                ComposerAtom::image_marker(marker_id, label)
            }
            _ => return Err(SyndicRecordError::InvalidContentEncoding),
        });
    }
    if !decoder.is_empty() {
        return Err(SyndicRecordError::InvalidContentEncoding);
    }
    ComposerPayload::new(atoms)
}

struct ContentDecoder<'a> {
    remaining: &'a [u8],
}

impl<'a> ContentDecoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], SyndicRecordError> {
        if self.remaining.len() < length {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        let (value, rest) = self.remaining.split_at(length);
        self.remaining = rest;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, SyndicRecordError> {
        Ok(self.take(1)?[0])
    }

    fn u64(&mut self) -> Result<u64, SyndicRecordError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| SyndicRecordError::InvalidContentEncoding)?,
        ))
    }

    const fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }
}
