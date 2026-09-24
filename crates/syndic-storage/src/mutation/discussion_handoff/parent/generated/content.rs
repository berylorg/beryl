use super::*;
use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits};

pub(super) fn closure_matches(
    reader: &ParentRead<'_>,
    changes: &[Change],
) -> Result<(bool, bool), ReadError> {
    let Some((owner, existed)) = changes.iter().find_map(|change| match change {
        Change::Manifest { key, old, .. } => Some((*key, old.is_some())),
        _ => None,
    }) else {
        return Ok((false, false));
    };
    let mut old = true;
    let mut new = true;
    macro_rules! inspect {
        ($variant:ident, $family:ty, $first:expr, $last:expr) => {{
            let expected = changes
                .iter()
                .filter(|change| matches!(change, Change::$variant { .. }))
                .count();
            let page = reader
                .access
                .read_cursor::<SyndicDomain, ExactCodec<$family>>(
                    &reader.storage.handle,
                    &CursorRange::closed($first, $last),
                    CursorDirection::Forward,
                    CursorReadLimits::new(expected + 1, 600_000)
                        .expect("bounded generated content closure"),
                )?;
            let complete = !page.has_more();
            old &= complete && page.records().len() == if existed { expected } else { 0 };
            new &= complete && page.records().len() == expected;
        }};
    }
    inspect!(
        Chunk,
        ContentChunksFamily,
        ContentChunkKey {
            owner,
            ordinal: ContentChunkOrdinal::FIRST
        },
        ContentChunkKey {
            owner,
            ordinal: ContentChunkOrdinal::new(u64::MAX).expect("nonzero ordinal")
        }
    );
    inspect!(
        ByteSpan,
        ContentByteSpansFamily,
        ContentByteSpanKey { owner, start: 0 },
        ContentByteSpanKey {
            owner,
            start: u64::MAX
        }
    );
    inspect!(
        TextSpan,
        ContentTextSpansFamily,
        ContentTextSpanKey {
            owner,
            logical_start: 0
        },
        ContentTextSpanKey {
            owner,
            logical_start: u64::MAX
        }
    );
    inspect!(
        Piece,
        ContentPiecesFamily,
        ContentPieceKey {
            owner,
            ordinal: ContentPieceOrdinal::FIRST
        },
        ContentPieceKey {
            owner,
            ordinal: ContentPieceOrdinal::new(u64::MAX).expect("nonzero ordinal")
        }
    );
    Ok((old, new))
}

pub(super) fn prepare_content(
    reader: &ParentRead<'_>,
    content: PreparedContent,
    changes: &mut Vec<Change>,
) -> Result<ContentReference, SyndicMutationError> {
    let owner = content.id();
    let old = reader.read::<ContentManifestsFamily>(&owner)?;
    let revision = old
        .as_ref()
        .map_or(ContentRevision::new(1)?, |manifest| manifest.revision());
    let manifest = content.sealed_manifest(revision);
    if old.as_ref().is_some_and(|old| old != &manifest) {
        return Err(SyndicMutationError::ContentIdentityCollision);
    }
    let exists = old.is_some();
    changes.push(Change::Manifest {
        key: owner,
        old,
        new: Some(manifest),
    });
    macro_rules! content_record {
        ($variant:ident, $family:ty, $key:expr, $value:expr) => {{
            let key = $key;
            let new = $value;
            let old = reader.read::<$family>(&key)?;
            if old.as_ref() != exists.then_some(&new) {
                return Err(SyndicMutationError::ContentIdentityCollision);
            }
            changes.push(Change::$variant {
                key,
                old,
                new: Some(new),
            });
        }};
    }
    let spans = content_byte_spans(content.chunks(), 0)?;
    for chunk in content.chunks() {
        content_record!(
            Chunk,
            ContentChunksFamily,
            ContentChunkKey {
                owner,
                ordinal: chunk.ordinal()
            },
            chunk.clone()
        );
    }
    for span in spans {
        content_record!(
            ByteSpan,
            ContentByteSpansFamily,
            ContentByteSpanKey {
                owner,
                start: span.start()
            },
            span
        );
    }
    for span in content.text_spans() {
        content_record!(
            TextSpan,
            ContentTextSpansFamily,
            ContentTextSpanKey {
                owner,
                logical_start: span.logical_start()
            },
            *span
        );
    }
    for piece in content.pieces() {
        content_record!(
            Piece,
            ContentPiecesFamily,
            ContentPieceKey {
                owner,
                ordinal: piece.ordinal()
            },
            *piece
        );
    }
    if !super::content::closure_matches(reader, changes)?.0 {
        return Err(SyndicMutationError::ContentIdentityCollision);
    }
    Ok(content.reference(revision))
}
