use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader, HomeStore};
use beryl_model::SyndicContentId;

use crate::{
    ContentByteSpanRecord, ContentLifecycle, ContentPieceOrdinal, ContentPieceRecord,
    ContentReference, SyndicPointReadLimit, SyndicReadError, SyndicStorage, codec::*,
    domain::SyndicDomain,
};

pub(super) trait SourceReader {
    fn point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, SyndicReadError>;
    fn byte_span(
        &self,
        owner: SyndicContentId,
        cursor: u64,
    ) -> Result<ContentByteSpanRecord, SyndicReadError>;
}

pub(super) struct StoreSourceReader<'a> {
    pub storage: &'a SyndicStorage,
    pub store: &'a HomeStore,
}

fn byte_range(owner: SyndicContentId, cursor: u64) -> CursorRange<ContentByteSpanKey> {
    CursorRange::closed(
        ContentByteSpanKey { owner, start: 0 },
        ContentByteSpanKey {
            owner,
            start: cursor,
        },
    )
}

impl SourceReader for StoreSourceReader<'_> {
    fn point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, SyndicReadError> {
        self.storage.point::<F>(
            self.store,
            key.clone(),
            SyndicPointReadLimit::new(F::MAX_VALUE_BYTES + 4).unwrap(),
        )
    }
    fn byte_span(
        &self,
        owner: SyndicContentId,
        cursor: u64,
    ) -> Result<ContentByteSpanRecord, SyndicReadError> {
        let page = self
            .store
            .read_cursor::<SyndicDomain, ContentByteSpansCodec>(
                &self.storage.handle,
                &byte_range(owner, cursor),
                CursorDirection::Reverse,
                CursorReadLimits::new(1, 512).unwrap(),
            )?;
        page.records()
            .first()
            .map(|r| *r.value())
            .ok_or(SyndicReadError::Invariant(
                "import source byte span missing",
            ))
    }
}

impl SourceReader for DomainReader<'_, SyndicDomain> {
    fn point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, SyndicReadError> {
        self.point::<ExactCodec<F>>(key, family_point_limit::<F>())
            .map_err(Into::into)
    }
    fn byte_span(
        &self,
        owner: SyndicContentId,
        cursor: u64,
    ) -> Result<ContentByteSpanRecord, SyndicReadError> {
        let page = self.cursor::<ContentByteSpansCodec>(
            &byte_range(owner, cursor),
            CursorDirection::Reverse,
            CursorReadLimits::new(1, 512).unwrap(),
        )?;
        page.records()
            .first()
            .map(|r| *r.value())
            .ok_or(SyndicReadError::Invariant(
                "import source byte span missing",
            ))
    }
}

pub(super) fn authenticate_manifest(
    reader: &impl SourceReader,
    content: ContentReference,
) -> Result<(), SyndicReadError> {
    let manifest = reader
        .point::<ContentManifestsFamily>(&content.id())?
        .ok_or(SyndicReadError::Invariant("import source manifest missing"))?;
    if manifest.id() != content.id()
        || manifest.revision() != content.revision()
        || manifest.encoding() != content.encoding()
        || manifest.lifecycle() != ContentLifecycle::Sealed
        || manifest.expected() != content.summary()
    {
        return Err(SyndicReadError::Invariant(
            "import source is not exact sealed content",
        ));
    }
    Ok(())
}

pub(super) fn encoded_range(
    reader: &impl SourceReader,
    content: ContentReference,
    start: u64,
    length: usize,
) -> Result<Vec<u8>, SyndicReadError> {
    if length > crate::CONTENT_CHUNK_MAX_BYTES + 4 {
        return Err(SyndicReadError::Invariant("import source window oversized"));
    }
    let end = start
        .checked_add(length as u64)
        .ok_or(SyndicReadError::Invariant("import source range overflow"))?;
    if end > content.summary().encoded_bytes() {
        return Err(SyndicReadError::Invariant(
            "import source range exceeds seal",
        ));
    }
    let mut cursor = start;
    let mut output = Vec::with_capacity(length);
    let mut reads = 0;
    while cursor < end {
        reads += 1;
        if reads > 32 {
            return Err(SyndicReadError::Invariant(
                "import source framing window exceeded read bound",
            ));
        }
        let span = reader.byte_span(content.id(), cursor)?;
        if span.content_id() != content.id()
            || span.start() > cursor
            || span.end() <= cursor
            || span.end() > content.summary().encoded_bytes()
            || span.ordinal().get() > content.summary().chunk_count()
        {
            return Err(SyndicReadError::Invariant(
                "import source byte span does not contain exact cursor",
            ));
        }
        let chunk = reader
            .point::<ContentChunksFamily>(&ContentChunkKey {
                owner: content.id(),
                ordinal: span.ordinal(),
            })?
            .ok_or(SyndicReadError::Invariant("import source chunk missing"))?;
        if chunk.content_id() != content.id()
            || chunk.ordinal() != span.ordinal()
            || span.len() != chunk.bytes().len() as u64
            || span.chunk_digest() != *chunk.digest()
        {
            return Err(SyndicReadError::Invariant(
                "import source chunk/span disagreement",
            ));
        }
        let next = span.end().min(end);
        output.extend_from_slice(
            &chunk.bytes()[(cursor - span.start()) as usize..(next - span.start()) as usize],
        );
        cursor = next;
    }
    Ok(output)
}

pub(super) fn source_piece(
    reader: &impl SourceReader,
    content: ContentReference,
    ordinal: u64,
) -> Result<ContentPieceRecord, SyndicReadError> {
    let ordinal = ContentPieceOrdinal::new(ordinal)
        .map_err(|_| SyndicReadError::Invariant("import source piece ordinal invalid"))?;
    let piece = reader
        .point::<ContentPiecesFamily>(&ContentPieceKey {
            owner: content.id(),
            ordinal,
        })?
        .ok_or(SyndicReadError::Invariant("import source piece missing"))?;
    if piece.content_id() != content.id()
        || piece.ordinal() != ordinal
        || piece.encoded_end() > content.summary().encoded_bytes()
    {
        return Err(SyndicReadError::Invariant(
            "import source piece identity disagreement",
        ));
    }
    Ok(piece)
}
