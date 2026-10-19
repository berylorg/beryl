use super::ActivityDisplayReadError;
use crate::{
    ContentChunkRecord, SyndicStorage,
    codec::{ContentByteSpanKey, ContentByteSpansCodec, ContentChunkKey, ContentChunksCodec},
};
use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore, PointReadLimit};
use beryl_model::SyndicContentId;

#[derive(Default)]
pub(super) struct Window {
    start: u64,
    chunk: Option<ContentChunkRecord>,
}
impl Window {
    pub(super) fn byte(&self, at: u64) -> Option<u8> {
        let offset = usize::try_from(at.checked_sub(self.start)?).ok()?;
        self.chunk.as_ref()?.bytes().get(offset).copied()
    }
    pub(super) fn load(
        &mut self,
        home: &HomeStore,
        storage: &SyndicStorage,
        owner: SyndicContentId,
        at: u64,
        frontier: u64,
    ) -> Result<(), ActivityDisplayReadError> {
        if at >= frontier {
            return Err(ActivityDisplayReadError::Invalid(
                "source range exceeds sealed frontier",
            ));
        }
        let page = home.read_cursor::<crate::domain::SyndicDomain, ContentByteSpansCodec>(
            &storage.handle,
            &CursorRange::closed(
                ContentByteSpanKey { owner, start: 0 },
                ContentByteSpanKey { owner, start: at },
            ),
            CursorDirection::Reverse,
            CursorReadLimits::new(1, 512).expect("fixed byte predecessor limit"),
        )?;
        let span = page
            .records()
            .first()
            .ok_or(ActivityDisplayReadError::Invalid(
                "missing source byte span",
            ))?
            .value();
        if span.content_id() != owner || !(span.start() <= at && at < span.end()) {
            return Err(ActivityDisplayReadError::Invalid(
                "invalid source byte span",
            ));
        }
        let chunk = home
            .read_point::<crate::domain::SyndicDomain, ContentChunksCodec>(
                &storage.handle,
                &ContentChunkKey {
                    owner,
                    ordinal: span.ordinal(),
                },
                PointReadLimit::new(65_792).expect("fixed chunk limit"),
            )?
            .ok_or(ActivityDisplayReadError::Invalid("missing source chunk"))?;
        if chunk.content_id() != owner
            || chunk.ordinal() != span.ordinal()
            || *chunk.digest() != span.chunk_digest()
            || chunk.bytes().len() as u64 != span.len()
        {
            return Err(ActivityDisplayReadError::Invalid(
                "source chunk and span disagree",
            ));
        }
        self.start = span.start();
        self.chunk = Some(chunk);
        Ok(())
    }
}
