use beryl_home_store::HomeStore;

use crate::{SyndicReadError, domain::SyndicStorage};

use super::{
    SyndicCaptureItem, SyndicContentTextRangeRead, SyndicPointReadLimit,
    content_text::{CONTENT_TEXT_MAX_PAYLOAD_BYTES, invalid_offset, read_text_page},
};

impl SyndicStorage {
    /// Reads one bounded logical UTF-8 page from an exact live-capture item snapshot.
    ///
    /// The CAS index, canonical item, and owned live or finalized manifest are stabilized before
    /// and after the page. Mutations of unrelated items do not invalidate this read.
    pub fn capture_item_text_range(
        &self,
        store: &HomeStore,
        item: &SyndicCaptureItem,
        start: u64,
        max_payload_bytes: usize,
        limit: SyndicPointReadLimit,
    ) -> Result<SyndicContentTextRangeRead, SyndicReadError> {
        if max_payload_bytes == 0 || max_payload_bytes > CONTENT_TEXT_MAX_PAYLOAD_BYTES {
            return Err(SyndicReadError::InvalidContentTextReadLimit {
                maximum: CONTENT_TEXT_MAX_PAYLOAD_BYTES,
                actual: max_payload_bytes,
            });
        }
        let source = item.item().cas_source().ok_or(SyndicReadError::Invariant(
            "live-capture text item has no CAS source",
        ))?;
        let before = self
            .capture_item(store, source, limit)?
            .ok_or_else(concurrent)?;
        if &before != item {
            return Err(concurrent());
        }
        let content = item
            .item()
            .payload()
            .content()
            .ok_or(SyndicReadError::CaptureItemHasNoTextContent)?;
        let content_bytes = content.summary().logical_utf8_bytes();
        if start > content_bytes {
            return Err(invalid_offset(content_bytes, start));
        }
        let (bytes, end, range_stored_bytes) = if start == content_bytes {
            (Vec::new(), start, 0)
        } else {
            let payload_bytes_u64 = u64::try_from(max_payload_bytes)
                .expect("the fixed content text payload bound fits u64");
            let desired_end = start.saturating_add(payload_bytes_u64).min(content_bytes);
            read_text_page(self, store, content, start, desired_end, max_payload_bytes)?
        };
        let after = self
            .capture_item(store, source, limit)?
            .ok_or_else(concurrent)?;
        if after != before {
            return Err(concurrent());
        }
        let stored_bytes = before
            .stored_bytes()
            .checked_add(range_stored_bytes)
            .and_then(|value| value.checked_add(after.stored_bytes()))
            .ok_or(SyndicReadError::Invariant(
                "live-capture text stored-byte accounting overflowed",
            ))?;
        let text = String::from_utf8(bytes)
            .map_err(|_| SyndicReadError::Invariant("live-capture text page is not valid UTF-8"))?
            .into_boxed_str();
        Ok(SyndicContentTextRangeRead::new(
            content,
            start,
            text,
            (end < content_bytes).then_some(end),
            stored_bytes,
        ))
    }
}

fn concurrent() -> SyndicReadError {
    SyndicReadError::ConcurrentChange {
        operation: "live-capture item text-range read",
    }
}
