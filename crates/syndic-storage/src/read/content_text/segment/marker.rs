use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use sha2::{Digest, Sha256};

use crate::{
    ContentByteSpanRecord, ContentChunkRecord, ContentPieceRecord, ContentReference,
    SyndicReadError,
    codec::{ContentByteSpanKey, ContentByteSpansCodec, ContentChunkKey, ContentChunksFamily},
    domain::SyndicStorage,
};

use super::{super::family_limit, SyndicContentTextSegmentBoundary};

const ENCODED_MARKER_BYTES: usize = 25;
const MARKER_RANGE_PAGE_ITEMS: usize = 256;
const MARKER_RANGE_PAGE_BYTES: usize = 65_536;

pub(super) fn authenticate_marker(
    storage: &SyndicStorage,
    store: &HomeStore,
    content: ContentReference,
    piece: ContentPieceRecord,
) -> Result<(SyndicContentTextSegmentBoundary, usize), SyndicReadError> {
    let ContentPieceRecord::ImageMarker {
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
    } = piece
    else {
        return Err(SyndicReadError::Invariant(
            "content text segment boundary is not an image marker",
        ));
    };
    let summary = content.summary();
    if content.encoding() != crate::ContentEncoding::ComposerV1
        || content_id != content.id()
        || ordinal.get() > summary.piece_count()
        || atom_ordinal.get() > summary.atom_count()
        || marker_ordinal.get() > summary.image_marker_count()
        || logical_offset > summary.logical_utf8_bytes()
        || encoded_start >= encoded_end
        || encoded_end > summary.encoded_bytes()
        || encoded_end - encoded_start != ENCODED_MARKER_BYTES as u64
    {
        return Err(SyndicReadError::Invariant(
            "content text segment marker frontier disagrees",
        ));
    }

    let (actual, stored_bytes) =
        read_marker_bytes(storage, store, content, encoded_start, encoded_end)?;
    let mut expected = [0_u8; ENCODED_MARKER_BYTES];
    expected[0] = 1;
    expected[1..17].copy_from_slice(marker_id.as_bytes());
    expected[17..].copy_from_slice(&label.get().to_be_bytes());
    let actual_digest: [u8; 32] = Sha256::digest(actual).into();
    if actual != expected || actual_digest != digest {
        return Err(SyndicReadError::Invariant(
            "content text segment marker bytes or digest disagree",
        ));
    }

    Ok((
        SyndicContentTextSegmentBoundary {
            piece_ordinal: ordinal,
            marker_ordinal,
            logical_offset,
            marker_id,
            label,
        },
        stored_bytes,
    ))
}

fn read_marker_bytes(
    storage: &SyndicStorage,
    store: &HomeStore,
    content: ContentReference,
    start: u64,
    end: u64,
) -> Result<([u8; ENCODED_MARKER_BYTES], usize), SyndicReadError> {
    let predecessor = store.read_cursor::<crate::domain::SyndicDomain, ContentByteSpansCodec>(
        storage.handle,
        &CursorRange::closed(
            ContentByteSpanKey {
                owner: content.id(),
                start: 0,
            },
            ContentByteSpanKey {
                owner: content.id(),
                start,
            },
        ),
        CursorDirection::Reverse,
        CursorReadLimits::new(1, 512).expect("marker byte predecessor bounds are nonzero"),
    )?;
    let mut stored_bytes = predecessor.stored_bytes();
    let Some(first) = predecessor.records().first() else {
        return Err(SyndicReadError::Invariant(
            "content text segment marker has no byte-span predecessor",
        ));
    };
    validate_span_key(content, first.key(), *first.value())?;

    let mut output = [0_u8; ENCODED_MARKER_BYTES];
    let mut written = 0_usize;
    let mut encoded = start;
    let mut after_start = first.value().start();
    let mut pending = Some(*first.value());
    while encoded < end {
        if let Some(span) = pending.take() {
            append_span(
                storage,
                store,
                content,
                span,
                &mut encoded,
                end,
                &mut output,
                &mut written,
            )?;
            if encoded == end {
                break;
            }
        }
        let page = store.read_cursor::<crate::domain::SyndicDomain, ContentByteSpansCodec>(
            storage.handle,
            &CursorRange::after(
                ContentByteSpanKey {
                    owner: content.id(),
                    start: after_start,
                },
                ContentByteSpanKey {
                    owner: content.id(),
                    start: end - 1,
                },
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(MARKER_RANGE_PAGE_ITEMS, MARKER_RANGE_PAGE_BYTES)
                .expect("marker byte page bounds are nonzero"),
        )?;
        add_stored(&mut stored_bytes, page.stored_bytes())?;
        if page.records().is_empty() {
            return Err(SyndicReadError::Invariant(
                "content text segment marker byte spans have a gap",
            ));
        }
        for record in page.records() {
            let span = *record.value();
            validate_span_key(content, record.key(), span)?;
            after_start = span.start();
            append_span(
                storage,
                store,
                content,
                span,
                &mut encoded,
                end,
                &mut output,
                &mut written,
            )?;
            if encoded == end {
                break;
            }
        }
    }
    if written != ENCODED_MARKER_BYTES {
        return Err(SyndicReadError::Invariant(
            "content text segment marker byte length disagrees",
        ));
    }
    Ok((output, stored_bytes))
}

fn validate_span_key(
    content: ContentReference,
    key: &ContentByteSpanKey,
    span: ContentByteSpanRecord,
) -> Result<(), SyndicReadError> {
    if key.owner != content.id()
        || key.start != span.start()
        || span.content_id() != content.id()
        || span.end() > content.summary().encoded_bytes()
    {
        return Err(SyndicReadError::Invariant(
            "content text segment marker byte-span frontier disagrees",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_span(
    storage: &SyndicStorage,
    store: &HomeStore,
    content: ContentReference,
    span: ContentByteSpanRecord,
    encoded: &mut u64,
    end: u64,
    output: &mut [u8; ENCODED_MARKER_BYTES],
    written: &mut usize,
) -> Result<(), SyndicReadError> {
    if span.content_id() != content.id() || span.start() > *encoded || span.end() <= *encoded {
        return Err(SyndicReadError::Invariant(
            "content text segment marker byte spans are not contiguous",
        ));
    }
    let chunk = storage
        .point::<ContentChunksFamily>(
            store,
            ContentChunkKey {
                owner: content.id(),
                ordinal: span.ordinal(),
            },
            family_limit::<ContentChunksFamily>(),
        )?
        .ok_or(SyndicReadError::Invariant(
            "content text segment marker chunk is missing",
        ))?;
    validate_chunk(content, span, &chunk)?;

    let selected_end = end.min(span.end());
    let local_start = usize::try_from(*encoded - span.start())
        .map_err(|_| SyndicReadError::Invariant("content marker chunk offset overflowed"))?;
    let local_end = usize::try_from(selected_end - span.start())
        .map_err(|_| SyndicReadError::Invariant("content marker chunk offset overflowed"))?;
    let source = chunk
        .bytes()
        .get(local_start..local_end)
        .ok_or(SyndicReadError::Invariant(
            "content text segment marker lies outside its chunk",
        ))?;
    let output_end = written
        .checked_add(source.len())
        .ok_or(SyndicReadError::Invariant(
            "content text segment marker byte accounting overflowed",
        ))?;
    output
        .get_mut(*written..output_end)
        .ok_or(SyndicReadError::Invariant(
            "content text segment marker exceeds its canonical size",
        ))?
        .copy_from_slice(source);
    *written = output_end;
    *encoded = selected_end;
    Ok(())
}

fn validate_chunk(
    content: ContentReference,
    span: ContentByteSpanRecord,
    chunk: &ContentChunkRecord,
) -> Result<(), SyndicReadError> {
    if chunk.content_id() != content.id()
        || chunk.ordinal() != span.ordinal()
        || chunk.digest() != &span.chunk_digest()
        || u64::try_from(chunk.bytes().len()).ok() != Some(span.len())
    {
        return Err(SyndicReadError::Invariant(
            "content text segment marker chunk disagrees with its byte span",
        ));
    }
    Ok(())
}

fn add_stored(total: &mut usize, value: usize) -> Result<(), SyndicReadError> {
    *total = total.checked_add(value).ok_or(SyndicReadError::Invariant(
        "logical content text segment stored-byte accounting overflowed",
    ))?;
    Ok(())
}
