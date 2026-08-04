use beryl_backend::{
    STREAMED_TEXT_MAX_PAGE_BYTES, StreamedInputSourceError, StreamedTextPage, StreamedTextSourceId,
};
use beryl_home_store::HomeStore;
use syndic_storage::SyndicStorage;

use super::prepared::{TextReplayAuthority, map_read_error};
use crate::cas_projection::ProjectionCancellationToken;

impl TextReplayAuthority {
    pub(in crate::cas_projection) fn read_page(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        self.check_page_authority(store, storage, cancellation, source_id)?;
        if max_utf8_bytes == 0 || max_utf8_bytes > STREAMED_TEXT_MAX_PAGE_BYTES {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        let utf8_len = self.content().summary().logical_utf8_bytes();
        if start >= utf8_len {
            return Err(StreamedInputSourceError::PagePastEnd {
                end: start,
                utf8_len,
            });
        }
        let page = storage
            .sealed_content_text_range(store, self.content(), start, max_utf8_bytes)
            .map_err(map_read_error)?
            .ok_or(StreamedInputSourceError::ReadFailed)?;
        if page.content() != self.content() {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        if page.start() != start {
            return Err(StreamedInputSourceError::PageStartMismatch {
                expected: start,
                actual: page.start(),
            });
        }
        let page_bytes = page.text().len();
        if page_bytes == 0 {
            return Err(StreamedInputSourceError::EmptyPage { start });
        }
        if page_bytes > max_utf8_bytes {
            return Err(StreamedInputSourceError::PageTooLarge {
                maximum: max_utf8_bytes,
                actual: page_bytes,
            });
        }
        let page_bytes_u64 = u64::try_from(page_bytes)
            .map_err(|_| StreamedInputSourceError::PageEndOverflow { start, page_bytes })?;
        let end = start
            .checked_add(page_bytes_u64)
            .ok_or(StreamedInputSourceError::PageEndOverflow { start, page_bytes })?;
        if end > utf8_len {
            return Err(StreamedInputSourceError::PagePastEnd { end, utf8_len });
        }
        let next_offset = page.next_offset();
        match (end < utf8_len, next_offset) {
            (true, None) => {
                return Err(StreamedInputSourceError::PrematureEof { end, utf8_len });
            }
            (true, Some(next_offset)) if next_offset != end => {
                return Err(StreamedInputSourceError::InvalidNextOffset {
                    end,
                    next_offset,
                    utf8_len,
                });
            }
            (false, Some(next_offset)) => {
                return Err(StreamedInputSourceError::InvalidNextOffset {
                    end,
                    next_offset,
                    utf8_len,
                });
            }
            (true, Some(_)) | (false, None) => {}
        }
        Ok(streamed_text_page(
            self,
            start,
            page.into_text(),
            next_offset,
        ))
    }
}

fn streamed_text_page(
    authority: &TextReplayAuthority,
    start: u64,
    text: Box<str>,
    next_offset: Option<u64>,
) -> StreamedTextPage {
    StreamedTextPage::new(
        authority.header().source_identity(),
        authority.header().source_revision(),
        authority.source_id(),
        authority.proof(),
        start,
        text,
        next_offset,
    )
}
