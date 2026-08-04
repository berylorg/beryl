use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};

use crate::{
    ContentPieceOrdinal, ContentPieceRecord, ContentReference, ContentTextSpanRecord,
    SyndicReadError,
    codec::{
        ContentPieceKey, ContentPiecesCodec, ContentPiecesFamily, ContentTextSpanKey,
        ContentTextSpansFamily,
    },
    domain::SyndicStorage,
};

use super::{super::family_limit, SyndicContentTextSegmentBoundary, marker::authenticate_marker};

const PIECE_PAGE_ITEMS: usize = 256;
const PIECE_PAGE_BYTES: usize = 65_536;

pub(super) struct ValidatedSegment {
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) preceding_marker: Option<SyndicContentTextSegmentBoundary>,
    pub(super) following_marker: Option<SyndicContentTextSegmentBoundary>,
}

pub(super) fn validate_segment(
    storage: &SyndicStorage,
    store: &HomeStore,
    content: ContentReference,
    after_marker: Option<SyndicContentTextSegmentBoundary>,
) -> Result<ValidatedSegment, SyndicReadError> {
    let summary = content.summary();
    let preceding_marker = match after_marker {
        None => None,
        Some(expected_boundary) => {
            let ordinal = expected_boundary.piece_ordinal();
            if ordinal.get() > summary.piece_count() {
                return Err(invalid_cursor(content, ordinal));
            }
            let record = storage
                .point::<ContentPiecesFamily>(
                    store,
                    ContentPieceKey {
                        owner: content.id(),
                        ordinal,
                    },
                    family_limit::<ContentPiecesFamily>(),
                )?
                .ok_or(SyndicReadError::Invariant(
                    "content text segment preceding marker piece is missing",
                ))?;
            let piece = record;
            if piece.content_id() != content.id() || piece.ordinal() != ordinal {
                return Err(SyndicReadError::Invariant(
                    "content text segment preceding marker key disagrees",
                ));
            }
            if !matches!(piece, ContentPieceRecord::ImageMarker { .. }) {
                return Err(invalid_cursor(content, ordinal));
            }
            let (boundary, _) = authenticate_marker(storage, store, content, piece)?;
            if boundary != expected_boundary {
                return Err(SyndicReadError::Invariant(
                    "content text segment preceding marker proof disagrees",
                ));
            }
            Some(boundary)
        }
    };
    let start = preceding_marker.map_or(0, |marker| marker.logical_offset());
    let next_piece = next_piece(after_marker, summary.piece_count())?;
    let Some(next_piece) = next_piece else {
        validate_eof(content, preceding_marker, start)?;
        return Ok(ValidatedSegment {
            start,
            end: start,
            preceding_marker,
            following_marker: None,
        });
    };

    let scanned = scan_to_boundary(storage, store, content, preceding_marker, next_piece, start)?;
    Ok(ValidatedSegment {
        start,
        end: scanned.end,
        preceding_marker,
        following_marker: scanned.following_marker,
    })
}

struct ScannedSegment {
    end: u64,
    following_marker: Option<SyndicContentTextSegmentBoundary>,
}

fn scan_to_boundary(
    storage: &SyndicStorage,
    store: &HomeStore,
    content: ContentReference,
    preceding_marker: Option<SyndicContentTextSegmentBoundary>,
    first_piece: ContentPieceOrdinal,
    start: u64,
) -> Result<ScannedSegment, SyndicReadError> {
    let summary = content.summary();
    let last_piece = ContentPieceOrdinal::new(summary.piece_count())
        .map_err(|_| SyndicReadError::Invariant("sealed content piece frontier is invalid"))?;
    let expected_marker = preceding_marker
        .map_or(Some(1), |marker| {
            marker.marker_ordinal().get().checked_add(1)
        })
        .ok_or(SyndicReadError::Invariant(
            "content text segment marker order exhausted",
        ))?;
    let mut expected_piece = first_piece;
    let mut after = None;
    let mut logical = start;
    let mut first_text = true;

    loop {
        let range = match after {
            Some(previous) => CursorRange::after(
                ContentPieceKey {
                    owner: content.id(),
                    ordinal: previous,
                },
                ContentPieceKey {
                    owner: content.id(),
                    ordinal: last_piece,
                },
            ),
            None => CursorRange::closed(
                ContentPieceKey {
                    owner: content.id(),
                    ordinal: first_piece,
                },
                ContentPieceKey {
                    owner: content.id(),
                    ordinal: last_piece,
                },
            ),
        };
        let page = store.read_cursor::<crate::domain::SyndicDomain, ContentPiecesCodec>(
            storage.handle,
            &range,
            CursorDirection::Forward,
            CursorReadLimits::new(PIECE_PAGE_ITEMS, PIECE_PAGE_BYTES)
                .expect("content segment piece-page bounds are nonzero"),
        )?;
        if page.records().is_empty() {
            return Err(SyndicReadError::Invariant(
                "content text segment ordered pieces have a gap",
            ));
        }

        for record in page.records() {
            let piece = *record.value();
            validate_piece_key(content, record.key(), piece, expected_piece)?;
            match piece {
                ContentPieceRecord::Text(span) => {
                    validate_text_piece(
                        storage,
                        store,
                        content,
                        span,
                        logical,
                        first_text,
                        preceding_marker.is_some(),
                    )?;
                    logical = span.logical_end();
                    first_text = false;
                }
                ContentPieceRecord::ImageMarker { .. } => {
                    let (boundary, _) = authenticate_marker(storage, store, content, piece)?;
                    if boundary.logical_offset() != logical {
                        return Err(SyndicReadError::Invariant(
                            "content text segment marker logical order disagrees",
                        ));
                    }
                    if boundary.marker_ordinal().get() != expected_marker {
                        return Err(SyndicReadError::Invariant(
                            "content text segment marker ordinal is not contiguous",
                        ));
                    }
                    return Ok(ScannedSegment {
                        end: logical,
                        following_marker: Some(boundary),
                    });
                }
            }
            after = Some(expected_piece);
            expected_piece = expected_piece.checked_next().map_err(|_| {
                SyndicReadError::Invariant("content text segment piece order exhausted")
            })?;
        }
        if !page.has_more() {
            validate_eof(content, preceding_marker, logical)?;
            return Ok(ScannedSegment {
                end: logical,
                following_marker: None,
            });
        }
    }
}

fn validate_piece_key(
    content: ContentReference,
    key: &ContentPieceKey,
    piece: ContentPieceRecord,
    expected: ContentPieceOrdinal,
) -> Result<(), SyndicReadError> {
    let summary = content.summary();
    if key.owner != content.id()
        || key.ordinal != expected
        || piece.content_id() != content.id()
        || piece.ordinal() != expected
        || expected.get() > summary.piece_count()
        || piece.logical_offset() > summary.logical_utf8_bytes()
        || piece.encoded_end() > summary.encoded_bytes()
    {
        return Err(SyndicReadError::Invariant(
            "content text segment piece key or contiguous frontier disagrees",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_text_piece(
    storage: &SyndicStorage,
    store: &HomeStore,
    content: ContentReference,
    span: ContentTextSpanRecord,
    logical: u64,
    first_text: bool,
    has_preceding_marker: bool,
) -> Result<(), SyndicReadError> {
    let summary = content.summary();
    if span.logical_start() != logical
        || span.logical_end() > summary.logical_utf8_bytes()
        || span.encoded_end() > summary.encoded_bytes()
        || span.chunk_ordinal().get() > summary.chunk_count()
        || span.piece_ordinal().get() > summary.piece_count()
        || span.break_before() != (first_text && has_preceding_marker)
    {
        return Err(SyndicReadError::Invariant(
            "content text segment text piece frontier disagrees",
        ));
    }
    let indexed = storage
        .point::<ContentTextSpansFamily>(
            store,
            ContentTextSpanKey {
                owner: content.id(),
                logical_start: span.logical_start(),
            },
            family_limit::<ContentTextSpansFamily>(),
        )?
        .ok_or(SyndicReadError::Invariant(
            "content text segment text piece index is missing",
        ))?;
    if indexed != span {
        return Err(SyndicReadError::Invariant(
            "content text segment text piece and offset index disagree",
        ));
    }
    Ok(())
}

fn validate_eof(
    content: ContentReference,
    preceding_marker: Option<SyndicContentTextSegmentBoundary>,
    logical: u64,
) -> Result<(), SyndicReadError> {
    let summary = content.summary();
    let marker_frontier = preceding_marker.map_or(0, |marker| marker.marker_ordinal().get());
    if logical != summary.logical_utf8_bytes() || marker_frontier != summary.image_marker_count() {
        return Err(SyndicReadError::Invariant(
            "content text segment EOF disagrees with its content frontier",
        ));
    }
    Ok(())
}

fn next_piece(
    after_marker: Option<SyndicContentTextSegmentBoundary>,
    piece_count: u64,
) -> Result<Option<ContentPieceOrdinal>, SyndicReadError> {
    match after_marker {
        None if piece_count == 0 => Ok(None),
        None => Ok(Some(ContentPieceOrdinal::FIRST)),
        Some(after) if after.piece_ordinal().get() == piece_count => Ok(None),
        Some(after) => after
            .piece_ordinal()
            .checked_next()
            .map(Some)
            .map_err(|_| SyndicReadError::Invariant("content segment piece cursor exhausted")),
    }
}

fn invalid_cursor(content: ContentReference, after_piece: ContentPieceOrdinal) -> SyndicReadError {
    SyndicReadError::InvalidContentTextSegmentCursor {
        piece_count: content.summary().piece_count(),
        after_piece: after_piece.get(),
    }
}
